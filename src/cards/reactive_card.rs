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

        let is_updating = matches!(
            &*current,
            ComputedState::Init(_) | ComputedState::Stale(_, _, _)
        );
        if is_updating {
            ui.ctx().request_repaint();
        }

        let value_text = match &*current {
            ComputedState::Ready(value, _) => format!("{value:?}"),
            ComputedState::Stale(previous, _, _) => format!("{previous:?}"),
            _ => "…".to_owned(),
        };

        let font_id = egui::TextStyle::Monospace.resolve(ui.style());
        let row_height = ui.fonts(|fonts| fonts.row_height(&font_id)) + 8.0;
        let (rect, _response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), row_height),
            egui::Sense::hover(),
        );

        if is_updating {
            let painter = ui.painter().with_clip_rect(rect);

            let stripe_spacing = 10.0;
            let stripe_width = 1.0;
            let stripe_color = ui.visuals().widgets.noninteractive.bg_stroke.color;

            let stroke = egui::Stroke::new(stripe_width, stripe_color);
            let h = rect.height();

            let mut x = rect.left() - h;
            while x < rect.right() + h {
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x + h, rect.bottom())],
                    stroke,
                );
                x += stripe_spacing;
            }
        }

        ui.painter().text(
            egui::pos2(rect.left() + 16.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            value_text,
            font_id,
            ui.visuals().text_color(),
        );
    }

    fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
}
