use std::any::Any;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;

use crate::dataflow::Dependency;

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StateId<T> {
    key: StateKey,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Copy for StateId<T> {}

impl<T> Clone for StateId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ErasedStateId {
    key: StateKey,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct StateKey(u64);

impl<T> StateId<T> {
    fn new(key: StateKey) -> Self {
        Self {
            key,
            _marker: PhantomData,
        }
    }

    fn from_erased(erased: ErasedStateId) -> Self {
        Self {
            key: erased.key,
            _marker: PhantomData,
        }
    }

    pub fn erase(self) -> ErasedStateId {
        ErasedStateId { key: self.key }
    }
}

#[derive(Copy, Clone)]
pub struct DependencyKey {
    id: ErasedStateId,
    generation: fn(&StateStore, ErasedStateId) -> Option<usize>,
}

impl DependencyKey {
    pub fn new<T: Dependency + 'static>(id: StateId<T>) -> Self {
        Self {
            id: id.erase(),
            generation: generation_for::<T>,
        }
    }

    pub(crate) fn generation(&self, store: &StateStore) -> Option<usize> {
        (self.generation)(store, self.id)
    }
}

fn generation_for<T: Dependency + 'static>(store: &StateStore, id: ErasedStateId) -> Option<usize> {
    store
        .try_with_state(StateId::from_erased(id), |state: &T| state.generation())
        .and_then(|generation| generation)
}

pub struct StateStore {
    next_id: AtomicU64,
    entries: RwLock<HashMap<StateKey, Arc<StateEntry>>>,
}

struct StateEntry {
    value: Box<dyn Any + Send + Sync>,
    type_name: &'static str,
}

impl StateStore {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn insert<T: Send + Sync + 'static>(&self, value: T) -> StateId<T> {
        let key = StateKey(self.next_id.fetch_add(1, Ordering::Relaxed));
        let entry = StateEntry {
            value: Box::new(Arc::new(RwLock::new(value))),
            type_name: std::any::type_name::<T>(),
        };
        self.entries.write().insert(key, Arc::new(entry));
        StateId::new(key)
    }

    pub fn read<T: Clone + 'static>(&self, id: StateId<T>) -> Option<T> {
        let state = self.state(id)?;
        let guard = state.read();
        Some(guard.clone())
    }

    pub fn try_read<T: Clone + 'static>(&self, id: StateId<T>) -> Option<T> {
        let state = self.state(id)?;
        let guard = state.try_read()?;
        Some(guard.clone())
    }

    pub fn read_dependency<T: Dependency + 'static>(&self, id: StateId<T>) -> Option<T::Value> {
        self.with_state(id, |state| state.ready())
            .and_then(|value| value)
    }

    pub fn try_read_dependency<T: Dependency + 'static>(&self, id: StateId<T>) -> Option<T::Value> {
        self.try_with_state(id, |state| state.ready())
            .and_then(|value| value)
    }

    pub fn with_state<T: 'static, R>(&self, id: StateId<T>, f: impl FnOnce(&T) -> R) -> Option<R> {
        let state = self.state(id)?;
        let guard = state.read();
        Some(f(&guard))
    }

    pub fn try_with_state<T: 'static, R>(
        &self,
        id: StateId<T>,
        f: impl FnOnce(&T) -> R,
    ) -> Option<R> {
        let state = self.state(id)?;
        let guard = state.try_read()?;
        Some(f(&guard))
    }

    pub fn with_state_mut<T: 'static, R>(
        &self,
        id: StateId<T>,
        f: impl FnOnce(&mut T) -> R,
    ) -> Option<R> {
        let state = self.state(id)?;
        let mut guard = state.write();
        Some(f(&mut guard))
    }

    fn state<T: 'static>(&self, id: StateId<T>) -> Option<Arc<RwLock<T>>> {
        let entry = self.entry(id.key)?;
        let state = entry
            .value
            .downcast_ref::<Arc<RwLock<T>>>()
            .unwrap_or_else(|| {
                panic!(
                    "state type mismatch: expected {}, got {}",
                    std::any::type_name::<T>(),
                    entry.type_name
                );
            });
        Some(Arc::clone(state))
    }

    fn entry(&self, key: StateKey) -> Option<Arc<StateEntry>> {
        self.entries.read().get(&key).cloned()
    }
}

pub struct StateReader<'a> {
    store: &'a StateStore,
}

impl<'a> StateReader<'a> {
    pub(crate) fn new(store: &'a StateStore) -> Self {
        Self { store }
    }

    pub fn read<T: Dependency + 'static>(&self, id: StateId<T>) -> Option<T::Value> {
        self.store.read_dependency(id)
    }

    pub fn try_read<T: Dependency + 'static>(&self, id: StateId<T>) -> Option<T::Value> {
        self.store.try_read_dependency(id)
    }
}
