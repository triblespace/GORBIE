pub mod stateful_card;
pub mod stateless_card;

pub use stateful_card::*;
pub use stateless_card::*;

pub const DEFAULT_CARD_PADDING: egui::Margin = egui::Margin::symmetric(16, 12);

pub fn with_padding<R>(
    ui: &mut egui::Ui,
    padding: impl Into<egui::Margin>,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::new()
        .inner_margin(padding)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
}

pub fn note_frame<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let fill = crate::themes::ral(1003);
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::NONE)
        .corner_radius(0.0)
        .inner_margin(DEFAULT_CARD_PADDING)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
}

#[macro_export]
macro_rules! note {
    ($ui:expr, $fmt:expr $(, $args:expr)*) => {{
        $crate::cards::note_frame($ui, |ui| {
            $crate::md!(ui, $fmt $(, $args)*);
        });
    }};
}

pub trait Card {
    fn draw(&mut self, ui: &mut egui::Ui);
}
