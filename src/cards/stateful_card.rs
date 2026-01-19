use std::hash::Hash;

use crate::cards::Card;
use crate::state::StateId;
use crate::CardCtx;
use crate::NotebookCtx;

type StatefulCardFn<T> = dyn for<'a, 'b> FnMut(&'a mut CardCtx<'b>, &mut T);

pub struct StatefulCard<T> {
    state: StateId<T>,
    function: Box<StatefulCardFn<T>>,
}

impl<T> StatefulCard<T> {
    pub(crate) fn new(
        state: StateId<T>,
        function: impl for<'a, 'b> FnMut(&'a mut CardCtx<'b>, &mut T) + 'static,
    ) -> Self {
        Self {
            state,
            function: Box::new(function),
        }
    }
}

impl<T: Send + Sync + 'static> Card for StatefulCard<T> {
    fn draw(&mut self, ctx: &mut CardCtx<'_>) {
        let mut current = self.state.read_mut(ctx.store());
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
    nb.state_store().get_or_insert(state.id(), init);
    nb.push(Box::new(StatefulCard::new(state, function)));
    handle
}
