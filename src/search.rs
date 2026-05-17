//! Generic notebook-wide search.
//!
//! Pattern: a widget that wants search support calls
//! [`crate::CardCtx::search`] during render. As a side effect, GORBIE
//! marks "search was requested this frame" and the notebook displays a
//! small text field + `n / total` counter + `◀ ▶` navigation in the
//! top-right corner. The widget reads the current query and uses it
//! however it wants (filter, highlight, jump-to-match, etc) — GORBIE
//! doesn't dictate the local behaviour.
//!
//! Widgets that find a match call [`SearchSession::report`] with a
//! stable `egui::Id` per match. That feeds the global counter and lets
//! the bar's prev/next buttons cycle a "focused" match. The widget
//! gets back a [`MatchInfo`] telling it whether THIS match is the
//! currently-focused one (so it can paint differently) and whether the
//! user just navigated to it (so it can scroll itself into view).

use egui::{Align2, Area, Frame, Id, Margin};

use crate::widgets::{Button, TextField};

/// Paint a per-glyph search-match indicator under `char_rect` in RAL
/// 1003 signal yellow. Non-focused matches render as a single 2 px
/// stroke; the currently-focused match (the one the bar's prev/next
/// nav has selected) renders as a double parallel stroke for a more
/// "GORBIE-themed" emphasis.
pub fn paint_match_underline(
    painter: &egui::Painter,
    char_rect: egui::Rect,
    focused: bool,
) {
    let yellow = crate::themes::ral(1003);
    let b = char_rect.bottom();
    if focused {
        let line1 = egui::Rect::from_min_max(
            egui::pos2(char_rect.left(), b - 1.0),
            egui::pos2(char_rect.right(), b + 0.5),
        );
        let line2 = egui::Rect::from_min_max(
            egui::pos2(char_rect.left(), b + 2.0),
            egui::pos2(char_rect.right(), b + 3.5),
        );
        painter.rect_filled(line1, egui::CornerRadius::ZERO, yellow);
        painter.rect_filled(line2, egui::CornerRadius::ZERO, yellow);
    } else {
        let line = egui::Rect::from_min_max(
            egui::pos2(char_rect.left(), b - 1.0),
            egui::pos2(char_rect.right(), b + 1.0),
        );
        painter.rect_filled(line, egui::CornerRadius::ZERO, yellow);
    }
}

/// Build a multi-style text layout where every (case-insensitive)
/// occurrence of `needle` in `text` is underlined in RAL 1003 signal
/// yellow. All other runs use `base` unchanged. Empty needle returns
/// a plain run.
///
/// Assumes Latin-ish text — uses byte-indexed `to_lowercase().find`
/// which is correct for ASCII and works for most Latin extended.
pub fn highlight_match(
    text: &str,
    needle: &str,
    base: egui::TextFormat,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    if needle.is_empty() {
        job.append(text, 0.0, base);
        return job;
    }
    let lower = text.to_lowercase();
    // Caller is expected to lowercase the needle; cheap-double-check
    // by re-lowercasing.
    let needle_lower = needle.to_lowercase();
    let mut highlighted = base.clone();
    highlighted.underline = egui::Stroke::new(2.0, crate::themes::ral(1003));

    let mut cursor = 0usize;
    while let Some(pos) = lower[cursor..].find(&needle_lower) {
        let abs = cursor + pos;
        if abs > cursor {
            job.append(&text[cursor..abs], 0.0, base.clone());
        }
        let end = abs + needle_lower.len();
        job.append(&text[abs..end], 0.0, highlighted.clone());
        cursor = end;
    }
    if cursor < text.len() {
        job.append(&text[cursor..], 0.0, base);
    }
    job
}

/// Render a wrapping label whose needle occurrences carry GORBIE's
/// search-match highlight. When `focused` is true, a second yellow
/// stroke is painted underneath each match (the same double-stroke
/// emphasis the typst widget uses for the bar's active match), so
/// label-based widgets (messages bubbles, decide cards, mail headers,
/// relations entries) stay consistent with typst-rendered content.
///
/// Returns the label's `Response` so callers can attach scroll-to or
/// hover behavior.
pub fn highlight_label(
    ui: &mut egui::Ui,
    text: &str,
    needle: &str,
    base: egui::TextFormat,
    focused: bool,
) -> egui::Response {
    if needle.is_empty() {
        // No search active — just a plain wrapping label.
        let mut job = egui::text::LayoutJob::default();
        job.append(text, 0.0, base);
        return ui.add(
            egui::Label::new(job).wrap_mode(egui::TextWrapMode::Wrap),
        );
    }

    // Build the job once; reuse it for both the laid-out galley (which
    // we need for hit-positions) and the label render.
    let mut job = highlight_match(text, needle, base.clone());
    // Wrap to the available width — same behavior as a normal wrapping
    // Label.
    job.wrap.max_width = ui.available_width();
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    let (rect, response) =
        ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    let painter = ui.painter().clone();
    painter.galley(rect.min, galley.clone(), base.color);

    if focused {
        paint_focused_overlay(&painter, rect.min, &galley, text, needle);
    }

    response
}

