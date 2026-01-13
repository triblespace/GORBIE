//! ## Working with mutable/non-cloneable things.
//! Sometimes when working with existing code, libraries or even std things like
//! files, can introduce an impedance mismatch with a dataflow-style model.
//! Often it is enough to wrap the object in question into another layer of `Arc`s
//! and `RWLock`s in addition to what Gorby already does with its shared state
//! store.
//!
//! For heavier work, `ComputedState` can run background tasks and hold the latest
//! value. Use `Option<T>` when a value may be absent while a computation runs.
//!
//! But sometimes that isn't enough, e.g. when you want to display some application
//! global state. This is why `state!` and `view!` are carefully designed to stay
//! independent from any dataflow runtime. Instead they can be used, like any other
//! mutable rust type, via the typed `StateId` handle.
//!

#![allow(non_snake_case)]

pub mod cards;
pub mod dataflow;
pub mod prelude;
pub mod state;
pub mod themes;
pub mod widgets;

pub use gorbie_macros::{notebook, state, view};

use crate::state::StateStore;
use crate::themes::industrial_dark;
use crate::themes::industrial_fonts;
use crate::themes::industrial_light;
use eframe::egui::{self};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FloatingAnchor {
    Content,
    Viewport,
}

enum FloatingElement {
    DetachedCard(DetachedCardDraw),
    CodeNote(CodeNoteDraw),
}

struct DetachedCardDraw {
    index: usize,
    area_id: egui::Id,
    width: f32,
}

struct CodeNoteDraw {
    index: usize,
    area_id: egui::Id,
    base_note_pos: egui::Pos2,
    code: String,
}

/// A notebook is a collection of cards.
/// Each card is a piece of content that can be displayed in the notebook.
/// Cards can be stateless or stateful.
pub struct Notebook {
    title: String,
    header_title: egui::WidgetText,
    pub cards: Vec<Box<dyn cards::Card + 'static>>,
    pub(crate) state_store: Arc<StateStore>,
    code_notes_open: Vec<bool>,
    code_note_offsets: Vec<egui::Vec2>,
    code_note_anchors: Vec<FloatingAnchor>,
    code_note_viewport_positions: Vec<egui::Pos2>,
    card_detached: Vec<bool>,
    card_detached_positions: Vec<egui::Pos2>,
    card_detached_anchors: Vec<FloatingAnchor>,
    card_placeholder_sizes: Vec<egui::Vec2>,
}

const NOTEBOOK_COLUMN_WIDTH: f32 = 768.0;
const NOTEBOOK_MIN_HEIGHT: f32 = 360.0;

impl Default for Notebook {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl Notebook {
    pub fn new(name: impl Into<String>) -> Self {
        let title = name.into();
        let header_title = if title.is_empty() {
            egui::WidgetText::default()
        } else {
            egui::RichText::new(title.to_uppercase())
                .monospace()
                .strong()
                .into()
        };
        Self {
            title,
            header_title,
            cards: Vec::new(),
            state_store: Arc::new(StateStore::new()),
            code_notes_open: Vec::new(),
            code_note_offsets: Vec::new(),
            code_note_anchors: Vec::new(),
            code_note_viewport_positions: Vec::new(),
            card_detached: Vec::new(),
            card_detached_positions: Vec::new(),
            card_detached_anchors: Vec::new(),
            card_placeholder_sizes: Vec::new(),
        }
    }

    pub fn push(&mut self, card: Box<dyn cards::Card>) {
        self.cards.push(card);
        self.code_notes_open.push(false);
        self.code_note_offsets.push(egui::Vec2::ZERO);
        self.code_note_anchors.push(FloatingAnchor::Content);
        self.code_note_viewport_positions.push(egui::Pos2::ZERO);
        self.card_detached.push(false);
        self.card_detached_positions.push(egui::Pos2::ZERO);
        self.card_detached_anchors.push(FloatingAnchor::Content);
        self.card_placeholder_sizes.push(egui::Vec2::ZERO);
    }

