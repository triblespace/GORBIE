use std::sync::Arc;

use crate::cards::Card;
use crate::cards::CardContext;
use crate::dataflow::ComputedState;
use crate::state::DependencyKey;
use crate::state::StateId;
use crate::state::StateReader;
use crate::state::StateStore;
use crate::Notebook;

pub struct ReactiveCard<T: Send + Sync> {
    value: StateId<ComputedState<T>>,
    generations: Option<Vec<usize>>,
    dependencies: Vec<DependencyKey>,
    function: Arc<dyn Fn(&StateReader) -> T + Send + Sync>,
    store: Arc<StateStore>,
    code: Option<String>,
}

pub fn reactive_card<
    T: Send + Sync + PartialEq + std::fmt::Debug + std::default::Default + 'static,
>(
    nb: &mut Notebook,
    dependencies: Vec<DependencyKey>,
    function: impl Fn(&StateReader) -> T + Send + Sync + 'static,
    code: Option<&str>,
) -> StateId<ComputedState<T>> {
    let value = nb.state_store.insert(ComputedState::Undefined);
    let handle = value;
    nb.push(Box::new(ReactiveCard {
        value,
        generations: None,
        dependencies,
        function: Arc::new(function),
        store: Arc::clone(&nb.state_store),
        code: code.map(|s| s.to_owned()),
    }));

    handle
}

impl<T: Send + Sync + std::fmt::Debug + PartialEq + 'static> Card for ReactiveCard<T> {
    fn draw(&mut self, ui: &mut CardContext) {
        let store = ui.store();
        let value = self.value;
        let dep_generations = self
            .dependencies
            .iter()
            .map(|dep| dep.generation(store))
            .collect::<Option<Vec<_>>>();
        let function = Arc::clone(&self.function);
        let store_handle = Arc::clone(&self.store);

        let mut current = store
            .read_mut(value)
            .expect("reactive state handle missing from store");
        *current = match std::mem::replace(&mut *current, ComputedState::Undefined) {
            ComputedState::Undefined => {
                if let Some(generations) = dep_generations.clone() {
                    self.generations = Some(generations);
                    ComputedState::Init(std::thread::spawn(move || {
                        let reader = StateReader::new(store_handle.as_ref());
                        (function)(&reader)
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
                if let Some(generations) = dep_generations.clone() {
                    if Some(generations.clone()) != self.generations {
                        self.generations = Some(generations);
                        ComputedState::Stale(
                            current,
                            generation,
                            std::thread::spawn(move || {
                                let reader = StateReader::new(store_handle.as_ref());
                                (function)(&reader)
                            }),
                        )
                    } else {
                        ComputedState::Ready(current, generation)
                    }
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

        let is_updating = store
            .read(value)
            .map(|current| {
                matches!(
                    &*current,
                    ComputedState::Init(_) | ComputedState::Stale(_, _, _)
                )
            })
            .unwrap_or(false);
        if is_updating {
            ui.ctx().request_repaint();
        }

        let value_text = store
            .read(value)
            .map(|current| match &*current {
                ComputedState::Ready(value, _) => format!("{value:?}"),
                ComputedState::Stale(previous, _, _) => format!("{previous:?}"),
                _ => "...".to_owned(),
            })
            .unwrap_or_else(|| "...".to_owned());

        let font_id = egui::TextStyle::Monospace.resolve(ui.style());
        let row_height = ui.fonts(|fonts| fonts.row_height(&font_id)) + 8.0;
        let available_width = ui.available_width();
        let (rect, _response) = ui.allocate_exact_size(
            egui::vec2(available_width, row_height),
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
