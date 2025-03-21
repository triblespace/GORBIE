pub mod markdown_card;
pub mod reactive_card;
pub mod stateful_card;
pub mod stateless_card;

use std::sync::Arc;

pub use markdown_card::*;
use parking_lot::RwLock;
pub use reactive_card::*;
pub use stateful_card::*;
pub use stateless_card::*;

use tribles::prelude::Id;

pub struct CardCtx<'a> {
    ui: &'a mut egui::Ui,
    id: Id,
}

impl CardCtx<'_> {
    pub fn new(ui: &mut egui::Ui, id: Id) -> CardCtx {
        CardCtx { ui, id }
    }
    pub fn ui(&mut self) -> &mut egui::Ui {
        self.ui
    }
    pub fn id(&self) -> Id {
        self.id
    }
}
pub trait Card {
    fn update(&mut self, ctx: &mut CardCtx) -> ();
}

pub type CardState<T> = Arc<RwLock<T>>;
