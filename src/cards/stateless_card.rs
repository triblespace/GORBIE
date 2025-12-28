use crate::cards::Card;
use crate::Notebook;

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut egui::Ui)>,
    code: Option<String>,
}

impl Card for StatelessCard {
    fn draw(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                (self.function)(ui);
            });
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
