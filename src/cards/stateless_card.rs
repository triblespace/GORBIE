use crate::cards::Card;
use crate::cards::CardContext;
use crate::Notebook;

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut CardContext)>,
    code: Option<String>,
}

impl Card for StatelessCard {
    fn draw(&mut self, ctx: &mut CardContext) {
        let CardContext { ui, store } = ctx;
        ui.set_width(ui.available_width());
        let mut ctx = CardContext::new(ui, store);
        (self.function)(&mut ctx);
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
    nb.push(Box::new(StatelessCard {
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
    }));
}