/// For each (case-insensitive) needle occurrence in `text`, paint a
/// second yellow stroke just below the existing underline. Matches
/// that span multiple rows in the galley get one stroke per row.
fn paint_focused_overlay(
    painter: &egui::Painter,
    origin: egui::Pos2,
    galley: &egui::Galley,
    text: &str,
    needle: &str,
) {
    let needle_lower = needle.to_lowercase();
    if needle_lower.is_empty() {
        return;
    }
    let lower = text.to_lowercase();
    let yellow = crate::themes::ral(1003);

    let mut byte_cursor = 0usize;
    while let Some(pos) = lower[byte_cursor..].find(&needle_lower) {
        let start_byte = byte_cursor + pos;
        let end_byte = start_byte + needle_lower.len();
        // Convert byte offsets to char offsets — egui's cursor API is
        // char-indexed. For pure-ASCII text these are the same; for
        // mixed text the char walk stays correct.
        let start_char = text[..start_byte].chars().count();
        let end_char = text[..end_byte].chars().count();

        let start_rect = galley.pos_from_cursor(
            egui::epaint::text::cursor::CCursor::new(start_char),
        );
        let end_rect = galley.pos_from_cursor(
            egui::epaint::text::cursor::CCursor::new(end_char),
        );

        if (start_rect.top() - end_rect.top()).abs() < 0.5 {
            // Single-row match — paint one extra underline.
            let baseline = origin.y + start_rect.bottom();
            let x0 = origin.x + start_rect.left();
            let x1 = origin.x + end_rect.left();
            paint_double_underline_stripe(painter, x0, x1, baseline, yellow);
        } else {
            // Multi-row match — find every row whose y intersects the
            // span [start_rect.top, end_rect.bottom] and paint a stripe
            // covering the visible x range on that row.
            for row in &galley.rows {
                let row_top = row.pos.y;
                if row_top < start_rect.top() - 0.5 || row_top > end_rect.top() + 0.5 {
                    continue;
                }
                let row_left = if row_top <= start_rect.top() + 0.5 {
                    origin.x + start_rect.left()
                } else {
                    origin.x + row.pos.x
                };
                let row_right = if row_top >= end_rect.top() - 0.5 {
                    origin.x + end_rect.left()
                } else {
                    origin.x + row.pos.x + row.size.x
                };
                let baseline = origin.y + row.pos.y + row.size.y;
                paint_double_underline_stripe(painter, row_left, row_right, baseline, yellow);
            }
        }
        byte_cursor = end_byte;
    }
}

fn paint_double_underline_stripe(
    painter: &egui::Painter,
    x0: f32,
    x1: f32,
    baseline_y: f32,
    color: egui::Color32,
) {
    // The label's own LayoutJob already painted a single 2-px stroke
    // at the baseline. We add a second 2-px stroke 1.5 px below to
    // produce the GORBIE double-underline emphasis without rendering
    // the entire label ourselves.
    let stripe = egui::Rect::from_min_max(
        egui::pos2(x0, baseline_y + 1.5),
        egui::pos2(x1, baseline_y + 3.5),
    );
    painter.rect_filled(stripe, egui::CornerRadius::ZERO, color);
}

/// Wrapper so we can `insert_temp` an `Option<Id>` (egui's
/// `remove_temp`/`get_temp_mut_or_default` require `Default`, which
/// `egui::Id` itself doesn't implement).
#[derive(Clone, Copy, Debug, Default)]
struct OptId(Option<Id>);

// ── Memory keys ──────────────────────────────────────────────────────

