use crate::cards::Card;
use crate::CardCtx;
use crate::NotebookCtx;

/// A card with no persistent state; redrawn from scratch each frame.
pub struct StatelessCard {
    function: Box<dyn for<'a, 'b> FnMut(&'a mut CardCtx<'b>)>,
}

impl StatelessCard {
    pub(crate) fn new(function: impl for<'a, 'b> FnMut(&'a mut CardCtx<'b>) + 'static) -> Self {
        Self {
            function: Box::new(function),
        }
    }
}

impl Card for StatelessCard {
    fn draw(&mut self, ctx: &mut CardCtx<'_>) {
        (self.function)(ctx);
    }
}

/// Creates a stateless card that runs `function` each frame with no retained state.
#[track_caller]
pub fn stateless_card(
    nb: &mut NotebookCtx,
    function: impl for<'a, 'b> FnMut(&'a mut CardCtx<'b>) + 'static,
) {
    nb.view(function);
}
