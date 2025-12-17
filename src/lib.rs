//! ## Working with mutable/non-cloneable things.
//! Sometimes when working with existing code, libraries or even std things like
//! files, can introduce an impedance mismatch with the reactive data-flow model.
//! Often it is enough to wrap the object in question into another layer of `Arc`s
//! and `RWLock`s in addition to what Gorby already does with it's `CardState`.
//!
//! - This is also why we compare explicit generations instead of return values,
//! to broaden the range of types that can be used with `derive!`. -
//!
//! But sometimes that isn't enough, e.g. when you want to display some application
//! global state. This is why `state!` and `view!` are carefully designed to not
//! rely on the dataflow mechanisms introduced by `derive`. Instead they can be
//! used, like any other mutable rust type, modulo the `CardState` wrapper.
//!

#![allow(non_snake_case)]

pub mod cards;
pub mod dataflow;
pub mod prelude;
pub mod themes;
pub mod widgets;

use eframe::egui::{self};
use egui_theme_switch::global_theme_switch;

use crate::themes::industrial_dark;
use crate::themes::industrial_fonts;
use crate::themes::industrial_light;

/// A notebook is a collection of cards.
/// Each card is a piece of content that can be displayed in the notebook.
/// Cards can be stateless, stateful, or reactively derived from other cards.
pub struct Notebook {
    header_title: egui::WidgetText,
    pub cards: Vec<Box<dyn cards::Card + 'static>>,
    code_notes_open: Vec<bool>,
    code_note_heights: Vec<f32>,
}

impl Default for Notebook {
    fn default() -> Self {
        Self::new()
    }
}

impl Notebook {
    pub fn new() -> Self {
        Self {
            header_title: egui::WidgetText::default(),
            cards: Vec::new(),
            code_notes_open: Vec::new(),
            code_note_heights: Vec::new(),
        }
    }

    pub fn push(&mut self, card: Box<dyn cards::Card>) {
        self.cards.push(card);
        self.code_notes_open.push(false);
        self.code_note_heights.push(0.0);
    }