fn query_id() -> Id {
    Id::new("gorbie_search_query")
}
fn last_req_id() -> Id {
    Id::new("gorbie_search_last_requested_frame")
}
fn matches_id() -> Id {
    Id::new("gorbie_search_matches")
}
fn cleared_frame_id() -> Id {
    Id::new("gorbie_search_matches_cleared_frame")
}
fn focused_key() -> Id {
    Id::new("gorbie_search_focused")
}
fn scroll_to_key() -> Id {
    Id::new("gorbie_search_scroll_to")
}
fn focus_index_id() -> Id {
    Id::new("gorbie_search_focus_index")
}

// ── Public types ─────────────────────────────────────────────────────

/// Returned by [`SearchSession::report`] for each match the widget
/// emits this frame.
#[derive(Clone, Copy, Debug)]
pub struct MatchInfo {
    /// 0-based index of this match within the session's emit order.
    pub index: usize,
    /// True if this is the match the user currently has "focused" via
    /// the bar's prev/next buttons.
    pub is_focused: bool,
    /// True for one frame after the user clicked prev/next and landed
    /// on this match. Widgets typically call `response.scroll_to_me()`
    /// to bring themselves into view.
    pub should_scroll_to: bool,
}

/// Per-widget search handle. Created by [`crate::CardCtx::search`];
/// see module docs.
pub struct SearchSession {
    ctx: egui::Context,
    query: String,
    focused_id: Option<Id>,
    scroll_to: Option<Id>,
    matches_emitted: usize,
}

impl SearchSession {
    /// The current search query string (empty when no search is active).
    pub fn query(&self) -> &str {
        &self.query
    }

    /// True when the user has typed something into the bar.
    pub fn is_active(&self) -> bool {
        !self.query.is_empty()
    }

    /// Report a match. `id` must be stable across frames for the same
    /// logical match so prev/next navigation lands consistently.
    pub fn report(&mut self, id: Id) -> MatchInfo {
        let info = MatchInfo {
            index: self.matches_emitted,
            is_focused: self.focused_id == Some(id),
            should_scroll_to: self.scroll_to == Some(id),
        };
        // The scroll request fires for exactly one frame.
        if info.should_scroll_to {
            self.scroll_to = None;
            self.ctx
                .data_mut(|d| d.insert_temp(scroll_to_key(), OptId(None)));
        }
        self.matches_emitted += 1;
        self.ctx.data_mut(|d| {
            let list: &mut Vec<Id> = d.get_temp_mut_or_default(matches_id());
            list.push(id);
        });
        info
    }
}

// ── Crate-internal helpers ───────────────────────────────────────────

pub(crate) fn new_session(ctx: egui::Context) -> SearchSession {
    let frame = ctx.cumulative_frame_nr();
    let (query, focused_id, scroll_to) = ctx.data_mut(|d| {
        d.insert_persisted(last_req_id(), frame);
        // Reset the matches list at most once per frame.
        let last_cleared: u64 = d.get_temp(cleared_frame_id()).unwrap_or(u64::MAX);
        if last_cleared != frame {
            d.insert_temp::<Vec<Id>>(matches_id(), Vec::new());
            d.insert_temp::<u64>(cleared_frame_id(), frame);
        }
        let query = d.get_persisted::<String>(query_id()).unwrap_or_default();
        let focused = d.get_temp::<OptId>(focused_key()).and_then(|w| w.0);
        let scroll = d.get_temp::<OptId>(scroll_to_key()).and_then(|w| w.0);
        (query, focused, scroll)
    });
    SearchSession {
        ctx,
        query,
        focused_id,
        scroll_to,
        matches_emitted: 0,
    }
}

