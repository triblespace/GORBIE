//! Widgets module — a thin glossary re-exporting individual widget submodules.
//!
//! The actual widget implementations live in `src/widgets/*.rs` so each widget can
//! be edited independently.

pub mod button;
pub mod code;
pub mod collapsing_divider;
pub mod dataframe;
pub mod histogram;
pub mod load;
pub mod marginalia;
pub mod markdown;
pub mod progress;
pub mod slider;

pub use button::Button;
pub use button::ChoiceToggle;
pub use button::ToggleButton;
pub use code::code_view;
pub use collapsing_divider::collapsing_divider;
pub use dataframe::dataframe;
pub use histogram::Histogram;
pub use histogram::HistogramBucket;
pub use histogram::HistogramYAxis;
pub use load::load_auto;
pub use load::load_button;
pub use marginalia::pinned_note;
pub use markdown::markdown;
pub use progress::ProgressBar;
pub use slider::Slider;
