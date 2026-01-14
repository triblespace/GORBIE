use crate::cards::Card;
use crate::Notebook;
use eframe::egui;

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut egui::Ui)>,
    code: Option<String>,
}

impl Card for StatelessCard {
    fn draw(&mut self, ui: &mut egui::Ui) {
        (self.function)(ui);
    }

    fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
}

pub fn stateless_card(
    nb: &mut Notebook,
    function: impl FnMut(&mut egui::Ui) + 'static,
    code: Option<&str>,
) {
    nb.push(Box::new(StatelessCard {
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
    }));
}