/// Render the search bar in the top-right when at least one widget
/// requested it in the last couple of frames.
pub(crate) fn render_bar(ctx: &egui::Context) {
    const RECENT_FRAMES: u64 = 2;
    let frame = ctx.cumulative_frame_nr();
    let (last_req, mut query, matches, focus_index) = ctx.data_mut(|d| {
        let last_req: u64 = d.get_persisted(last_req_id()).unwrap_or(0);
        let query: String = d.get_persisted(query_id()).unwrap_or_default();
        let matches: Vec<Id> = d.get_temp(matches_id()).unwrap_or_default();
        let focus_index: usize = d.get_temp(focus_index_id()).unwrap_or(0);
        (last_req, query, matches, focus_index)
    });
    if last_req == 0 || frame.saturating_sub(last_req) > RECENT_FRAMES {
        return;
    }
    let total = matches.len();
    // Keep focus_index in range; reset on empty.
    let focus_index = if total == 0 { 0 } else { focus_index.min(total - 1) };

    Area::new(Id::new("gorbie_search_bar"))
        .anchor(Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            // Without this, the Area's ui has unbounded `available_width`,
            // and the LCD TextField inside `wrap_width = available_width
            // - margin` blows up to ~infinity — the bar then anchors past
            // both screen edges. Cap to a sane bar width.
            ui.set_max_width(380.0);
            let visuals = ui.visuals();
            let outline = visuals.widgets.noninteractive.bg_stroke.color;
            Frame::new()
                .fill(visuals.window_fill)
                .stroke(egui::Stroke::new(1.0, outline))
                .shadow(egui::epaint::Shadow {
                    offset: [3, 3],
                    blur: 0,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(48),
                })
                .corner_radius(egui::CornerRadius::ZERO)
                .inner_margin(Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // `engaged` = the user has already navigated (a
                        // previous nav set the focused widget). Drives
                        // both the progress fill below and the nav-key
                        // logic further down (first press after typing
                        // lands on match 0 rather than cycling forward
                        // from a stale index).
                        let engaged = ctx.data(|d| {
                            d.get_temp::<OptId>(focused_key())
                                .and_then(|w| w.0)
                                .is_some()
                        });
                        // Classic left-to-right progress fill driven by
                        // the navigation index. Empty fill before the
                        // user navigates (just the dim "unlit" track
                        // showing matches exist); each ◀/▶/Enter step
                        // grows the fill toward 100% at the last match.
                        let progress = if !query.is_empty() && total > 0 {
                            if engaged {
                                Some(0.0..((focus_index + 1) as f32 / total as f32))
                            } else {
                                Some(0.0..0.0)
                            }
                        } else {
                            None
                        };
                        let response =
                            ui.add(TextField::singleline(&mut query).progress(progress));
                        // Enter while the field has focus = explicit
                        // "go to match" action, same as clicking ▶.
                        let enter_pressed = response.has_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let mut new_focus: Option<usize> = None;
                        if response.changed() {
                            // Typing only updates the highlight set —
                            // it does NOT auto-jump or scroll anywhere.
                            // Focus is cleared so the next nav action
                            // (Enter or ◀/▶) selects the first match
                            // fresh rather than cycling from a stale
                            // index that may no longer match.
                            ctx.data_mut(|d| {
                                d.insert_persisted(query_id(), query.clone());
                                d.insert_temp(focus_index_id(), 0usize);
                                d.insert_temp(focused_key(), OptId(None));
                                d.insert_temp(scroll_to_key(), OptId(None));
                            });
                        }
                        // Nav buttons stay visible at all times so the
                        // bar's layout doesn't reshuffle as the user
                        // types — they just disable when there's no
                        // query / no matches to act on.
                        let nav_enabled = !query.is_empty() && total > 0;
                        let clear_enabled = !query.is_empty();
                        let prev = ui
                            .add_enabled(
                                nav_enabled,
                                Button::new("\u{25C0}").modules(2),
                            )
                            .on_hover_text("Previous match");
                        if prev.clicked() {
                            let next = if !engaged {
                                0
                            } else if focus_index == 0 {
                                total - 1
                            } else {
                                focus_index - 1
                            };
                            new_focus = Some(next);
                        }
                        let nxt = ui
                            .add_enabled(
                                nav_enabled,
                                Button::new("\u{25B6}").modules(2),
                            )
                            .on_hover_text("Next match");
                        if nxt.clicked() {
                            let next = if !engaged {
                                0
                            } else {
                                (focus_index + 1) % total
                            };
                            new_focus = Some(next);
                        }
                        if enter_pressed && nav_enabled {
                            let next = if !engaged {
                                0
                            } else {
                                (focus_index + 1) % total
                            };
                            new_focus = Some(next);
                        }
                        if ui
                            .add_enabled(
                                clear_enabled,
                                Button::new("\u{2715}").modules(2),
                            )
                            .on_hover_text("Clear")
                            .clicked()
                        {
                            query.clear();
                            ctx.data_mut(|d| {
                                d.insert_persisted(query_id(), String::new());
                                d.insert_temp(focused_key(), OptId(None));
                                d.insert_temp(scroll_to_key(), OptId(None));
                                d.insert_temp(focus_index_id(), 0usize);
                            });
                        }

                        if let Some(idx) = new_focus {
                            ctx.data_mut(|d| {
                                d.insert_temp(focus_index_id(), idx);
                                let pick = matches.get(idx).copied();
                                d.insert_temp(focused_key(), OptId(pick));
                                d.insert_temp(scroll_to_key(), OptId(pick));
                            });
                        }
                    });
                });
        });
}
