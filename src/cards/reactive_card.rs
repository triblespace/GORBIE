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

        let value_resp = match &*current {
            ComputedState::Ready(value, _) => ui.monospace(format!("{value:?}")),
            ComputedState::Stale(previous, _, _) => ui.monospace(format!("{previous:?}")),
            _ => ui.monospace("…"),
        };

        if is_updating {
            let rect =
                egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), value_resp.rect.y_range())
                    .shrink(2.0);
            let painter = ui.painter().with_clip_rect(rect);

            let stripe_spacing = 10.0;
            let stripe_width = 1.0;
            let stripe_color = {
                let outline = ui.visuals().widgets.noninteractive.bg_stroke.color;
                let [r, g, b, _] = outline.to_srgba_unmultiplied();
                egui::Color32::from_rgba_unmultiplied(r, g, b, 120)
            };

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
    }

    fn code(&self) -> Option<&str> {
        self.code.as_deref()
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
