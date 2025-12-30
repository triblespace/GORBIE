use crate::cards::Card;
use crate::cards::CardContext;
use crate::state::StateId;
use crate::Notebook;

type StatefulCardFn<T> = dyn FnMut(&mut CardContext, &mut T);

pub struct StatefulCard<T> {
    state: StateId<T>,
    function: Box<StatefulCardFn<T>>,
    code: Option<String>,
}

impl<T: std::fmt::Debug + std::default::Default + 'static> Card for StatefulCard<T> {
    fn draw(&mut self, ctx: &mut CardContext) {
        let CardContext { ui, store } = ctx;
        let state = self.state;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let mut ctx = CardContext::new(ui, store);
                let mut current = store
                    .read_mut(state)
                    .expect("state handle missing from store");
                (self.function)(&mut ctx, &mut current);
            });
    }

    fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
}

pub fn stateful_card<T: std::fmt::Debug + std::default::Default + Send + Sync + 'static>(
    nb: &mut Notebook,
    init: T,
    function: impl FnMut(&mut CardContext, &mut T) + 'static,
    code: Option<&str>,
) -> StateId<T> {
    let state = nb.state_store.insert(init);
    let handle = state;
    nb.push(Box::new(StatefulCard {
        state,
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
    }));
    handle
}