    pub fn run(self) -> eframe::Result {
        let notebook = self;
        let window_title = if notebook.title.is_empty() {
            "GORBIE".to_owned()
        } else {
            notebook.title.clone()
        };

        let mut native_options = eframe::NativeOptions::default();
        native_options.persist_window = true;
        native_options.viewport = native_options
            .viewport
            .with_inner_size(egui::vec2(1200.0, 800.0))
            .with_min_inner_size(egui::vec2(NOTEBOOK_COLUMN_WIDTH, NOTEBOOK_MIN_HEIGHT));

        eframe::run_native(
            &window_title,
            native_options,
            Box::new(|cc| {
                let ctx = cc.egui_ctx.clone();
                ctrlc::set_handler(move || ctx.send_viewport_cmd(egui::ViewportCommand::Close))
                    .expect("failed to set exit signal handler");

                cc.egui_ctx.set_fonts(industrial_fonts());

                cc.egui_ctx
                    .set_style_of(egui::Theme::Light, industrial_light());
                cc.egui_ctx
                    .set_style_of(egui::Theme::Dark, industrial_dark());

                Ok(Box::new(notebook))
            }),
        )
    }
}

impl eframe::App for Notebook {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show_viewport(ui, |ui, viewport| {
                    let rect = ui.max_rect();
                    let clip_rect = ui.clip_rect();
                    let scroll_y = viewport.min.y;

                    let column_width = NOTEBOOK_COLUMN_WIDTH;
                    let left_margin_width = 0.0;
                    let card_width = column_width;

                    let left_margin_paint = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, clip_rect.min.y),
                        egui::pos2(rect.min.x + left_margin_width, clip_rect.max.y),
                    );
                    let left_margin = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(rect.min.x + left_margin_width, rect.max.y),
                    );
                    let column_rect = egui::Rect::from_min_max(
                        egui::pos2(left_margin.max.x, rect.min.y),
                        egui::pos2(left_margin.max.x + column_width, rect.max.y),
                    );
                    let right_margin_paint = egui::Rect::from_min_max(
                        egui::pos2(column_rect.max.x, clip_rect.min.y),
                        egui::pos2(rect.max.x, clip_rect.max.y),
                    );
                    let right_margin = egui::Rect::from_min_max(
                        egui::pos2(column_rect.max.x, rect.min.y),
                        rect.max,
                    );

                    paint_dot_grid(ui, left_margin_paint, scroll_y);
                    paint_dot_grid(ui, right_margin_paint, scroll_y);

