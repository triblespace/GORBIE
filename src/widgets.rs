//! Widgets module — a thin glossary re-exporting individual widget submodules.
//!
//! The actual widget implementations live in `src/widgets/*.rs` so each widget can
//! be edited independently.

pub mod load;
pub mod dataframe;
pub mod markdown;
pub mod collapsing_divider;
pub mod slider;

pub use load::{load_auto, load_button};
pub use dataframe::dataframe;
pub use markdown::markdown;
pub use collapsing_divider::collapsing_divider;
pub use slider::Slider;