    pub fn run(self, name: &str) -> eframe::Result {
        let mut notebook = self;
        notebook.header_title = egui::RichText::new(name.to_uppercase())
            .monospace()
            .strong()
            .into();

        let mut native_options = eframe::NativeOptions::default();
        native_options.persist_window = true;
        native_options.viewport = native_options
            .viewport
            .with_inner_size(egui::vec2(1200.0, 800.0))
            .with_min_inner_size(egui::vec2(480.0, 360.0));

        eframe::run_native(
            name,
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

                    let column_max_width: f32 = 740.0;
                    let column_width = column_max_width.min(rect.width());
                    let remaining_width = (rect.width() - column_width).max(0.0);
                    let left_margin_width = (remaining_width * 0.10).min(120.0);

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

                        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                        let fill = ui.visuals().window_fill;

                        let column_inner_margin = egui::Margin::symmetric(0, 12);
                        let code_note_width = (right_margin.width() - 16.0).max(260.0);

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
                                            ui.scope(|ui| {
                                                let dark_mode = ui.visuals().dark_mode;
                                                let bg = ui.visuals().window_fill;
                                                let outline =
                                                    ui.visuals().widgets.noninteractive.bg_stroke;

                                                if dark_mode {
                                                    let widgets = &mut ui.visuals_mut().widgets;
                                                    widgets.inactive.bg_fill = bg;
                                                    widgets.inactive.weak_bg_fill = bg;
                                                    widgets.hovered.bg_fill = bg;
                                                    widgets.hovered.weak_bg_fill = bg;
                                                }

                                                let visuals = ui.visuals_mut();
                                                visuals.window_fill = crate::themes::ral(1003);
                                                visuals.window_stroke = outline;
                                                visuals.override_text_color =
                                                    Some(crate::themes::ral(9011));

                                                global_theme_switch(ui);
                                            });
                                        },
                                    );
                                });

                                ui.add_space(12.0);

                                let divider_x_range = ui.max_rect().x_range();

                                self.code_notes_open.resize(self.cards.len(), false);
                                self.code_note_heights.resize(self.cards.len(), 0.0);

                                ui.style_mut().spacing.item_spacing.y = 0.0;
                                for (i, card) in self.cards.iter_mut().enumerate() {
                                    let code_note_open = self
                                        .code_notes_open
                                        .get_mut(i)
                                        .expect("code_notes_open synced to cards");
                                    let code_note_height = self
                                        .code_note_heights
                                        .get_mut(i)
                                        .expect("code_note_heights synced to cards");
                                    ui.push_id(i, |ui| {
                                        let card: &mut dyn cards::Card = card.as_mut();
                                        let inner = egui::Frame::group(ui.style())
                                            .stroke(egui::Stroke::NONE)
                                            .corner_radius(0.0)
                                            .inner_margin(egui::Margin::ZERO)
                                            .show(ui, |ui| {
                                                ui.reset_style();
                                                ui.set_width(ui.available_width());
                                                card.draw(ui);
                                            });

                                        ui.painter().hline(
                                            divider_x_range,
                                            inner.response.rect.top(),
                                            ui.visuals().widgets.noninteractive.bg_stroke,
                                        );

                                        let Some(code) = card.code() else {
                                            return;
                                        };

                                        let flag_left_x = inner.response.rect.right() + 8.0;
                                        let flag_size = egui::vec2(18.0, 32.0);
                                        let flag_pos_y = if *code_note_open {
                                            let note_height = if *code_note_height > 0.0 {
                                                *code_note_height
                                            } else {
                                                240.0
                                            };
                                            let padding = 8.0;
                                            let min_top = clip_rect.min.y + padding;
                                            let max_top = clip_rect.max.y - note_height - padding;
                                            if max_top < min_top {
                                                min_top
                                            } else {
                                                inner.response.rect.top().clamp(min_top, max_top)
                                            }
                                        } else {
                                            inner.response.rect.center().y - flag_size.y / 2.0
                                        };
                                        let flag_pos = egui::pos2(flag_left_x, flag_pos_y);

                                        let flag_id = ui.id().with("code_flag");
                                        let flag_resp = egui::Area::new(flag_id)
                                            .order(egui::Order::Foreground)
                                            .fixed_pos(flag_pos)
                                            .movable(false)
                                            .constrain_to(egui::Rect::EVERYTHING)
                                            .show(ui.ctx(), |ui| {
                                                if *code_note_open {
                                                    let outline = ui
                                                        .visuals()
                                                        .widgets
                                                        .noninteractive
                                                        .bg_stroke
                                                        .color;
                                                    let shadow_color = crate::themes::ral(9004);
                                                    let shadow = egui::epaint::Shadow {
                                                        offset: [4, 4],
                                                        blur: 0,
                                                        spread: 0,
                                                        color: shadow_color,
                                                    };

                                                    let note_width = (right_margin.max.x
                                                        - flag_left_x)
                                                        .max(56.0)
                                                        .min(code_note_width);
                                                    ui.set_width(note_width);

                                                    let frame = egui::Frame::new()
                                                        .fill(crate::themes::ral(1003))
                                                        .stroke(egui::Stroke::new(1.0, outline))
                                                        .shadow(shadow)
                                                        .corner_radius(0.0)
                                                        .inner_margin(egui::Margin::same(10));
                                                    let inner = frame.show(ui, |ui| {
                                                        ui.add(
                                                            egui::Label::new(
                                                                egui::RichText::new(code)
                                                                    .monospace()
                                                                    .color(crate::themes::ral(
                                                                        9011,
                                                                    )),
                                                            )
                                                            .selectable(true)
                                                            .wrap_mode(egui::TextWrapMode::Wrap),
                                                        );
                                                    });
                                                    let resp = inner
                                                        .response
                                                        .interact(egui::Sense::click());
                                                    show_postit_tooltip(
                                                        ui,
                                                        &resp,
                                                        "Hide code note",
                                                    );
                                                    resp
                                                } else {
                                                    let (rect, resp) = ui.allocate_exact_size(
                                                        flag_size,
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
                                                        egui::StrokeKind::Middle,
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
                                                }
                                            })
                                            .inner;

                                        if flag_resp.clicked() {
                                            *code_note_open = !*code_note_open;
                                        }

                                        if *code_note_open {
                                            *code_note_height = flag_resp.rect.height();
                                        }
                                    });
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

#[macro_export]
macro_rules! notebook {
    ($setup:ident) => {
        let mut notebook = Notebook::new();
        $setup(&mut notebook);

        let this_file = file!();
        let filename = std::path::Path::new(this_file)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap();

        notebook.run(filename).unwrap();
    };
}
