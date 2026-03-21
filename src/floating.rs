//! Shared rendering for floating cards (detached cards and `ctx.float()`).
//!
//! Each floating card is an `egui::Area` that manages its own position,
//! anchor mode (content-scrolling vs viewport-fixed), drag handle, and chrome.

use eframe::egui;

use crate::card_ctx::{CardCtx, GRID_ROW_MODULE};
use crate::state;
use crate::themes;

// ── scroll info (written by the notebook, read by floats) ────────────

/// Scroll state published by the notebook each frame so floating cards
/// can convert between content and screen coordinates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NotebookScrollInfo {
    pub scroll_y: f32,
    pub viewport_top: f32,
    pub clip_rect: egui::Rect,
}

const SCROLL_INFO_ID: &str = "gorbie_notebook_scroll_info";

pub(crate) fn store_scroll_info(ctx: &egui::Context, info: NotebookScrollInfo) {
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::new(SCROLL_INFO_ID), info);
        // Reset the float extent tracker each frame.
        d.insert_temp(egui::Id::new(FLOAT_MAX_BOTTOM_ID), 0.0f32);
    });
}

const FLOAT_MAX_BOTTOM_ID: &str = "gorbie_float_max_content_bottom";

/// Returns the maximum content-space bottom Y across all floating cards this frame.
pub(crate) fn max_float_content_bottom(ctx: &egui::Context) -> f32 {
    ctx.data(|d| d.get_temp(egui::Id::new(FLOAT_MAX_BOTTOM_ID)).unwrap_or(0.0))
}

fn record_float_extent(ctx: &egui::Context, content_bottom: f32) {
    ctx.data_mut(|d| {
        let current: f32 = d.get_temp(egui::Id::new(FLOAT_MAX_BOTTOM_ID)).unwrap_or(0.0);
        if content_bottom > current {
            d.insert_temp(egui::Id::new(FLOAT_MAX_BOTTOM_ID), content_bottom);
        }
    });
}

fn read_scroll_info(ctx: &egui::Context) -> Option<NotebookScrollInfo> {
    ctx.data(|d| d.get_temp(egui::Id::new(SCROLL_INFO_ID)))
}

// ── anchor mode ──────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum Anchor {
    /// Scrolls with notebook content.
    Content,
    /// Fixed to viewport (sticks when dragged past right edge).
    Viewport,
}

/// Persisted state for a floating card.
#[derive(Clone, Copy, Debug, Default)]
struct FloatState {
    pos: egui::Pos2,
    anchor: Anchor,
}

impl Default for Anchor {
    fn default() -> Self {
        Anchor::Content
    }
}

const STICK_RIGHT_OUTSIDE_RATIO: f32 = 0.5;
const UNSTICK_RIGHT_OUTSIDE_RATIO: f32 = 0.4;

fn right_outside_ratio(rect: egui::Rect, viewport: egui::Rect) -> f32 {
    let width = rect.width().max(0.0);
    if width <= 0.0 {
        return 1.0;
    }
    let outside_width = (rect.right() - viewport.right()).max(0.0);
    (outside_width / width).clamp(0.0, 1.0)
}

fn content_to_screen(pos: egui::Pos2, info: &NotebookScrollInfo) -> egui::Pos2 {
    egui::pos2(pos.x, pos.y - info.scroll_y + info.viewport_top)
}

fn screen_to_content(pos: egui::Pos2, info: &NotebookScrollInfo) -> egui::Pos2 {
    egui::pos2(pos.x, pos.y - info.viewport_top + info.scroll_y)
}

// ── public API ───────────────────────────────────────────────────────

/// Response from [`show_floating_card`].
pub struct FloatingCardResponse {
    /// Whether the drag handle was clicked (close/dock).
    pub handle_clicked: bool,
}

