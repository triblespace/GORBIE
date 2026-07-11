use std::collections::{HashSet, VecDeque};

use eframe::egui::{
    self, pos2, vec2, Align2, CursorIcon, Margin, Rect, Response, RichText, Sense, Stroke,
    TextStyle, TextWrapMode, Ui,
};

use crate::themes::colorhash;

struct FeedItem {
    id: u64,
    category: String,
    summary: String,
    detail: Option<String>,
}

/// A newest-first scrolling feed of events.
///
/// Each row shows a category pill (colored deterministically from the
/// category via [`colorhash::ral_cvd_safe`] — colorblind-safe, and
/// always paired with the category text) and a one-line summary. Rows
/// pushed with a detail string are clickable and expand to show the
/// detail below the summary, marked with a bar in the pill color —
/// the same colored-divider language as `CardCtx::section`.
///
/// ```ignore
/// // In notebook state:
/// let mut feed = EventFeed::new();
/// feed.push("tick", "cycle 41 finished in 62 ms");
/// feed.push_with_detail("plan", "replanned route", "3 candidates, chose B (cost 12.4)");
/// // In the card closure:
/// feed.show(ui);
/// ```
pub struct EventFeed {
    items: VecDeque<FeedItem>,
    next_id: u64,
    max_items: usize,
    desired_height: f32,
    open: HashSet<u64>,
}

impl Default for EventFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl EventFeed {
    /// An empty feed with a 500-event retention cap and a 240px viewport.
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
            next_id: 0,
            max_items: 500,
            desired_height: 240.0,
            open: HashSet::new(),
        }
    }

    /// Cap the number of retained events; the oldest are dropped once
    /// the cap is exceeded. Default is 500.
    pub fn max_items(mut self, max_items: usize) -> Self {
        self.max_items = max_items.max(1);
        self
    }

    /// Set the height of the scrolling viewport in pixels. Default is 240.
    pub fn desired_height(mut self, height: f32) -> Self {
        self.desired_height = height.max(24.0);
        self
    }

    /// Number of events currently retained.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when no events are retained.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Drop all events.
    pub fn clear(&mut self) {
        self.items.clear();
        self.open.clear();
    }

    /// Push an event (it appears at the top of the feed).
    pub fn push(&mut self, category: impl Into<String>, summary: impl Into<String>) {
        self.push_item(category.into(), summary.into(), None);
    }

    /// Push an event with a collapsible detail section.
    pub fn push_with_detail(
        &mut self,
        category: impl Into<String>,
        summary: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.push_item(category.into(), summary.into(), Some(detail.into()));
    }

    fn push_item(&mut self, category: String, summary: String, detail: Option<String>) {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push_front(FeedItem {
            id,
            category,
            summary,
            detail,
        });
        while self.items.len() > self.max_items {
            if let Some(dropped) = self.items.pop_back() {
                self.open.remove(&dropped.id);
            }
        }
    }

    /// Render the feed into `ui`, newest event first.
    pub fn show(&mut self, ui: &mut Ui) -> Response {
        let outline = ui.visuals().widgets.noninteractive.bg_stroke.color;

        let frame = egui::Frame::new()
            .stroke(Stroke::new(1.0, outline))
            .inner_margin(Margin::same(4))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("gorbie_event_feed")
                    .max_height(self.desired_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        for item in &self.items {
                            show_row(ui, item, &mut self.open);
                        }
                    });
            });

        frame.response
    }
}

fn show_row(ui: &mut Ui, item: &FeedItem, open: &mut HashSet<u64>) {
    let pill_color = colorhash::ral_cvd_safe(item.category.as_bytes());
    let pill_text_color = colorhash::text_color_on(pill_color);
    let expandable = item.detail.is_some();
    let is_open = expandable && open.contains(&item.id);

    ui.push_id(item.id, |ui| {
        let row = ui.horizontal(|ui| {
            // Category pill.
            let font_id = TextStyle::Small.resolve(ui.style());
            let galley = ui.fonts_mut(|fonts| {
                fonts.layout_no_wrap(item.category.clone(), font_id, pill_text_color)
            });
            let pad = vec2(6.0, 2.0);
            let (pill_rect, _) = ui.allocate_exact_size(
                galley.size() + pad * 2.0,
                Sense::hover(),
            );
            if ui.is_rect_visible(pill_rect) {
                ui.painter().rect_filled(pill_rect, 2.0, pill_color);
                let placement = Align2::CENTER_CENTER
                    .align_size_within_rect(galley.size(), pill_rect);
                ui.painter().galley(placement.min, galley, pill_text_color);
            }

            // Expansion marker + one-line summary.
            let marker = match (expandable, is_open) {
                (false, _) => "",
                (true, false) => "▸ ",
                (true, true) => "▾ ",
            };
            ui.add(
                egui::Label::new(format!("{marker}{summary}", summary = item.summary))
                    .wrap_mode(TextWrapMode::Truncate),
            );
        });

        if expandable {
            let response = ui.interact(
                row.response.rect,
                ui.id().with("row"),
                Sense::click(),
            );
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if response.clicked() {
                if is_open {
                    open.remove(&item.id);
                } else {
                    open.insert(item.id);
                }
            }
        }

        if is_open {
            if let Some(detail) = item.detail.as_deref() {
                let weak = ui.visuals().weak_text_color();
                let inner = ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.add(
                        egui::Label::new(RichText::new(detail).monospace().color(weak))
                            .wrap_mode(TextWrapMode::Wrap),
                    );
                });
                // Detail marker bar in the pill color, mirroring the
                // colored-section language.
                let rect = inner.response.rect;
                let bar = Rect::from_min_max(
                    pos2(rect.left() + 4.0, rect.top()),
                    pos2(rect.left() + 6.0, rect.bottom()),
                );
                ui.painter().rect_filled(bar, 0.0, pill_color);
            }
        }
    });
}
