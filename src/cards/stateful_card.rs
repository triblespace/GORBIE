use std::hash::Hash;

use crate::cards::Card;
use crate::state::StateId;
use crate::Notebook;
use eframe::egui;

type StatefulCardFn<T> = dyn FnMut(&mut egui::Ui, &mut T);

pub struct StatefulCard<T> {
    state: StateId<T>,
    init: Option<T>,
    function: Box<StatefulCardFn<T>>,
}

impl<T: std::fmt::Debug + std::default::Default + Send + Sync + 'static> Card for StatefulCard<T> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let state = self.state;
        let mut current = state.state_or_init(ui, &mut self.init).write_arc();
        (self.function)(ui, &mut current);
    }
}

pub fn stateful_card<K, T>(
    nb: &mut Notebook,
    key: &K,
    init: T,
    function: impl FnMut(&mut egui::Ui, &mut T) + 'static,
) -> StateId<T>
where
    K: Hash + ?Sized,
    T: std::fmt::Debug + std::default::Default + Send + Sync + 'static,
{
    let state = StateId::new(nb.state_id_for(key));
    let handle = state;
    nb.push(Box::new(StatefulCard {
        state,
        init: Some(init),
        function: Box::new(function),
    }));
    handle
}
