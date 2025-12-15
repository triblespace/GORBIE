use std::sync::Arc;

use parking_lot::RwLock;

use crate::cards::Card;
use crate::Notebook;

use super::CardState;

pub struct StatefulCard<T> {
    current: CardState<T>,
    function: Box<dyn FnMut(&mut egui::Ui, &mut T)>,
    code: Option<String>,
    // UI state for the card so we don't rely on global memory.
    show_value_note: bool,
    show_code_note: bool,
}

impl<T: std::fmt::Debug + std::default::Default> Card for StatefulCard<T> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let mut current = self.current.write();
        (self.function)(ui, &mut current);

        if self.code.is_none() {
            self.show_code_note = false;
        }

        ui.add_space(8.0);

        let button_size = egui::vec2(24.0, 24.0);
        let (row_rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), button_size.y),
            egui::Sense::hover(),
        );

        let left_btn_rect = egui::Rect::from_min_size(row_rect.min, button_size);
        let right_btn_rect = egui::Rect::from_min_size(
            egui::pos2(row_rect.max.x - button_size.x, row_rect.min.y),
            button_size,
        );

        let value_btn = ui
            .scope_builder(egui::UiBuilder::new().max_rect(left_btn_rect), |ui| {
                ui.push_id("marginalia_value_btn", |ui| {
                    ui.add_sized(
                        button_size,
                        egui::Button::new(egui::RichText::new("V").monospace()),
                    )
                    .on_hover_text("Toggle value note")
                })
                .inner
            })
            .inner;

        if value_btn.clicked() {
            self.show_value_note = !self.show_value_note;
        }

        let code_btn = self.code.as_ref().map(|_| {
            ui.scope_builder(egui::UiBuilder::new().max_rect(right_btn_rect), |ui| {
                ui.push_id("marginalia_code_btn", |ui| {
                    ui.add_sized(
                        button_size,
                        egui::Button::new(egui::RichText::new("{}").monospace()),
                    )
                    .on_hover_text("Toggle code note")
                })
                .inner
            })
            .inner
        });

        if code_btn.as_ref().is_some_and(|btn| btn.clicked()) {
            self.show_code_note = !self.show_code_note;
        }

        let screen = ui.ctx().screen_rect();
        let card_rect = ui.max_rect();
        let left_margin_width = (card_rect.left() - screen.left()).max(0.0);
        let right_margin_width = (screen.right() - card_rect.right()).max(0.0);
        let value_note_width = left_margin_width.clamp(220.0, 360.0);
        let code_note_width = right_margin_width.clamp(260.0, 480.0);

        if self.show_value_note {
            let value = &*current;
            let value_text = format!("{value:?}");
            let _ = crate::widgets::pinned_note(
                ui,
                &value_btn,
                &mut self.show_value_note,
                egui::RectAlign::LEFT_END,
                value_note_width,
                |ui| {
                    egui::ScrollArea::both()
                        .auto_shrink([false; 2])
                        .max_height(240.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(value_text).monospace())
                                    .selectable(true)
                                    .wrap_mode(egui::TextWrapMode::Extend),
                            );
                        });
                },
            );
        }

        if self.show_code_note {
            if let (Some(btn), Some(code)) = (code_btn.as_ref(), self.code.as_deref()) {
                let _ = crate::widgets::pinned_note(
                    ui,
                    btn,
                    &mut self.show_code_note,
                    egui::RectAlign::RIGHT_END,
                    code_note_width,
                    |ui| {
                        egui::ScrollArea::both()
                            .auto_shrink([false; 2])
                            .max_height(320.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(code).monospace())
                                        .selectable(true)
                                        .wrap_mode(egui::TextWrapMode::Extend),
                                );
                            });
                    },
                );
            } else {
                self.show_code_note = false;
            }
        }
    }
}

pub fn stateful_card<T: std::fmt::Debug + std::default::Default + 'static>(
    nb: &mut Notebook,
    init: T,
    function: impl FnMut(&mut egui::Ui, &mut T) + 'static,
    code: Option<&str>,
) -> CardState<T> {
    let current = Arc::new(RwLock::new(init));
    nb.push(Box::new(StatefulCard {
        current: current.clone(),
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
        show_value_note: false,
        show_code_note: false,
    }));

    current
}

#[macro_export]
macro_rules! state {
    ($nb:expr, ($($Dep:ident),*), $code:expr) => {
        {
            // We capture the dependencies to ensure they are cloned.
            // Each clone gets assigned it's own let statement.
            // This makes type checking errors more readable.
            $(let $Dep = $Dep.clone();)*
            $crate::cards::stateful_card($nb, Default::default(), $code, Some(stringify!($code)))
        }
    };
    ($nb:expr, ($($Dep:ident),*), $init:expr, $code:expr) => {
        {
            // We capture the dependencies to ensure they are cloned.
            // Each clone gets assigned it's own let statement.
            // This makes type checking errors more readable.
            $(let $Dep = $Dep.clone();)*
            $crate::cards::stateful_card($nb, $init, $code, Some(stringify!($code)))
        }
    };
}
