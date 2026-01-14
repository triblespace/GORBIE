use crate::cards::Card;
use crate::state::StateId;
use crate::Notebook;
use eframe::egui;

type StatefulCardFn<T> = dyn FnMut(&mut egui::Ui, &mut T);

pub struct StatefulCard<T> {
    state: StateId<T>,
    init: Option<T>,
    function: Box<StatefulCardFn<T>>,
    code: Option<String>,
}

impl<T: std::fmt::Debug + std::default::Default + Send + Sync + 'static> Card for StatefulCard<T> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let state = self.state;
        let mut current = state.state_or_init(ui, &mut self.init).write_arc();
        (self.function)(ui, &mut current);
    }

    fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
}

pub fn stateful_card<T: std::fmt::Debug + std::default::Default + Send + Sync + 'static>(
    nb: &mut Notebook,
    init: T,
    function: impl FnMut(&mut egui::Ui, &mut T) + 'static,
    code: Option<&str>,
) -> StateId<T> {
    let state = nb.alloc_state_id();
    let handle = state;
    nb.push(Box::new(StatefulCard {
        state,
        init: Some(init),
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
    }));
    handle
}
