use std::sync::Arc;

use parking_lot::RwLock;

use crate::cards::Card;
use crate::dataflow::ComputedState;
use crate::dataflow::Dependencies;
use crate::Notebook;

use super::CardState;

pub struct ReactiveCard<T: Send, D: for<'a> Dependencies + Send> {
    value: CardState<ComputedState<T>>,
    generations: Option<<D as Dependencies>::Generations>,
    dependencies: D,
    function: Arc<dyn Fn(<D as Dependencies>::Values) -> T + Send + Sync>,
    code: Option<String>,
    // UI state for the marginalia notes
    show_value_note: bool,
    show_code_note: bool,
}

pub fn reactive_card<
    T: Send + PartialEq + std::fmt::Debug + std::default::Default + 'static,
    D: for<'a> Dependencies + Send + Clone + 'static,
>(
    nb: &mut Notebook,
    dependencies: D,
    function: impl Fn(<D as Dependencies>::Values) -> T + Send + Sync + 'static,
    code: Option<&str>,
) -> CardState<ComputedState<T>> {
    let current = Arc::new(RwLock::new(ComputedState::Undefined));
    nb.push(Box::new(ReactiveCard {
        value: current.clone(),
        generations: None,
        dependencies,
        function: Arc::new(function),
        code: code.map(|s| s.to_owned()),
        show_value_note: false,
        show_code_note: false,
    }));

    current
}

impl<T: Send + std::fmt::Debug + PartialEq + 'static, D: Dependencies + Send + Clone + 'static> Card
    for ReactiveCard<T, D>
{
    fn draw(&mut self, ui: &mut egui::Ui) {
        let mut current = self.value.write();
        *current = match std::mem::replace(&mut *current, ComputedState::Undefined) {
            ComputedState::Undefined => {
                let dependencies = self.dependencies.clone();
                let function = self.function.clone();
                let generations = dependencies.try_generations();
                if generations.is_some() {
                    self.generations = generations;
                    ComputedState::Init(std::thread::spawn(move || {
                        let dependencies = dependencies.read().expect(
                            "failed to read dependencies, although generations were available",
                        );
                        (function)(dependencies)
                    }))
                } else {
                    ComputedState::Undefined
                }
            }

            ComputedState::Init(handle) if handle.is_finished() => {
                ui.ctx().request_repaint();
                ComputedState::Ready(handle.join().unwrap(), 0)
            }

            ComputedState::Init(handle) => ComputedState::Init(handle),

            ComputedState::Ready(current, generation) => {
                let dependencies = self.dependencies.clone();
                let function = self.function.clone();
                let generations = dependencies.try_generations();
                if generations.is_some() && generations != self.generations {
                    self.generations = generations;
                    ComputedState::Stale(
                        current,
                        generation,
                        std::thread::spawn(move || {
                            let dependencies = dependencies.read().expect(
                                "failed to read dependencies, although generations were available",
                            );
                            (function)(dependencies)
                        }),
                    )
                } else {
                    ComputedState::Ready(current, generation)
                }
            }
            ComputedState::Stale(previous, generation, join_handle)
                if join_handle.is_finished() =>
            {
                let result = join_handle.join().unwrap();
                if result != previous {
                    ui.ctx().request_repaint();
                    ComputedState::Ready(result, generation + 1)
                } else {
                    ComputedState::Ready(result, generation)
                }
            }

            ComputedState::Stale(current, generation, join_handle) => {
                ComputedState::Stale(current, generation, join_handle)
            }
        };

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
                        .show(ui, |ui| match &*current {
                            ComputedState::Ready(value, _) => {
                                let value_text = format!("{value:?}");
                                ui.add(
                                    egui::Label::new(egui::RichText::new(value_text).monospace())
                                        .selectable(true)
                                        .wrap_mode(egui::TextWrapMode::Extend),
                                );
                            }
                            ComputedState::Stale(previous, _, _) => {
                                let value_text = format!("{previous:?}");
                                ui.add(
                                    egui::Label::new(egui::RichText::new(value_text).monospace())
                                        .selectable(true)
                                        .wrap_mode(egui::TextWrapMode::Extend),
                                );
                            }
                            _ => {
                                ui.label(egui::RichText::new("…").monospace());
                            }
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

#[macro_export]
macro_rules! derive {
    ($nb:expr, ($($Dep:ident),*), $code:expr) => {
        {
            // We capture the dependencies to ensure they are cloned.
            // Each clone gets assigned it's own let statement.
            // This makes type checking errors more readable.
            $(let $Dep = $Dep.clone();)*
            $crate::cards::reactive_card($nb, ($($Dep),*,), $code, Some(stringify!($code)))
        }
    };
}
