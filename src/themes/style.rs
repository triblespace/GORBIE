use egui::Style;

/// Traits and helpers for widget-level styles derived from our theme.

/// Provide a per-widget override API.
pub trait Styled {
    type Style: Clone;
    fn styled(self, style: Self::Style) -> Self;
}

/// Construct a widget style from the global application style.
pub trait FromTheme {
    fn from_theme(style: &Style) -> Self;
}

// The FromTheme implementation for `GorbieSliderStyle` has been moved into
// the widgets module (`src/widgets/slider.rs`) so that widget-specific
// derivation logic stays close to the widget and decouples the theme.
