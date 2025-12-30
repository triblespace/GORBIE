pub mod reactive_card;
pub mod stateful_card;
pub mod stateless_card;

use std::ops::{Deref, DerefMut};

pub use reactive_card::*;
pub use stateful_card::*;
pub use stateless_card::*;

use crate::dataflow::Dependency;
use crate::state::{ArcReadGuard, ArcWriteGuard, StateId, StateStore};

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

    pub fn read<T: 'static>(&self, id: StateId<T>) -> Option<ArcReadGuard<T>> {
        self.store.read(id)
    }

    pub fn try_read<T: 'static>(&self, id: StateId<T>) -> Option<ArcReadGuard<T>> {
        self.store.try_read(id)
    }

    pub fn read_mut<T: 'static>(&self, id: StateId<T>) -> Option<ArcWriteGuard<T>> {
        self.store.read_mut(id)
    }

    pub fn try_read_mut<T: 'static>(&self, id: StateId<T>) -> Option<ArcWriteGuard<T>> {
        self.store.try_read_mut(id)
    }

    pub fn ready<T: Dependency + 'static>(&self, id: StateId<T>) -> Option<T::Value> {
        self.store.ready(id)
    }

    pub fn try_ready<T: Dependency + 'static>(&self, id: StateId<T>) -> Option<T::Value> {
        self.store.try_ready(id)
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
