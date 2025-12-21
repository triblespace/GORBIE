//! ## Working with mutable/non-cloneable things.
//! Sometimes when working with existing code, libraries or even std things like
//! files, can introduce an impedance mismatch with the reactive data-flow model.
//! Often it is enough to wrap the object in question into another layer of `Arc`s
//! and `RWLock`s in addition to what Gorby already does with its `CardState`.
//! This is also why we compare explicit generations instead of return values, to
//! broaden the range of types that can be used with `derive!`.
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

use crate::themes::industrial_dark;
use crate::themes::industrial_fonts;
use crate::themes::industrial_light;
use eframe::egui::{self};

/// A notebook is a collection of cards.
/// Each card is a piece of content that can be displayed in the notebook.
/// Cards can be stateless, stateful, or reactively derived from other cards.
pub struct Notebook {
    header_title: egui::WidgetText,
    pub cards: Vec<Box<dyn cards::Card + 'static>>,
    code_notes_open: Vec<bool>,
    code_note_offsets: Vec<egui::Vec2>,
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
            code_note_offsets: Vec::new(),
        }
    }

    pub fn push(&mut self, card: Box<dyn cards::Card>) {
        self.cards.push(card);
        self.code_notes_open.push(false);
        self.code_note_offsets.push(egui::Vec2::ZERO);
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

                                ui.style_mut().spacing.item_spacing.y = 0.0;
                                let mut max_code_note_bottom_content_y: Option<f32> = None;
                                for (i, card) in self.cards.iter_mut().enumerate() {
                                    let code_note_open = self
                                        .code_notes_open
                                        .get_mut(i)
                                        .expect("code_notes_open synced to cards");
                                    let code_note_offset = self
                                        .code_note_offsets
                                        .get_mut(i)
                                        .expect("code_note_offsets synced to cards");
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
                                        let flag_pos = if *code_note_open {
                                            egui::pos2(flag_left_x, inner.response.rect.top())
                                                + *code_note_offset
                                        } else {
                                            egui::pos2(
                                                flag_left_x,
                                                inner.response.rect.center().y - flag_size.y / 2.0,
                                            )
                                        };

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
                                                        - flag_pos.x)
                                                        .max(56.0)
                                                        .min(code_note_width);
                                                    ui.set_width(note_width);

                                                    let frame = egui::Frame::new()
                                                        .fill(crate::themes::ral(1003))
                                                        .stroke(egui::Stroke::new(1.0, outline))
                                                        .shadow(shadow)
                                                        .corner_radius(0.0)
                                                        .inner_margin(egui::Margin::ZERO);

                                                    let inner = frame.show(ui, |ui| {
                                                        let handle_height = 18.0;
                                                        let (handle_rect, handle_resp) = ui
                                                            .allocate_exact_size(
                                                                egui::vec2(
                                                                    ui.available_width(),
                                                                    handle_height,
                                                                ),
                                                                egui::Sense::click_and_drag(),
                                                            );
                                                        if handle_resp.dragged() {
                                                            ui.ctx().set_cursor_icon(
                                                                egui::CursorIcon::Grabbing,
                                                            );
                                                            *code_note_offset +=
                                                                handle_resp.drag_delta();
                                                        } else if handle_resp.hovered() {
                                                            ui.ctx().set_cursor_icon(
                                                                egui::CursorIcon::Grab,
                                                            );
                                                        }

                                                        let stripe_color = crate::themes::ral(9004);
                                                        let stripe_stroke =
                                                            egui::Stroke::new(1.0, stripe_color);
                                                        let stripe_x = handle_rect.x_range();
                                                        let stripe_padding = 3.0;
                                                        let stripe_spacing = 3.0;
                                                        let mut stripe_y =
                                                            handle_rect.top() + stripe_padding;
                                                        while stripe_y
                                                            <= handle_rect.bottom() - stripe_padding
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
                                                            &handle_resp,
                                                            "Hide code note",
                                                        );

                                                        ui.add_space(6.0);
                                                        egui::Frame::new()
                                                            .inner_margin(egui::Margin::same(10))
                                                            .show(ui, |ui| {
                                                                ui.add(
                                                                    egui::Label::new(
                                                                        egui::RichText::new(code)
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

                                                        handle_resp
                                                    });

                                                    inner.inner
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
                                            });

                                        let resp = flag_resp.inner;
                                        if resp.clicked() {
                                            *code_note_open = !*code_note_open;
                                        }

                                        if *code_note_open {
                                            let note_bottom_content_y =
                                                flag_resp.response.rect.bottom() - clip_rect.min.y
                                                    + scroll_y;
                                            max_code_note_bottom_content_y = Some(
                                                max_code_note_bottom_content_y
                                                    .unwrap_or(note_bottom_content_y)
                                                    .max(note_bottom_content_y),
                                            );
                                        }
                                    });
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
