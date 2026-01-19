use std::any::Any;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use eframe::egui;
use parking_lot::RawRwLock;
use parking_lot::RwLock;

use crate::CardCtx;

pub type ArcReadGuard<T> = parking_lot::lock_api::ArcRwLockReadGuard<RawRwLock, T>;
pub type ArcWriteGuard<T> = parking_lot::lock_api::ArcRwLockWriteGuard<RawRwLock, T>;

#[derive(Debug, Default)]
pub struct StateStore {
    states: RwLock<HashMap<egui::Id, Arc<dyn Any + Send + Sync>>>,
}

impl StateStore {
    fn get_raw<T: Send + Sync + 'static>(&self, id: egui::Id) -> Option<Arc<RwLock<T>>> {
        let entry = self.states.read().get(&id).cloned()?;
        entry.downcast::<RwLock<T>>().ok()
    }

    fn get_or_insert_raw<T: Send + Sync + 'static>(
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

    pub(crate) fn get_or_insert<T: Send + Sync + 'static>(
        &self,
        id: StateId<T>,
        init: T,
    ) -> Arc<RwLock<T>> {
        self.get_or_insert_raw(id.id(), init)
    }

    /// Returns the state for the given handle or panics when it is missing.
    /// Use `try_get` when the state may be absent (e.g., handle from another store).
    pub fn get<T: Send + Sync + 'static>(&self, id: StateId<T>) -> Arc<RwLock<T>> {
        self.get_raw(id.id()).unwrap_or_else(|| {
            let type_name = std::any::type_name::<T>();
            panic!(
                "state missing for id {:?} ({type_name}); this usually means the handle was created in a different StateStore. Use try_read/try_read_mut when the state may be absent.",
                id.id()
            )
        })
    }

    /// Returns the state for the given handle if it exists.
    pub fn try_get<T: Send + Sync + 'static>(&self, id: StateId<T>) -> Option<Arc<RwLock<T>>> {
        self.get_raw(id.id())
    }

    /// Returns a read guard for the given handle or panics when missing.
    pub fn read<T: Send + Sync + 'static>(&self, id: StateId<T>) -> ArcReadGuard<T> {
        self.get(id).read_arc()
    }

    /// Returns a read guard for the given handle if it exists and can be read.
    pub fn try_read<T: Send + Sync + 'static>(
        &self,
        id: StateId<T>,
    ) -> Option<ArcReadGuard<T>> {
        self.try_get(id).and_then(|state| state.try_read_arc())
    }

    /// Returns a write guard for the given handle or panics when missing.
    pub fn read_mut<T: Send + Sync + 'static>(&self, id: StateId<T>) -> ArcWriteGuard<T> {
        self.get(id).write_arc()
    }

    /// Returns a write guard for the given handle if it exists and can be written.
    pub fn try_read_mut<T: Send + Sync + 'static>(
        &self,
        id: StateId<T>,
    ) -> Option<ArcWriteGuard<T>> {
        self.try_get(id).and_then(|state| state.try_write_arc())
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
    pub fn read(self, ctx: &CardCtx<'_>) -> ArcReadGuard<T> {
        ctx.store().read(self)
    }

    pub fn try_read(self, ctx: &CardCtx<'_>) -> Option<ArcReadGuard<T>> {
        ctx.store().try_read(self)
    }

    pub fn read_mut(self, ctx: &CardCtx<'_>) -> ArcWriteGuard<T> {
        ctx.store().read_mut(self)
    }

    pub fn try_read_mut(self, ctx: &CardCtx<'_>) -> Option<ArcWriteGuard<T>> {
        ctx.store().try_read_mut(self)
    }
}
