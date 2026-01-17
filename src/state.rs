use std::marker::PhantomData;
use std::sync::Arc;

use eframe::egui;
use parking_lot::RawRwLock;
use parking_lot::RwLock;

pub type ArcReadGuard<T> = parking_lot::lock_api::ArcRwLockReadGuard<RawRwLock, T>;
pub type ArcWriteGuard<T> = parking_lot::lock_api::ArcRwLockWriteGuard<RawRwLock, T>;

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
    pub fn read(self, ui: &egui::Ui) -> Option<ArcReadGuard<T>> {
        let state = self.state_arc(ui)?;
        Some(state.read_arc())
    }

    pub fn try_read(self, ui: &egui::Ui) -> Option<ArcReadGuard<T>> {
        let state = self.state_arc(ui)?;
        state.try_read_arc()
    }

    pub fn read_mut(self, ui: &egui::Ui) -> Option<ArcWriteGuard<T>> {
        let state = self.state_arc(ui)?;
        Some(state.write_arc())
    }

    pub fn try_read_mut(self, ui: &egui::Ui) -> Option<ArcWriteGuard<T>> {
        let state = self.state_arc(ui)?;
        state.try_write_arc()
    }

    pub(crate) fn state_or_init(self, ui: &egui::Ui, init: &mut Option<T>) -> Arc<RwLock<T>>
    where
        T: Default,
    {
        let state_id = self.id();
        ui.ctx().data_mut(|data| {
            data.get_temp_mut_or_insert_with(state_id, || {
                Arc::new(RwLock::new(init.take().unwrap_or_default()))
            })
            .clone()
        })
    }

    fn state_arc(self, ui: &egui::Ui) -> Option<Arc<RwLock<T>>> {
        let state_id = self.id();
        ui.ctx()
            .data_mut(|data| data.get_temp::<Arc<RwLock<T>>>(state_id))
    }
}
