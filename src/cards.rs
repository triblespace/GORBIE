pub mod markdown_card;
pub mod reactive_card;
pub mod stateful_card;
pub mod stateless_card;

use std::sync::Arc;

use egui::{Response, Widget};
pub use markdown_card::*;
use parking_lot::RwLock;
pub use reactive_card::*;
pub use stateful_card::*;
pub use stateless_card::*;

pub trait Card {
    fn draw(&mut self, ui: &mut egui::Ui);
}

impl Widget for &mut dyn Card {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        // Delegate the call to the draw method of the Card trait
        // This allows us to use the Card trait as a Widget in egui
        // without needing to implement Widget directly on each card type.
        ui.group(|ui| {
            self.draw(ui);
        })
        .response
    }
}

pub type CardState<T> = Arc<RwLock<T>>;
