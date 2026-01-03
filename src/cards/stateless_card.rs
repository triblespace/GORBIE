use crate::cards::Card;
use crate::cards::CardContext;
use crate::Notebook;

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut CardContext)>,
    code: Option<String>,
    inner_margin: egui::Margin,
}

impl Card for StatelessCard {
    fn draw(&mut self, ctx: &mut CardContext) {
        let CardContext { ui, store } = ctx;
        egui::Frame::new()
            .inner_margin(self.inner_margin)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let mut ctx = CardContext::new(ui, store);
                (self.function)(&mut ctx);
            });
    }

    fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
}

pub fn stateless_card(
    nb: &mut Notebook,
    function: impl FnMut(&mut CardContext) + 'static,
    code: Option<&str>,
) {
    stateless_card_with_margin(nb, function, code, egui::Margin::symmetric(16, 12));
}

pub fn stateless_card_full_bleed(
    nb: &mut Notebook,
    function: impl FnMut(&mut CardContext) + 'static,
    code: Option<&str>,
) {
    stateless_card_with_margin(nb, function, code, egui::Margin::ZERO);
}

fn stateless_card_with_margin(
    nb: &mut Notebook,
    function: impl FnMut(&mut CardContext) + 'static,
    code: Option<&str>,
    inner_margin: egui::Margin,
) {
    nb.push(Box::new(StatelessCard {
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
        inner_margin,
    }));
}