                    ui.scope_builder(egui::UiBuilder::new().max_rect(column_rect), |ui| {
                        ui.set_min_size(column_rect.size());
                        ui.set_max_width(column_rect.width());

                        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                        let fill = ui.visuals().window_fill;

                        let column_inner_margin = egui::Margin::symmetric(0, 12);
                        let code_note_width = card_width;

                        egui::Frame::new()
                            .fill(fill)
                            .stroke(stroke)
                            .corner_radius(0.0)
                            .inner_margin(column_inner_margin)
                            .show(ui, |ui| {
                                // Theme switch is part of the page header (above the first card).
                                ui.horizontal(|ui| {
                                    ui.add_space(16.0);
                                    if !self.header_title.is_empty() {
                                        ui.add(
                                            egui::Label::new(self.header_title.clone()).truncate(),
                                        );
                                    }

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.add_space(16.0);
                                            let mut preference =
                                                ui.ctx().options(|opt| opt.theme_preference);
                                            if ui
                                                .add(
                                                    widgets::ChoiceToggle::new(&mut preference)
                                                        .choice(egui::ThemePreference::System, "◐")
                                                        .choice(egui::ThemePreference::Dark, "●")
                                                        .choice(egui::ThemePreference::Light, "○"),
                                                )
                                                .changed()
                                            {
                                                ui.ctx().set_theme(preference);
                                            }
                                        },
                                    );
                                });

                                ui.add_space(12.0);

                                let divider_x_range = ui.max_rect().x_range();

                                self.code_notes_open.resize(self.cards.len(), false);
                                self.code_note_offsets
                                    .resize(self.cards.len(), egui::Vec2::ZERO);
                                self.code_note_anchors
                                    .resize(self.cards.len(), FloatingAnchor::Content);
                                self.code_note_viewport_positions
                                    .resize(self.cards.len(), egui::Pos2::ZERO);
                                self.card_detached.resize(self.cards.len(), false);
                                self.card_detached_positions
                                    .resize(self.cards.len(), egui::Pos2::ZERO);
                                self.card_detached_anchors
                                    .resize(self.cards.len(), FloatingAnchor::Content);
                                self.card_placeholder_sizes
                                    .resize(self.cards.len(), egui::Vec2::ZERO);

                                ui.style_mut().spacing.item_spacing.y = 0.0;
                                let mut max_code_note_bottom_content_y: Option<f32> = None;
                                let mut floating_elements: Vec<FloatingElement> = Vec::new();
                                let mut dragged_layer_ids: Vec<egui::LayerId> = Vec::new();
                                for (i, card) in self.cards.iter_mut().enumerate() {
                                    let code_note_open = self
                                        .code_notes_open
                                        .get_mut(i)
                                        .expect("code_notes_open synced to cards");
                                    let card_detached = self
                                        .card_detached
                                        .get_mut(i)
                                        .expect("card_detached synced to cards");
                                    let card_detached_position = self
                                        .card_detached_positions
                                        .get_mut(i)
                                        .expect("card_detached_positions synced to cards");
                                    let card_detached_anchor = self
                                        .card_detached_anchors
                                        .get_mut(i)
                                        .expect("card_detached_anchors synced to cards");
                                    let card_placeholder_size = self
                                        .card_placeholder_sizes
                                        .get_mut(i)
                                        .expect("card_placeholder_sizes synced to cards");
                                    ui.push_id(i, |ui| {
                                        let card: &mut dyn cards::Card = card.as_mut();
                                        let card_rect = if *card_detached {
                                            let placeholder_height =
                                                if card_placeholder_size.y > 0.0 {
                                                    card_placeholder_size.y
                                                } else {
                                                    120.0
                                                };
                                            let placeholder_width = card_width;
                                            let (rect, resp) = ui.allocate_exact_size(
                                                egui::vec2(placeholder_width, placeholder_height),
                                                egui::Sense::click(),
                                            );
                                            let fill = ui.visuals().window_fill;
                                            let outline =
                                                ui.visuals().widgets.noninteractive.bg_stroke.color;
                                            ui.painter().rect_filled(rect, 0.0, fill);
                                            paint_hatching(
                                                &ui.painter().with_clip_rect(rect),
                                                rect,
                                                outline,
                                            );
                                            show_postit_tooltip(ui, &resp, "Dock card");
                                            if resp.clicked() {
                                                *card_detached = false;
                                            }

                                            if *card_detached {
                                                let detached_card_width =
                                                    card_width.max(240.0);
                                                if *card_detached_position == egui::Pos2::ZERO {
                                                    let initial_screen_pos = egui::pos2(
                                                        right_margin.min.x + 12.0,
                                                        rect.top(),
                                                    );
                                                    *card_detached_anchor =
                                                        FloatingAnchor::Content;
                                                    *card_detached_position = screen_to_content_pos(
                                                        initial_screen_pos,
                                                        scroll_y,
                                                        clip_rect.min.y,
                                                    );
                                                }

                                                let detached_id = ui.id().with("detached_card");
                                                floating_elements.push(FloatingElement::DetachedCard(
                                                    DetachedCardDraw {
                                                        index: i,
                                                        area_id: detached_id,
                                                        width: detached_card_width,
                                                    },
                                                ));
                                            }
                                            rect
                                        } else {
                                            let inner = egui::Frame::group(ui.style())
                                                .stroke(egui::Stroke::NONE)
                                                .corner_radius(0.0)
                                                .inner_margin(egui::Margin::ZERO)
                                                .show(ui, |ui| {
                                                    ui.reset_style();
                                                    ui.set_width(card_width);
                                                    let mut ctx = cards::CardContext::new(
                                                        ui,
                                                        self.state_store.as_ref(),
                                                    );
                                                    card.draw(&mut ctx);
                                                });
                                            *card_placeholder_size = egui::vec2(
                                                card_width,
                                                inner.response.rect.height(),
                                            );
                                            inner.response.rect
                                        };
                                        ui.painter().hline(
                                            divider_x_range,
                                            card_rect.top(),
                                            ui.visuals().widgets.noninteractive.bg_stroke,
                                        );

                                        let button_size = egui::vec2(18.0, 18.0);
                                        let button_spacing = 6.0;
                                        let button_x = (card_rect.right() + 8.0).round();
                                        let has_code_note = card.code().is_some();
                                        let show_detach_button = !*card_detached;
                                        let (detach_pos, code_button_pos) = if has_code_note {
                                            let stack_height =
                                                button_size.y * 2.0 + button_spacing;
                                            let stack_top = (card_rect.center().y
                                                - stack_height / 2.0)
                                                .round();
                                            (
                                                Some(egui::pos2(button_x, stack_top)),
                                                Some(egui::pos2(
                                                    button_x,
                                                    stack_top + button_size.y + button_spacing,
                                                )),
                                            )
                                        } else {
                                            (
                                                Some(egui::pos2(
                                                    button_x,
                                                    (card_rect.center().y - button_size.y / 2.0)
                                                        .round(),
                                                )),
                                                None,
                                            )
                                        };

                                        if let Some(detach_pos) = detach_pos {
                                            let detach_id = ui.id().with("detach_button");
                                            let detach_area = egui::Area::new(detach_id)
                                                .order(egui::Order::Middle)
                                                .fixed_pos(detach_pos)
                                                .movable(false)
                                                .constrain_to(egui::Rect::EVERYTHING);
                                            if show_detach_button {
                                                let detach_resp = detach_area.show(ui.ctx(), |ui| {
                                                    let (rect, resp) = ui.allocate_exact_size(
                                                        button_size,
                                                        egui::Sense::click(),
                                                    );
                                                    let fill = ui.visuals().window_fill;
                                                    let outline = ui
                                                        .visuals()
                                                        .widgets
                                                        .noninteractive
                                                        .bg_stroke
                                                        .color;
                                                    let accent =
                                                        ui.visuals().selection.stroke.color;
                                                    let stroke_color =
                                                        if resp.hovered() || resp.has_focus() {
                                                            accent
                                                        } else {
                                                            outline
                                                        };
                                                    let stroke =
                                                        egui::Stroke::new(1.0, stroke_color);

                                                    ui.painter().rect_filled(rect, 0.0, fill);
                                                    ui.painter().rect_stroke(
                                                        rect,
                                                        0.0,
                                                        stroke,
                                                        egui::StrokeKind::Inside,
                                                    );
                                                    ui.painter().text(
                                                        rect.center(),
                                                        egui::Align2::CENTER_CENTER,
                                                        "[]",
                                                        egui::FontId::monospace(10.0),
                                                        ui.visuals().text_color(),
                                                    );

                                                    let tooltip = if *card_detached {
                                                        "Dock card"
                                                    } else {
                                                        "Detach card"
                                                    };
                                                    show_postit_tooltip(ui, &resp, tooltip);
                                                    resp
                                                });

                                                if detach_resp.inner.clicked() {
                                                    if *card_detached {
                                                        *card_detached = false;
                                                    } else {
                                                        *card_detached = true;
                                                        *card_detached_anchor =
                                                            FloatingAnchor::Content;
                                                        let initial_screen_pos = egui::pos2(
                                                            right_margin.min.x + 12.0,
                                                            card_rect.top(),
                                                        );
                                                        *card_detached_position =
                                                            screen_to_content_pos(
                                                                initial_screen_pos,
                                                                scroll_y,
                                                                clip_rect.min.y,
                                                            );
                                                    }
                                                }
                                            } else {
                                                detach_area.show(ui.ctx(), |ui| {
                                                    ui.allocate_exact_size(
                                                        button_size,
                                                        egui::Sense::hover(),
                                                    );
                                                });
                                            }
                                        }

                                        let Some(code) = card.code() else {
                                            return;
                                        };

                                        let base_note_pos =
                                            egui::pos2(button_x, card_rect.top());
                                        let flag_id = ui.id().with("code_flag");
                                        if *code_note_open {
                                            floating_elements.push(
                                                FloatingElement::CodeNote(CodeNoteDraw {
                                                    index: i,
                                                    area_id: flag_id,
                                                    base_note_pos,
                                                    code: code.to_owned(),
                                                }),
                                            );
                                        } else {
                                            let flag_pos =
                                                code_button_pos.expect("code button position");
                                            let flag_resp = egui::Area::new(flag_id)
                                                .order(egui::Order::Middle)
                                                .fixed_pos(flag_pos)
                                                .movable(false)
                                                .constrain_to(egui::Rect::EVERYTHING)
                                                .show(ui.ctx(), |ui| {
                                                    let (rect, resp) = ui.allocate_exact_size(
                                                        button_size,
                                                        egui::Sense::click(),
                                                    );

                                                    let fill = ui.visuals().window_fill;
                                                    let outline = ui
                                                        .visuals()
                                                        .widgets
                                                        .noninteractive
                                                        .bg_stroke
                                                        .color;
                                                    let accent =
                                                        ui.visuals().selection.stroke.color;
                                                    let stroke_color =
                                                        if resp.hovered() || resp.has_focus() {
                                                            accent
                                                        } else {
                                                            outline
                                                        };
                                                    let stroke =
                                                        egui::Stroke::new(1.0, stroke_color);

                                                    ui.painter().rect_filled(rect, 0.0, fill);
                                                    ui.painter().rect_stroke(
                                                        rect,
                                                        0.0,
                                                        stroke,
                                                        egui::StrokeKind::Inside,
                                                    );
                                                    ui.painter().text(
                                                        rect.center(),
                                                        egui::Align2::CENTER_CENTER,
                                                        "{}",
                                                        egui::FontId::monospace(10.0),
                                                        ui.visuals().text_color(),
                                                    );

                                                    show_postit_tooltip(
                                                        ui,
                                                        &resp,
                                                        "Show code note",
                                                    );
                                                    resp
                                                });

                                            if flag_resp.inner.clicked() {
                                                *code_note_open = true;
                                            }
                                        }
                                    });
                                }

                                for pass_anchor in
                                    [FloatingAnchor::Content, FloatingAnchor::Viewport]
                                {
                                    for element in floating_elements.iter() {
                                        match element {
                                            FloatingElement::DetachedCard(draw) => {
                                                let current_anchor = *self
                                                    .card_detached_anchors
                                                    .get(draw.index)
                                                    .expect(
                                                        "card_detached_anchors synced to cards",
                                                    );
                                                if current_anchor != pass_anchor {
                                                    continue;
                                                }

                                                let card_detached = self
                                                    .card_detached
                                                    .get_mut(draw.index)
                                                    .expect("card_detached synced to cards");
                                                if !*card_detached {
                                                    continue;
                                                }

                                                let card_detached_position = self
                                                    .card_detached_positions
                                                    .get_mut(draw.index)
                                                    .expect(
                                                        "card_detached_positions synced to cards",
                                                    );
                                                let card_detached_anchor = self
                                                    .card_detached_anchors
                                                    .get_mut(draw.index)
                                                    .expect(
                                                        "card_detached_anchors synced to cards",
                                                    );

                                                let detached_screen_pos =
                                                    match *card_detached_anchor {
                                                        FloatingAnchor::Content => {
                                                            content_to_screen_pos(
                                                                *card_detached_position,
                                                                scroll_y,
                                                                clip_rect.min.y,
                                                            )
                                                        }
                                                        FloatingAnchor::Viewport => {
                                                            *card_detached_position
                                                        }
                                                    };

                                                let card_width = draw.width;
                                                let detached_id = draw.area_id;
                                                let card: &mut dyn cards::Card = self
                                                    .cards
                                                    .get_mut(draw.index)
                                                    .expect("cards synced to floating_elements")
                                                    .as_mut();

                                                let area_order = match pass_anchor {
                                                    FloatingAnchor::Content => {
                                                        egui::Order::Foreground
                                                    }
                                                    FloatingAnchor::Viewport => egui::Order::Tooltip,
                                                };
                                                let detached_area =
                                                    egui::Area::new(detached_id)
                                                        .order(area_order)
                                                        .fixed_pos(detached_screen_pos)
                                                        .movable(false)
                                                        .constrain_to(egui::Rect::EVERYTHING);
                                                detached_area.show(ui.ctx(), |ui| {
                                                        let outline = ui
                                                            .visuals()
                                                            .widgets
                                                            .noninteractive
                                                            .bg_stroke
                                                            .color;
                                                        let shadow_color =
                                                            crate::themes::ral(9004);
                                                        let shadow = egui::epaint::Shadow {
                                                            offset: [6, 6],
                                                            blur: 0,
                                                            spread: 0,
                                                            color: shadow_color,
                                                        };

                                                        ui.set_width(card_width);
                                                        let frame = egui::Frame::new()
                                                            .fill(ui.visuals().window_fill)
                                                            .stroke(egui::Stroke::new(
                                                                1.0, outline,
                                                            ))
                                                            .shadow(shadow)
                                                            .corner_radius(0.0)
                                                            .inner_margin(egui::Margin::ZERO);

                                                        let mut handle_resp = None;
                                                        let inner = frame.show(ui, |ui| {
                                                            let handle_height = 18.0;
                                                            let previous_spacing =
                                                                ui.spacing().item_spacing;
                                                            ui.spacing_mut().item_spacing = egui::vec2(
                                                                previous_spacing.x,
                                                                0.0,
                                                            );
                                                            let (handle_rect, handle_resp_local) =
                                                                ui.allocate_exact_size(
                                                                    egui::vec2(
                                                                        ui.available_width(),
                                                                        handle_height,
                                                                    ),
                                                                    egui::Sense::click_and_drag(),
                                                                );
                                                            ui.spacing_mut().item_spacing =
                                                                previous_spacing;
                                                            if handle_resp_local.dragged() {
                                                                ui.ctx().set_cursor_icon(
                                                                    egui::CursorIcon::Grabbing,
                                                                );
                                                            } else if handle_resp_local.hovered() {
                                                                ui.ctx().set_cursor_icon(
                                                                    egui::CursorIcon::Grab,
                                                                );
                                                            }

                                                            let stripe_color =
                                                                crate::themes::ral(9004);
                                                            let stripe_stroke = egui::Stroke::new(
                                                                1.0,
                                                                stripe_color,
                                                            );
                                                            let stripe_x = handle_rect.x_range();
                                                            let stripe_padding = 3.0;
                                                            let stripe_spacing = 3.0;
                                                            let mut stripe_y = handle_rect.top()
                                                                + stripe_padding;
                                                            while stripe_y
                                                                <= handle_rect.bottom()
                                                                    - stripe_padding
                                                            {
                                                                ui.painter().hline(
                                                                    stripe_x,
                                                                    stripe_y,
                                                                    stripe_stroke,
                                                                );
                                                                stripe_y += stripe_spacing;
                                                            }

                                                            show_postit_tooltip(
                                                                ui,
                                                                &handle_resp_local,
                                                                "Dock card",
                                                            );

                                                            handle_resp =
                                                                Some(handle_resp_local.clone());
                                                            egui::Frame::group(ui.style())
                                                                .stroke(egui::Stroke::NONE)
                                                                .corner_radius(0.0)
                                                                .inner_margin(egui::Margin::ZERO)
                                                                .show(ui, |ui| {
                                                                    ui.reset_style();
                                                                    ui.set_width(card_width);
                                                                    let mut ctx =
                                                                        cards::CardContext::new(
                                                                            ui,
                                                                            self.state_store
                                                                                .as_ref(),
                                                                        );
                                                                    card.draw(&mut ctx);
                                                                });
                                                            handle_resp_local
                                                        });

                                                        if let Some(handle_resp) = handle_resp {
                                                            if handle_resp.dragged() {
                                                                ui.ctx().move_to_top(
                                                                    handle_resp.layer_id,
                                                                );
                                                                dragged_layer_ids
                                                                    .push(handle_resp.layer_id);
                                                                let delta =
                                                                    handle_resp.drag_delta();
                                                                let moved_rect = inner
                                                                    .response
                                                                    .rect
                                                                    .translate(delta);
                                                                *card_detached_position += delta;

                                                                match *card_detached_anchor {
                                                                    FloatingAnchor::Content => {
                                                                        if right_outside_ratio(
                                                                            moved_rect,
                                                                            clip_rect,
                                                                        )
                                                                            >= STICK_RIGHT_OUTSIDE_RATIO
                                                                        {
                                                                            *card_detached_anchor =
                                                                                FloatingAnchor::Viewport;
                                                                            *card_detached_position =
                                                                                moved_rect.min;
                                                                        }
                                                                    }
                                                                    FloatingAnchor::Viewport => {
                                                                        if right_outside_ratio(
                                                                            moved_rect,
                                                                            clip_rect,
                                                                        )
                                                                            <= UNSTICK_RIGHT_OUTSIDE_RATIO
                                                                        {
                                                                            *card_detached_anchor =
                                                                                FloatingAnchor::Content;
                                                                            *card_detached_position =
                                                                                screen_to_content_pos(
                                                                                    moved_rect.min,
                                                                                    scroll_y,
                                                                                    clip_rect.min.y,
                                                                                );
                                                                        }
                                                                    }
                                                                }
                                                            }

                                                            if handle_resp.clicked() {
                                                                *card_detached = false;
                                                            }
                                                        }
                                                    });
                                            }
                                            FloatingElement::CodeNote(draw) => {
                                                let code_note_open = self
                                                    .code_notes_open
                                                    .get_mut(draw.index)
                                                    .expect("code_notes_open synced to cards");
                                                if !*code_note_open {
                                                    continue;
                                                }

                                                let code_note_offset = self
                                                    .code_note_offsets
                                                    .get_mut(draw.index)
                                                    .expect("code_note_offsets synced to cards");
                                                let code_note_anchor = self
                                                    .code_note_anchors
                                                    .get_mut(draw.index)
                                                    .expect("code_note_anchors synced to cards");
                                                let code_note_viewport_position = self
                                                    .code_note_viewport_positions
                                                    .get_mut(draw.index)
                                                    .expect(
                                                        "code_note_viewport_positions synced to cards",
                                                    );

                                                if *code_note_anchor != pass_anchor {
                                                    continue;
                                                }

                                                let open_note_pos = match *code_note_anchor {
                                                    FloatingAnchor::Content => {
                                                        draw.base_note_pos + *code_note_offset
                                                    }
                                                    FloatingAnchor::Viewport => {
                                                        *code_note_viewport_position
                                                    }
                                                };

                                                let mut code_note_frame_rect = None;
                                                let mut code_note_handle_resp = None;
                                                let area_order = match pass_anchor {
                                                    FloatingAnchor::Content => {
                                                        egui::Order::Foreground
                                                    }
                                                    FloatingAnchor::Viewport => egui::Order::Tooltip,
                                                };
                                                let code_note_area =
                                                    egui::Area::new(draw.area_id)
                                                        .order(area_order)
                                                        .fixed_pos(open_note_pos)
                                                        .movable(false)
                                                        .constrain_to(egui::Rect::EVERYTHING);
                                                let flag_resp = code_note_area.show(ui.ctx(), |ui| {
                                                        let outline = ui
                                                            .visuals()
                                                            .widgets
                                                            .noninteractive
                                                            .bg_stroke
                                                            .color;
                                                        let shadow_color =
                                                            crate::themes::ral(9004);
                                                        let shadow = egui::epaint::Shadow {
                                                            offset: [4, 4],
                                                            blur: 0,
                                                            spread: 0,
                                                            color: shadow_color,
                                                        };

                                                        ui.set_width(code_note_width);

                                                        let frame = egui::Frame::new()
                                                            .fill(crate::themes::ral(1003))
                                                            .stroke(egui::Stroke::new(
                                                                1.0, outline,
                                                            ))
                                                            .shadow(shadow)
                                                            .corner_radius(0.0)
                                                            .inner_margin(egui::Margin::ZERO);

                                                        let inner = frame.show(ui, |ui| {
                                                            let handle_height = 18.0;
                                                            let (handle_rect, handle_resp_local) =
                                                                ui.allocate_exact_size(
                                                                    egui::vec2(
                                                                        ui.available_width(),
                                                                        handle_height,
                                                                    ),
                                                                    egui::Sense::click_and_drag(),
                                                                );
                                                            if handle_resp_local.dragged() {
                                                                ui.ctx().set_cursor_icon(
                                                                    egui::CursorIcon::Grabbing,
                                                                );
                                                            } else if handle_resp_local.hovered() {
                                                                ui.ctx().set_cursor_icon(
                                                                    egui::CursorIcon::Grab,
                                                                );
                                                            }

                                                            let stripe_color =
                                                                crate::themes::ral(9004);
                                                            let stripe_stroke = egui::Stroke::new(
                                                                1.0,
                                                                stripe_color,
                                                            );
                                                            let stripe_x = handle_rect.x_range();
                                                            let stripe_padding = 3.0;
                                                            let stripe_spacing = 3.0;
                                                            let mut stripe_y = handle_rect.top()
                                                                + stripe_padding;
                                                            while stripe_y
                                                                <= handle_rect.bottom()
                                                                    - stripe_padding
                                                            {
                                                                ui.painter().hline(
                                                                    stripe_x,
                                                                    stripe_y,
                                                                    stripe_stroke,
                                                                );
                                                                stripe_y += stripe_spacing;
                                                            }

                                                            show_postit_tooltip(
                                                                ui,
                                                                &handle_resp_local,
                                                                "Hide code note",
                                                            );

                                                            code_note_handle_resp = Some(
                                                                handle_resp_local.clone(),
                                                            );

                                                            ui.add_space(6.0);
                                                            egui::Frame::new()
                                                                .inner_margin(egui::Margin::same(
                                                                    10,
                                                                ))
                                                                .show(ui, |ui| {
                                                                    ui.add(
                                                                        egui::Label::new(
                                                                            egui::RichText::new(
                                                                                &draw.code,
                                                                            )
                                                                            .monospace()
                                                                            .color(
                                                                                crate::themes::ral(
                                                                                    9011,
                                                                                ),
                                                                            ),
                                                                        )
                                                                        .selectable(true)
                                                                        .wrap_mode(
                                                                            egui::TextWrapMode::Wrap,
                                                                        ),
                                                                    );
                                                                });

                                                            handle_resp_local
                                                        });

                                                        code_note_frame_rect =
                                                            Some(inner.response.rect);
                                                        inner.inner
                                                    });

                                                if let (Some(handle_resp), Some(frame_rect)) = (
                                                    code_note_handle_resp,
                                                    code_note_frame_rect,
                                                ) {
                                                    if handle_resp.dragged() {
                                                        ui.ctx().move_to_top(
                                                            handle_resp.layer_id,
                                                        );
                                                        dragged_layer_ids
                                                            .push(handle_resp.layer_id);
                                                        let delta = handle_resp.drag_delta();
                                                        let moved_rect =
                                                            frame_rect.translate(delta);
                                                        let clamped_rect = clamp_rect_visible(
                                                            moved_rect,
                                                            clip_rect,
                                                            NOTE_MIN_VISIBLE,
                                                        );
                                                        let applied_delta =
                                                            clamped_rect.min - frame_rect.min;

                                                        match *code_note_anchor {
                                                            FloatingAnchor::Content => {
                                                                *code_note_offset +=
                                                                    applied_delta;
                                                                if right_outside_ratio(
                                                                    clamped_rect,
                                                                    clip_rect,
                                                                ) >= STICK_RIGHT_OUTSIDE_RATIO
                                                                {
                                                                    *code_note_anchor =
                                                                        FloatingAnchor::Viewport;
                                                                    *code_note_viewport_position =
                                                                        clamped_rect.min;
                                                                }
                                                            }
                                                            FloatingAnchor::Viewport => {
                                                                *code_note_viewport_position +=
                                                                    applied_delta;
                                                                if right_outside_ratio(
                                                                    clamped_rect,
                                                                    clip_rect,
                                                                ) <= UNSTICK_RIGHT_OUTSIDE_RATIO
                                                                {
                                                                    *code_note_anchor =
                                                                        FloatingAnchor::Content;
                                                                    *code_note_offset =
                                                                        clamped_rect.min
                                                                            - draw.base_note_pos;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }

                                                if flag_resp.inner.clicked() {
                                                    *code_note_open = false;
                                                }

                                                if *code_note_open
                                                    && *code_note_anchor
                                                        == FloatingAnchor::Content
                                                {
                                                    let note_bottom_content_y =
                                                        flag_resp.response.rect.bottom()
                                                            - clip_rect.min.y
                                                            + scroll_y;
                                                    max_code_note_bottom_content_y = Some(
                                                        max_code_note_bottom_content_y
                                                            .unwrap_or(note_bottom_content_y)
                                                            .max(note_bottom_content_y),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }

                                for layer_id in dragged_layer_ids {
                                    ui.ctx().move_to_top(layer_id);
                                }

                                if let Some(max_note_bottom_content_y) =
                                    max_code_note_bottom_content_y
                                {
                                    let content_bottom_y =
                                        ui.min_rect().bottom() - clip_rect.min.y + scroll_y;
                                    let padding = 12.0;
                                    let extra_bottom = (max_note_bottom_content_y + padding
                                        - content_bottom_y)
                                        .max(0.0);
                                    if extra_bottom > 0.0 {
                                        ui.allocate_space(egui::vec2(0.0, extra_bottom));
                                    }
                                }
                            });
                    });
                });
        });
    }
}

fn paint_dot_grid(ui: &egui::Ui, rect: egui::Rect, scroll_y: f32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    let painter = ui.painter_at(rect);

    let spacing = 18.0;
    let radius = 1.2;
    let background = ui.visuals().window_fill;
    let outline = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let color = crate::themes::blend(background, outline, 0.35);

    let start_x = (rect.left() / spacing).floor() * spacing + spacing / 2.0;
    let start_y = rect.top() - scroll_y.rem_euclid(spacing) + spacing / 2.0;

    let mut y = start_y;
    while y < rect.bottom() {
        let mut x = start_x;
        while x < rect.right() {
            painter.circle_filled(egui::pos2(x, y), radius, color);
            x += spacing;
        }
        y += spacing;
    }
}

fn paint_hatching(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let spacing = 12.0;
    let stroke = egui::Stroke::new(1.0, color);

    let h = rect.height();
    let mut x = rect.left() - h;
    while x < rect.right() + h {
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x + h, rect.bottom())],
            stroke,
        );
        x += spacing;
    }
}

