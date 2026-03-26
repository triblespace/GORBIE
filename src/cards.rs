/// Card types and helpers for building notebook content.
pub mod stateful_card;
pub mod stateless_card;

pub use stateful_card::*;
pub use stateless_card::*;

use crate::CardCtx;

/// Default inner margin applied to card frames.
pub const DEFAULT_CARD_PADDING: egui::Margin = egui::Margin::symmetric(16, 12);

/// Wraps content in a styled note frame with default padding and fill color.
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

/// Render a formatted markdown string inside a styled note frame.
///
/// Accepts `format!`-style arguments:
/// ```ignore
/// note!(ui, "Status: **{}**", status);
/// ```
#[cfg(feature = "markdown")]
#[macro_export]
macro_rules! note {
    ($ui:expr, $fmt:expr $(, $args:expr)*) => {{
        let text = format!($fmt $(, $args)*);
        $crate::cards::note_frame($ui, |ui| {
            $crate::widgets::markdown(ui, &text);
        });
    }};
}

/// A drawable notebook card.
pub trait Card {
    /// Renders the card into the given drawing context.
    fn draw(&mut self, ctx: &mut CardCtx<'_>);
}
