//! Widgets module — a thin glossary re-exporting individual widget submodules.
//!
//! The actual widget implementations live in `src/widgets/*.rs` so each widget can
//! be edited independently.

pub mod collapsing_divider;
pub mod dataframe;
pub mod load;
pub mod markdown;
pub mod slider;

pub use collapsing_divider::collapsing_divider;
pub use dataframe::dataframe;
pub use load::{load_auto, load_button};
pub use markdown::markdown;
pub use slider::Slider;