/// Show a floating card with full lifecycle management.
///
/// Handles Area creation, position persistence, anchor switching
/// (content-scrolling ↔ viewport-fixed), drag handle, and GORBIE chrome.
///
/// - `id`: unique egui ID for this float (position/anchor persisted under it)
/// - `initial_pos`: screen position for first appearance
/// - `card_width`: width of the card in pixels
/// - `min_height`: minimum card height (for placeholder matching), 0.0 for none
/// - `store`: state store for CardCtx
/// - `tooltip`: text shown when hovering the drag handle
/// - `draw_body`: closure that draws the card contents
pub fn show_floating_card(
    egui_ctx: &egui::Context,
    id: egui::Id,
    initial_pos: egui::Pos2,
    card_width: f32,
    min_height: f32,
    store: &state::StateStore,
    tooltip: &str,
    draw_body: &mut dyn FnMut(&mut CardCtx<'_>),
) -> FloatingCardResponse {
    let scroll_info = read_scroll_info(egui_ctx);

    // Read or initialize persisted float state.
    let float_state_id = id.with("gorbie_float_state");
    let mut fstate: FloatState = egui_ctx.memory_mut(|mem| {
        *mem.data
            .get_temp_mut_or_insert_with(float_state_id, move || {
                // Convert initial screen pos to content coords if we have scroll info.
                let (pos, anchor) = if let Some(info) = &scroll_info {
                    (screen_to_content(initial_pos, info), Anchor::Content)
                } else {
                    (initial_pos, Anchor::Viewport)
                };
                FloatState { pos, anchor }
            })
    });

    // Convert stored position to screen coordinates for rendering.
    let screen_pos = match (fstate.anchor, &scroll_info) {
        (Anchor::Content, Some(info)) => content_to_screen(fstate.pos, info),
        _ => fstate.pos,
    };

    let area_order = match fstate.anchor {
        Anchor::Content => egui::Order::Foreground,
        Anchor::Viewport => egui::Order::Tooltip,
    };

    let area = egui::Area::new(id)
        .order(area_order)
        .fixed_pos(screen_pos)
        .movable(false)
        .constrain_to(egui::Rect::EVERYTHING);

    let mut handle_clicked = false;

    area.show(egui_ctx, |ui| {
        let resp = draw_card_chrome(
            ui,
            card_width,
            min_height,
            store,
            tooltip,
            draw_body,
        );

        if resp.dragged {
            ui.ctx().move_to_top(resp.layer_id);

            let delta = resp.drag_delta;
            fstate.pos += delta;

            // Anchor switching based on how far the card is dragged off the right edge.
            if let Some(info) = &scroll_info {
                let moved_screen_rect = resp.card_rect.translate(delta);
                match fstate.anchor {
                    Anchor::Content => {
                        if right_outside_ratio(moved_screen_rect, info.clip_rect)
                            >= STICK_RIGHT_OUTSIDE_RATIO
                        {
                            // Switch to viewport-fixed: store screen pos.
                            fstate.anchor = Anchor::Viewport;
                            fstate.pos = moved_screen_rect.min;
                        }
                    }
                    Anchor::Viewport => {
                        if right_outside_ratio(moved_screen_rect, info.clip_rect)
                            <= UNSTICK_RIGHT_OUTSIDE_RATIO
                        {
                            // Switch to content-scrolling: store content pos.
                            fstate.anchor = Anchor::Content;
                            fstate.pos = screen_to_content(moved_screen_rect.min, info);
                        }
                    }
                }
            }

            // Persist updated state.
            ui.ctx().memory_mut(|mem| {
                mem.data.insert_temp(float_state_id, fstate);
            });
        }

        handle_clicked = resp.handle_clicked;

        // Track the content-space extent of this float for scroll area sizing.
        if fstate.anchor == Anchor::Content {
            let content_bottom = fstate.pos.y + resp.card_rect.height();
            record_float_extent(ui.ctx(), content_bottom);
        }
    });

    if handle_clicked {
        // Clean up persisted state so next show starts fresh.
        egui_ctx.memory_mut(|mem| {
            mem.data.remove_temp::<FloatState>(float_state_id);
        });
    }

    FloatingCardResponse { handle_clicked }
}

// ── card chrome (internal) ───────────────────────────────────────────

struct CardChromeResponse {
    card_rect: egui::Rect,
    handle_clicked: bool,
    drag_delta: egui::Vec2,
    dragged: bool,
    layer_id: egui::LayerId,
}

fn draw_card_chrome(
    ui: &mut egui::Ui,
    card_width: f32,
    min_height: f32,
    store: &state::StateStore,
    tooltip: &str,
    draw_body: &mut dyn FnMut(&mut CardCtx<'_>),
) -> CardChromeResponse {
    let outline = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let shadow_color = themes::ral(9004);
    let shadow = egui::epaint::Shadow {
        offset: [6, 6],
        blur: 0,
        spread: 0,
        color: shadow_color,
    };

    let frame = egui::Frame::new()
        .fill(ui.visuals().window_fill)
        .stroke(egui::Stroke::new(1.0, outline))
        .shadow(shadow)
        .corner_radius(0.0)
        .inner_margin(egui::Margin::ZERO);

    let background_idx = ui.painter().add(egui::Shape::Noop);

    let min_y = ui.min_rect().min.y;
    let max_y = ui.max_rect().max.y.max(min_y + min_height);
    let max_rect = egui::Rect::from_min_max(
        ui.min_rect().min,
        egui::pos2(ui.min_rect().min.x + card_width, max_y),
    );

    let inner = ui.scope_builder(egui::UiBuilder::new().max_rect(max_rect), |ui| {
        ui.reset_style();
        if min_height > 0.0 {
            ui.set_min_size(egui::vec2(card_width, min_height));
        }
        ui.set_width(card_width);

        let restore_clip = ui.clip_rect();
        let card_clip = egui::Rect::from_min_max(
            egui::pos2(ui.min_rect().left(), restore_clip.min.y),
            egui::pos2(ui.min_rect().left() + card_width, restore_clip.max.y),
        );
        ui.set_clip_rect(card_clip);
        let mut ctx = CardCtx::new(ui, store);
        draw_body(&mut ctx);
        ui.set_clip_rect(restore_clip);
    });

    let content_min = inner.response.rect.min;
    let card_rect = egui::Rect::from_min_size(
        content_min,
        egui::vec2(card_width, inner.response.rect.height()),
    );
    let content_rect = card_rect.shrink(frame.stroke.width);
    ui.painter()
        .set(background_idx, frame.paint(content_rect));

    // ── drag handle ──────────────────────────────────────────────────
    let handle_height = GRID_ROW_MODULE;
    let handle_rect = egui::Rect::from_min_size(
        content_rect.min,
        egui::vec2(content_rect.width(), handle_height),
    );
    let handle_id = ui.id().with("floating_handle");
    let handle_resp = ui.interact(handle_rect, handle_id, egui::Sense::click_and_drag());

    if handle_resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if handle_resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    let show_stripes = handle_resp.hovered() || handle_resp.dragged();
    if show_stripes {
        let stripe_color = themes::ral(9004);
        let stripe_stroke = egui::Stroke::new(1.0, stripe_color);
        let stripe_x = handle_rect.x_range();
        let stripe_spacing = 3.0;
        let mut stripe_y = handle_rect.top() + stripe_spacing - stripe_stroke.width * 0.5;
        let painter = ui.painter();
        while stripe_y <= handle_rect.bottom() {
            painter.hline(stripe_x, stripe_y, stripe_stroke);
            stripe_y += stripe_spacing;
        }
    }

    if handle_resp.hovered() {
        crate::show_postit_tooltip(ui, &handle_resp, tooltip);
    }

    CardChromeResponse {
        card_rect,
        handle_clicked: handle_resp.clicked(),
        drag_delta: handle_resp.drag_delta(),
        dragged: handle_resp.dragged(),
        layer_id: handle_resp.layer_id,
    }
}
