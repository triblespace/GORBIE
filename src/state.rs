use std::any::Any;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use eframe::egui;
use parking_lot::RawRwLock;
use parking_lot::RwLock;

pub type ArcReadGuard<T> = parking_lot::lock_api::ArcRwLockReadGuard<RawRwLock, T>;
pub type ArcWriteGuard<T> = parking_lot::lock_api::ArcRwLockWriteGuard<RawRwLock, T>;

#[derive(Debug, Default)]
pub struct StateStore {
    states: RwLock<HashMap<egui::Id, Arc<dyn Any + Send + Sync>>>,
}

impl StateStore {
    pub(crate) fn get<T: Send + Sync + 'static>(&self, id: egui::Id) -> Option<Arc<RwLock<T>>> {
        let entry = self.states.read().get(&id).cloned()?;
        entry.downcast::<RwLock<T>>().ok()
    }

    pub(crate) fn get_or_insert<T: Send + Sync + 'static>(
        &self,
        id: egui::Id,
        init: T,
    ) -> Arc<RwLock<T>> {
        {
            let states = self.states.read();
            if let Some(existing) = states.get(&id) {
                if let Ok(state) = existing.clone().downcast::<RwLock<T>>() {
                    return state;
                }
            }
        }

        let state = Arc::new(RwLock::new(init));
        let erased: Arc<dyn Any + Send + Sync> = state.clone();
        let mut states = self.states.write();
        let entry = states.entry(id).or_insert_with(|| erased.clone());
        entry
            .clone()
            .downcast::<RwLock<T>>()
            .expect("state store type mismatch")
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StateId<T> {
    id: egui::Id,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Copy for StateId<T> {}

impl<T> Clone for StateId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> StateId<T> {
    pub(crate) fn new(id: egui::Id) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    pub(crate) fn id(self) -> egui::Id {
        self.id
    }
}

impl<T: Send + Sync + 'static> StateId<T> {
    pub fn read(self, store: &StateStore) -> Option<ArcReadGuard<T>> {
        let state = self.state_arc(store)?;
        Some(state.read_arc())
    }

    pub fn try_read(self, store: &StateStore) -> Option<ArcReadGuard<T>> {
        let state = self.state_arc(store)?;
        state.try_read_arc()
    }

    pub fn read_mut(self, store: &StateStore) -> Option<ArcWriteGuard<T>> {
        let state = self.state_arc(store)?;
        Some(state.write_arc())
    }

    pub fn try_read_mut(self, store: &StateStore) -> Option<ArcWriteGuard<T>> {
        let state = self.state_arc(store)?;
        state.try_write_arc()
    }

    pub(crate) fn state_or_init(
        self,
        store: &StateStore,
        init: &mut Option<T>,
    ) -> Option<Arc<RwLock<T>>> {
        if let Some(state) = store.get(self.id()) {
            return Some(state);
        }
        let init = init.take()?;
        Some(store.get_or_insert(self.id(), init))
    }

    fn state_arc(self, store: &StateStore) -> Option<Arc<RwLock<T>>> {
        store.get(self.id())
    }
}
