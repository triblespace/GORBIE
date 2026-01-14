use crate::cards::Card;
use crate::Notebook;
use eframe::egui;

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut egui::Ui)>,
}

impl Card for StatelessCard {
    fn draw(&mut self, ui: &mut egui::Ui) {
        (self.function)(ui);
    }
}

pub fn stateless_card(nb: &mut Notebook, function: impl FnMut(&mut egui::Ui) + 'static) {
    nb.push(Box::new(StatelessCard {
        function: Box::new(function),
    }));
}
