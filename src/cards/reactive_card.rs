use std::{ops::Deref, sync::Arc};

use egui::CollapsingHeader;
use parking_lot::RwLock;

use crate::{ComputedState, Dependencies, Notebook};

use super::{Card, CardCtx, CardState};


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
    nb.push_card(Box::new(ReactiveCard {
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
    fn update(&mut self, ctx: &mut CardCtx) -> () {
        let mut current = self.value.write();
        *current = match std::mem::replace(&mut *current, ComputedState::Undefined) {
            ComputedState::Undefined => {
                let dependencies = self.dependencies.clone();
                let function = self.function.clone();
                let generations = dependencies.try_generations();
                if generations.is_some() {
                    self.generations = generations;
                    ComputedState::Init(std::thread::spawn(move || {
                        let dependencies = dependencies.read().expect("failed to read dependencies, although generations were available");
                        (function)(dependencies)
                    }))
                } else {
                    ComputedState::Undefined
                }
            }

            ComputedState::Init(handle) if handle.is_finished() => {
                ctx.ui().ctx().request_repaint();
                ComputedState::Ready(handle.join().unwrap(), 0)
            }

            ComputedState::Init(handle) => {
                ComputedState::Init(handle)
            }

            ComputedState::Ready(current, generation) => {
                let dependencies = self.dependencies.clone();
                let function = self.function.clone();
                let generations = dependencies.try_generations();
                if generations.is_some() && generations != self.generations {
                    self.generations = generations;
                    ComputedState::Stale(current, generation, std::thread::spawn(move || {
                        let dependencies = dependencies.read().expect("failed to read dependencies, although generations were available");
                        (function)(dependencies)
                    }))
                } else {
                    ComputedState::Ready(current, generation)
                }
            }
            ComputedState::Stale(previous, generation, join_handle)
                if join_handle.is_finished() =>
            {
                let result = join_handle.join().unwrap();
                if result != previous {
                    ctx.ui().ctx().request_repaint();
                    ComputedState::Ready(result, generation + 1)
                } else {
                    ComputedState::Ready(result, generation)
                }
            }

            ComputedState::Stale(current, generation, join_handle) => {
                ComputedState::Stale(current, generation, join_handle)
            }
        };

        CollapsingHeader::new("Current")
        .id_salt("__current")
        .show(ctx.ui(), |ui| {
            match current.deref() {
                ComputedState::Ready(value, _) => {
                    ui.monospace(format!("{:?}", value));
                }
                _ => {
                    ui.add(egui::widgets::Spinner::new());
                }
            }
        });

        if let Some(code) = &mut self.code {
            CollapsingHeader::new("Code")
                .id_salt("__code")
                .show(ctx.ui(), |ui| {
                    let language = "rs";
                    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(
                        ui.ctx(),
                        ui.style(),
                    );
                    egui_extras::syntax_highlighting::code_view_ui(
                        ui, &theme, code, language,
                    );
                });
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
            $crate::reactive_card($nb, ($($Dep),*,), $code, Some(stringify!($code)))
        }
    };
}