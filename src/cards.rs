pub mod reactive_card;
pub mod stateful_card;
pub mod stateless_card;

use std::ops::{Deref, DerefMut};

pub use reactive_card::*;
pub use stateful_card::*;
pub use stateless_card::*;

use crate::dataflow::Dependency;
use crate::state::{StateId, StateStore};

pub struct CardContext<'a> {
    ui: &'a mut egui::Ui,
    store: &'a StateStore,
}

impl<'a> CardContext<'a> {
    pub fn new(ui: &'a mut egui::Ui, store: &'a StateStore) -> Self {
        Self { ui, store }
    }

    pub fn store(&self) -> &StateStore {
        self.store
    }

    pub fn with_state<T: 'static, R>(
        &mut self,
        id: StateId<T>,
        f: impl FnOnce(&mut egui::Ui, &T) -> R,
    ) -> Option<R> {
        let ui = &mut *self.ui;
        let store = self.store;
        store.with_state(id, |state| f(ui, state))
    }

    pub fn with_state_mut<T: 'static, R>(
        &mut self,
        id: StateId<T>,
        f: impl FnOnce(&mut egui::Ui, &mut T) -> R,
    ) -> Option<R> {
        let ui = &mut *self.ui;
        let store = self.store;
        store.with_state_mut(id, |state| f(ui, state))
    }

    pub fn read<T: Clone + 'static>(&self, id: StateId<T>) -> Option<T> {
        self.store.read(id)
    }

    pub fn try_read<T: Clone + 'static>(&self, id: StateId<T>) -> Option<T> {
        self.store.try_read(id)
    }

    pub fn read_dependency<T: Dependency + 'static>(&self, id: StateId<T>) -> Option<T::Value> {
        self.store.read_dependency(id)
    }

    pub fn try_read_dependency<T: Dependency + 'static>(&self, id: StateId<T>) -> Option<T::Value> {
        self.store.try_read_dependency(id)
    }
}

impl Deref for CardContext<'_> {
    type Target = egui::Ui;

    fn deref(&self) -> &Self::Target {
        self.ui
    }
}

impl DerefMut for CardContext<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ui
    }
}

pub trait Card {
    fn draw(&mut self, ctx: &mut CardContext);

    fn code(&self) -> Option<&str> {
        None
    }
}
