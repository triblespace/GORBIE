//! Widgets module — a thin glossary re-exporting individual widget submodules.
//!
//! The actual widget implementations live in `src/widgets/*.rs` so each widget can
//! be edited independently.

pub mod button;
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
#[cfg(feature = "polars")]
pub mod table;
#[cfg(feature = "polars")]
mod table_layout;
#[cfg(feature = "polars")]
mod table_sizing;
#[cfg(feature = "triblespace")]
pub mod triblespace;

pub use button::Button;
pub use button::ChoiceToggle;
pub use button::RadioButton;
pub use button::ToggleButton;
#[cfg(feature = "polars")]
pub use dataframe::{data_export_tiny, data_summary_tiny, dataframe, dataframe_summary};
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
#[cfg(feature = "polars")]
pub use table::Column;
#[cfg(feature = "polars")]
pub use table::TableBuilder;