const NOTE_MIN_VISIBLE: f32 = 24.0;
const STICK_RIGHT_OUTSIDE_RATIO: f32 = 0.5;
const UNSTICK_RIGHT_OUTSIDE_RATIO: f32 = 0.4;

fn screen_to_content_pos(pos: egui::Pos2, scroll_y: f32, viewport_top: f32) -> egui::Pos2 {
    egui::pos2(pos.x, pos.y - viewport_top + scroll_y)
}

fn content_to_screen_pos(pos: egui::Pos2, scroll_y: f32, viewport_top: f32) -> egui::Pos2 {
    egui::pos2(pos.x, pos.y - scroll_y + viewport_top)
}

fn clamp_rect_visible(rect: egui::Rect, viewport: egui::Rect, min_visible: f32) -> egui::Rect {
    let size = rect.size();
    let min_allowed_x = viewport.min.x + min_visible - size.x;
    let max_allowed_x = viewport.max.x - min_visible;
    let min_allowed_y = viewport.min.y + min_visible - size.y;
    let max_allowed_y = viewport.max.y - min_visible;

    let min_x = min_allowed_x.min(max_allowed_x);
    let max_x = min_allowed_x.max(max_allowed_x);
    let min_y = min_allowed_y.min(max_allowed_y);
    let max_y = min_allowed_y.max(max_allowed_y);

    let clamped_min = egui::pos2(
        rect.min.x.clamp(min_x, max_x),
        rect.min.y.clamp(min_y, max_y),
    );

    egui::Rect::from_min_size(clamped_min, size)
}

