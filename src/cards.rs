pub mod reactive_card;
pub mod stateful_card;
pub mod stateless_card;

use std::sync::Arc;

use egui::Response;
use egui::Widget;
use parking_lot::RwLock;
pub use reactive_card::*;
pub use stateful_card::*;
pub use stateless_card::*;

pub trait Card {
    fn draw(&mut self, ui: &mut egui::Ui);

    fn code(&self) -> Option<&str> {
        None
    }

    fn is_updating(&self) -> bool {
        false
    }
}

impl Widget for &mut dyn Card {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        egui::Frame::group(ui.style())
            .stroke(egui::Stroke::NONE)
            .corner_radius(0.0)
            .show(ui, |ui| {
                // Allow the notebook layout to remove inter-card spacing without
                // affecting spacing inside the card content.
                ui.reset_style();
                ui.set_width(ui.available_width());
                self.draw(ui);
            })
            .response
    }
}

pub type CardState<T> = Arc<RwLock<T>>;
