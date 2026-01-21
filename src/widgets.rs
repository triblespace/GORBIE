//! Widgets module — a thin glossary re-exporting individual widget submodules.
//!
//! The actual widget implementations live in `src/widgets/*.rs` so each widget can
//! be edited independently.

pub mod button;
pub mod code;
#[cfg(feature = "polars")]
pub mod dataframe;
pub mod field;
pub mod histogram;
pub mod label;
pub mod load;
#[cfg(feature = "markdown")]
pub mod markdown;
pub mod progress;
pub mod slider;
#[cfg(feature = "triblespace")]
pub mod triblespace;

pub use button::Button;
pub use button::ChoiceToggle;
pub use button::ToggleButton;
pub use code::code_view;
#[cfg(feature = "polars")]
pub use dataframe::dataframe;
pub use field::NumberField;
pub use field::TextField;
pub use histogram::Histogram;
pub use histogram::HistogramBucket;
pub use histogram::HistogramYAxis;
pub use label::row_label;
pub use load::load_auto;
pub use load::load_button;
#[cfg(feature = "markdown")]
pub use markdown::markdown;
pub use progress::ProgressBar;
pub use slider::Slider;
pub use slider::SliderClamping;
