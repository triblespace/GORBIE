use crate::cards::Card;
use crate::Notebook;

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut egui::Ui)>,
    code: Option<String>,
    show_code_note: bool,
}

impl Card for StatelessCard {
    fn draw(&mut self, ui: &mut egui::Ui) {
        (self.function)(ui);

        if self.code.is_none() {
            self.show_code_note = false;
            return;
        }

        ui.add_space(8.0);

        let button_size = egui::vec2(24.0, 24.0);
        let (row_rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), button_size.y),
            egui::Sense::hover(),
        );

        let right_btn_rect = egui::Rect::from_min_size(
            egui::pos2(row_rect.max.x - button_size.x, row_rect.min.y),
            button_size,
        );

        let code_btn = ui
            .scope_builder(egui::UiBuilder::new().max_rect(right_btn_rect), |ui| {
                ui.push_id("marginalia_code_btn", |ui| {
                    ui.add_sized(
                        button_size,
                        egui::Button::new(egui::RichText::new("{}").monospace()),
                    )
                    .on_hover_text("Toggle code note")
                })
                .inner
            })
            .inner;

        if code_btn.clicked() {
            self.show_code_note = !self.show_code_note;
        }

        let screen = ui.ctx().screen_rect();
        let card_rect = ui.max_rect();
        let right_margin_width = (screen.right() - card_rect.right()).max(0.0);
        let code_note_width = right_margin_width.clamp(260.0, 480.0);

        if self.show_code_note {
            if let Some(code) = self.code.as_deref() {
                let _ = crate::widgets::pinned_note(
                    ui,
                    &code_btn,
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

pub fn stateless_card(
    nb: &mut Notebook,
    function: impl FnMut(&mut egui::Ui) + 'static,
    code: Option<&str>,
) {
    nb.push(Box::new(StatelessCard {
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
        show_code_note: false,
    }));
}

#[macro_export]
macro_rules! view {
    ($nb:expr, ($($Dep:ident),*), $code:expr) => {
        {
            // We capture the dependencies to ensure they are cloned.
            // Each clone gets assigned it's own let statement.
            // This makes type checking errors more readable.
            $(let $Dep = $Dep.clone();)*
            $crate::cards::stateless_card($nb, $code, Some(stringify!($code)))
        }
    };
}