fn right_outside_ratio(rect: egui::Rect, viewport: egui::Rect) -> f32 {
    let width = rect.width().max(0.0);
    if width <= 0.0 {
        return 1.0;
    }

    let outside_width = (rect.right() - viewport.right()).max(0.0);
    let ratio = outside_width / width;
    ratio.clamp(0.0, 1.0)
}

fn show_postit_tooltip(ui: &egui::Ui, response: &egui::Response, text: &str) {
    let outline = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let shadow_color = crate::themes::ral(9004);
    let shadow = egui::epaint::Shadow {
        offset: [4, 4],
        blur: 0,
        spread: 0,
        color: shadow_color,
    };

    let frame = egui::Frame::new()
        .fill(crate::themes::ral(1003))
        .stroke(egui::Stroke::new(1.0, outline))
        .shadow(shadow)
        .corner_radius(0.0)
        .inner_margin(egui::Margin::same(10));

    let mut tooltip = egui::containers::Tooltip::for_enabled(response);
    tooltip.popup = tooltip.popup.frame(frame);
    tooltip.show(|ui| {
        ui.set_max_width(ui.spacing().tooltip_width);
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .monospace()
                    .color(crate::themes::ral(9011)),
            )
            .wrap_mode(egui::TextWrapMode::Extend),
        );
    });
}

// notebook initialization is handled by the #[notebook] attribute macro.
