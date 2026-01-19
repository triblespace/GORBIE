use std::hash::Hash;

use crate::cards::Card;
use crate::state::StateId;
use crate::CardCtx;
use crate::NotebookCtx;

type StatefulCardFn<T> = dyn for<'a, 'b> FnMut(&'a mut CardCtx<'b>, &mut T);

pub struct StatefulCard<T> {
    state: StateId<T>,
    init: Option<T>,
    function: Box<StatefulCardFn<T>>,
}

impl<T> StatefulCard<T> {
    pub(crate) fn new(
        state: StateId<T>,
        init: T,
        function: impl for<'a, 'b> FnMut(&'a mut CardCtx<'b>, &mut T) + 'static,
    ) -> Self {
        Self {
            state,
            init: Some(init),
            function: Box::new(function),
        }
    }
}

impl<T: Send + Sync + 'static> Card for StatefulCard<T> {
    fn draw(&mut self, ctx: &mut CardCtx<'_>) {
        let state = self.state;
        let Some(state) = state.state_or_init(ctx.store(), &mut self.init) else {
            return;
        };
        let mut current = state.write_arc();
        (self.function)(ctx, &mut current);
    }
}

pub fn stateful_card<K, T>(
    nb: &mut NotebookCtx,
    key: &K,
    init: T,
    function: impl for<'a, 'b> FnMut(&'a mut CardCtx<'b>, &mut T) + 'static,
) -> StateId<T>
where
    K: Hash + ?Sized,
    T: Send + Sync + 'static,
{
    let state = StateId::new(nb.state_id_for(key));
    let handle = state;
    nb.push(Box::new(StatefulCard::new(state, init, function)));
    handle
}
