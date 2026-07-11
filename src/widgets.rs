//! Widgets module — a thin glossary re-exporting individual widget submodules.
//!
//! The actual widget implementations live in `src/widgets/*.rs` so each widget can
//! be edited independently.

/// Scalar-vs-budget gauge composed from the LCD readout and progress meter.
pub mod budget_gauge;
/// Toggle buttons, radio buttons, and choice toggles.
pub mod button;
/// Polars dataframe display widgets.
#[cfg(feature = "polars")]
pub mod dataframe;
/// Newest-first scrolling event feed with category pills.
pub mod event_feed;
/// Number and text input fields with LCD styling.
pub mod field;
/// Bar chart / histogram widget.
pub mod histogram;
/// Row-height labels for grid alignment.
pub mod label;
/// Background-loading toggle buttons and auto-loaders.
pub mod load;
/// Markdown rendering via gorbie-commonmark.
#[cfg(feature = "markdown")]
pub mod markdown;
/// Rolling time-series strip with readout and percentile band.
#[cfg(feature = "plots")]
pub mod metric_strip;
/// Determinate progress bars.
pub mod progress;
/// Horizontal and vertical sliders.
pub mod slider;
/// Scrolling stream of tagged text runs.
pub mod stream_lane;
/// Polars table display widget.
#[cfg(feature = "polars")]
pub mod table;
#[cfg(feature = "polars")]
mod table_layout;
#[cfg(feature = "polars")]
mod table_sizing;
/// TribleSpace browser widgets (pile repo, inspectors).
#[cfg(feature = "triblespace")]
pub mod triblespace;
/// Typst vector rendering engine (outline, painter, world).
#[cfg(feature = "typst")]
pub mod typst_render;
/// Typst compilation and selection widget.
#[cfg(feature = "typst")]
pub mod typst_widget;

pub use budget_gauge::BudgetGauge;
pub use button::Button;
pub use button::ChoiceToggle;
pub use button::RadioButton;
#[cfg(feature = "polars")]
pub use dataframe::{data_export_tiny, data_summary_tiny, dataframe, dataframe_summary};
pub use event_feed::EventFeed;
pub use field::lcd_readout;
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
#[cfg(feature = "plots")]
pub use metric_strip::MetricStrip;
pub use progress::ProgressBar;
pub use slider::Slider;
pub use slider::SliderClamping;
pub use stream_lane::RunStyle;
pub use stream_lane::StreamLane;
#[cfg(feature = "polars")]
pub use table::Column;
#[cfg(feature = "polars")]
pub use table::TableBuilder;
