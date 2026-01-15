use crate::cards::Card;
use crate::Notebook;
use eframe::egui;

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut egui::Ui)>,
}

impl StatelessCard {
    pub(crate) fn new(function: impl FnMut(&mut egui::Ui) + 'static) -> Self {
        Self {
            function: Box::new(function),
        }
    }
}

impl Card for StatelessCard {
    fn draw(&mut self, ui: &mut egui::Ui) {
        (self.function)(ui);
    }
}

pub fn stateless_card(nb: &mut Notebook, function: impl FnMut(&mut egui::Ui) + 'static) {
    nb.push(Box::new(StatelessCard::new(function)));
}
