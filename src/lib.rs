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
        }
    }

    pub fn push(&mut self, card: Box<dyn cards::Card>) {
        self.cards.push(card);
        self.code_notes_open.push(false);
    }

    pub fn run(self, name: &str) -> eframe::Result {
        let mut notebook = self;
        notebook.header_title = egui::RichText::new(name.to_uppercase())
            .monospace()
            .strong()
            .into();

        let mut native_options = eframe::NativeOptions::default();
        native_options.persist_window = true;

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
                .show(ui, |ui| {
                    let rect = ui.max_rect();

                    let column_max_width: f32 = 740.0;
                    let column_width = column_max_width.min(rect.width());
                    let side_margin_width = ((rect.width() - column_width) / 2.0).max(0.0);

                    let left_margin = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(rect.min.x + side_margin_width, rect.max.y),
                    );
                    let column_rect = egui::Rect::from_min_max(
                        egui::pos2(left_margin.max.x, rect.min.y),
                        egui::pos2(left_margin.max.x + column_width, rect.max.y),
                    );
                    let right_margin = egui::Rect::from_min_max(
                        egui::pos2(column_rect.max.x, rect.min.y),
                        rect.max,
                    );

                    paint_dot_grid(ui, left_margin);
                    paint_dot_grid(ui, right_margin);

                    ui.scope_builder(egui::UiBuilder::new().max_rect(column_rect), |ui| {
                        ui.set_min_size(column_rect.size());

                        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                        let fill = ui.visuals().window_fill;

                        let column_inner_margin = egui::Margin::symmetric(16, 12);
                        let code_note_width = right_margin.width().clamp(260.0, 480.0);

                        egui::Frame::new()
                            .fill(fill)
                            .stroke(stroke)
                            .corner_radius(0.0)
                            .inner_margin(column_inner_margin)
                            .show(ui, |ui| {
                                // Theme switch is part of the page header (above the first card).
                                ui.horizontal(|ui| {
                                    if !self.header_title.is_empty() {
                                        ui.add(
                                            egui::Label::new(self.header_title.clone()).truncate(),
                                        );
                                    }

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.scope(|ui| {
                                                let dark_mode = ui.visuals().dark_mode;
                                                let bg = ui.visuals().window_fill;

                                                if dark_mode {
                                                    let widgets = &mut ui.visuals_mut().widgets;
                                                    widgets.inactive.bg_fill = bg;
                                                    widgets.inactive.weak_bg_fill = bg;
                                                    widgets.hovered.bg_fill = bg;
                                                    widgets.hovered.weak_bg_fill = bg;
                                                }

                                                global_theme_switch(ui);
                                            });
                                        },
                                    );
                                });

                                ui.add_space(12.0);

                                let divider_x_range =
                                    ui.max_rect().x_range().expand(column_inner_margin.leftf());

                                self.code_notes_open.resize(self.cards.len(), false);

                                ui.style_mut().spacing.item_spacing.y = 0.0;
                                for (i, card) in self.cards.iter_mut().enumerate() {
                                    let code_note_open = self
                                        .code_notes_open
                                        .get_mut(i)
                                        .expect("code_notes_open synced to cards");
                                    ui.push_id(i, |ui| {
                                        let card: &mut dyn cards::Card = card.as_mut();
                                        let inner = egui::Frame::group(ui.style())
                                            .stroke(egui::Stroke::NONE)
                                            .corner_radius(0.0)
                                            .show(ui, |ui| {
                                                ui.reset_style();
                                                ui.set_width(ui.available_width());
                                                card.draw(ui);

                                                let Some(code) = card.code() else {
                                                    return;
                                                };

                                                ui.add_space(8.0);
                                                let button_size = egui::vec2(24.0, 24.0);
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(egui::Align::Center),
                                                    |ui| {
                                                        let code_btn = ui
                                                            .add_sized(
                                                                button_size,
                                                                egui::Button::new(
                                                                    egui::RichText::new("{}")
                                                                        .monospace(),
                                                                ),
                                                            )
                                                            .on_hover_text("Toggle code note");

                                                        if code_btn.clicked() {
                                                            *code_note_open = !*code_note_open;
                                                        }

                                                        if *code_note_open {
                                                            let _ = crate::widgets::pinned_note(
                                                                ui,
                                                                &code_btn,
                                                                code_note_open,
                                                                egui::RectAlign::RIGHT_END,
                                                                code_note_width,
                                                                |ui| {
                                                                    egui::ScrollArea::both()
                                                                        .auto_shrink([false; 2])
                                                                        .max_height(320.0)
                                                                        .show(ui, |ui| {
                                                                            ui.add(
                                                                                egui::Label::new(
                                                                                    egui::RichText::new(code)
                                                                                        .monospace(),
                                                                                )
                                                                                .selectable(true)
                                                                                .wrap_mode(egui::TextWrapMode::Extend),
                                                                            );
                                                                        });
                                                                },
                                                            );
                                                        }
                                                    },
                                                );
                                            });

                                        ui.painter().hline(
                                            divider_x_range,
                                            inner.response.rect.top(),
                                            ui.visuals().widgets.noninteractive.bg_stroke,
                                        );

                                        if card.is_updating() {
                                            let rect = inner.response.rect.shrink(2.0);
                                            let painter = ui.painter().with_clip_rect(rect);

                                            let stripe_spacing = 10.0;
                                            let stripe_width = 1.0;
                                            let stripe_color = {
                                                let [r, g, b, _] =
                                                    ui.visuals().hyperlink_color.to_srgba_unmultiplied();
                                                egui::Color32::from_rgba_unmultiplied(r, g, b, 42)
                                            };

                                            let stroke = egui::Stroke::new(stripe_width, stripe_color);
                                            let h = rect.height();

                                            let mut x = rect.left() - h;
                                            while x < rect.right() + h {
                                                painter.line_segment(
                                                    [
                                                        egui::pos2(x, rect.top()),
                                                        egui::pos2(x + h, rect.bottom()),
                                                    ],
                                                    stroke,
                                                );
                                                x += stripe_spacing;
                                            }
                                        }
                                    });
                                }
                            });
                    });
                });
        });
    }
}

fn paint_dot_grid(ui: &egui::Ui, rect: egui::Rect) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    let painter = ui.painter_at(rect);

    let spacing = 18.0;
    let radius = 1.2;
    let color = ui
        .visuals()
        .widgets
        .noninteractive
        .bg_stroke
        .color
        .gamma_multiply(0.35);

    let start_x = (rect.left() / spacing).floor() * spacing + spacing / 2.0;
    let start_y = (rect.top() / spacing).floor() * spacing + spacing / 2.0;

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
