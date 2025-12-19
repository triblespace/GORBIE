//! Traits and helpers for widget-level styles derived from our theme.

/// Provide a per-widget override API.
pub trait Styled {
    type Style;

    /// Apply style in-place (mutating). Implementors should update their
    /// internal style override/state accordingly.
    fn set_style(&mut self, style: Option<Self::Style>);

    /// Consuming builder convenience; default implementation delegates to
    /// `set_style` so implementors only need to implement `set_style`.
    #[must_use]
    fn styled<S: Into<Self::Style>>(self, style: S) -> Self
    where
        Self: Sized,
    {
        let mut me = self;
        me.set_style(Some(style.into()));
        me
    }

    /// Remove the current style.
    #[must_use]
    fn unstyled(self) -> Self
    where
        Self: Sized,
    {
        let mut me = self;
        me.set_style(None);
        me
    }
}

// NOTE: `FromTheme` was removed in favor of implementing `From<&egui::Style>`
// for widget styles so callers can use the standard `Into`/`From` conversions.
