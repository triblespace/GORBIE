use std::sync::Arc;

use parking_lot::RwLock;

use crate::cards::Card;
use crate::Notebook;

use super::CardState;

type StatefulCardFn<T> = dyn FnMut(&mut egui::Ui, &mut T);

pub struct StatefulCard<T> {
    current: CardState<T>,
    function: Box<StatefulCardFn<T>>,
    code: Option<String>,
}

impl<T: std::fmt::Debug + std::default::Default> Card for StatefulCard<T> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let mut current = self.current.write();
                (self.function)(ui, &mut current);
            });
    }

    fn code(&self) -> Option<&str> {
        self.code.as_deref()
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
    }));

    current
}
