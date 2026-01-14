use crate::cards::Card;
use crate::NotebookFrame;
use eframe::egui;

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut egui::Ui)>,
}

impl Card for StatelessCard {
    fn draw(&mut self, ui: &mut egui::Ui) {
        (self.function)(ui);
    }
}

pub fn stateless_card(
    nb: &mut NotebookFrame<'_>,
    function: impl FnMut(&mut egui::Ui) + 'static,
) {
    nb.push(Box::new(StatelessCard {
        function: Box::new(function),
    }));
}
