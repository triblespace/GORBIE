use std::any::Any;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::RawRwLock;
use parking_lot::RwLock;

pub type ArcReadGuard<T> = parking_lot::lock_api::ArcRwLockReadGuard<RawRwLock, T>;
pub type ArcWriteGuard<T> = parking_lot::lock_api::ArcRwLockWriteGuard<RawRwLock, T>;

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
struct StateKey(u64);

impl<T> StateId<T> {
    fn new(key: StateKey) -> Self {
        Self {
            key,
            _marker: PhantomData,
        }
    }

    // Intentionally no public accessors; keep the key internal to the store.
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

    pub fn read<T: 'static>(&self, id: StateId<T>) -> Option<ArcReadGuard<T>> {
        let state = self.state(id)?;
        Some(state.read_arc())
    }

    pub fn try_read<T: 'static>(&self, id: StateId<T>) -> Option<ArcReadGuard<T>> {
        let state = self.state(id)?;
        state.try_read_arc()
    }

    pub fn read_mut<T: 'static>(&self, id: StateId<T>) -> Option<ArcWriteGuard<T>> {
        let state = self.state(id)?;
        Some(state.write_arc())
    }

    pub fn try_read_mut<T: 'static>(&self, id: StateId<T>) -> Option<ArcWriteGuard<T>> {
        let state = self.state(id)?;
        state.try_write_arc()
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
