use eframe::egui::{Response, Ui, Widget};

use crate::themes;
use crate::widgets::field::lcd_readout;
use crate::widgets::ProgressBar;

/// A single scalar measured against a budget (e.g. frame time vs a
/// deadline), composed from the LCD readout and [`ProgressBar`] meter.
///
/// The layout is `[ LCD current value ][ segmented meter ]`. The meter
/// scale runs from 0 to [`scale_max`] (default 1.5× the budget) with a
/// labeled tick at the budget mark.
///
/// The over/under state is expressed **without red/green semantics**,
/// through two redundant colorblind-safe channels:
/// - luminance/position: lit segments end left of the budget tick when
///   under, and extend past it when over;
/// - the RAL 2005 accent ([`themes::button_light_on`]): segments past
///   the tick light up in the accent, and the LCD readout ink flips to
///   the same accent while over budget.
///
/// ```ignore
/// ui.add(
///     BudgetGauge::new(frame_ms, 80.0)
///         .label("frame")
///         .suffix(" ms"),
/// );
/// ```
///
/// [`scale_max`]: Self::scale_max
#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct BudgetGauge {
    value: f64,
    budget: f64,
    scale_max: Option<f64>,
    label: Option<String>,
    suffix: String,
    precision: usize,
}

impl BudgetGauge {
    /// A gauge showing `value` against `budget` (both in the same unit).
    pub fn new(value: f64, budget: f64) -> Self {
        Self {
            value,
            budget,
            scale_max: None,
            label: None,
            suffix: String::new(),
            precision: 1,
        }
    }

    /// Label shown on the meter (e.g. the metric name).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Unit suffix appended to the readout and scale labels (e.g. `" ms"`).
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    /// Decimal places in the readout and scale labels. Default is 1.
    pub fn precision(mut self, precision: usize) -> Self {
        self.precision = precision;
        self
    }

    /// Full-scale value of the meter. Defaults to 1.5× the budget.
    /// Values beyond the scale peg the meter at full; the readout
    /// always shows the true value.
    pub fn scale_max(mut self, scale_max: f64) -> Self {
        self.scale_max = Some(scale_max);
        self
    }
}

impl Widget for BudgetGauge {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            value,
            budget,
            scale_max,
            label,
            suffix,
            precision,
        } = self;

        let scale_max = scale_max.unwrap_or(budget * 1.5).max(f64::EPSILON);
        let fraction = (value / scale_max).clamp(0.0, 1.0) as f32;
        let budget_fraction = (budget / scale_max).clamp(0.0, 1.0) as f32;
        let over = value > budget;
        let accent = themes::button_light_on();

        ui.horizontal(|ui| {
            let readout = format!("{value:.precision$}{suffix}");
            lcd_readout(ui, &readout, over.then_some(accent));

            let mut bar = ProgressBar::new(fraction)
                .zone(budget_fraction..=1.0, accent)
                .scale_labels(vec![
                    (0.0, format!("0{suffix}")),
                    (budget_fraction, format!("{budget:.precision$}{suffix}")),
                    (1.0, format!("{scale_max:.precision$}{suffix}")),
                ]);
            if let Some(label) = label {
                bar = bar.text(label);
            }
            ui.add(bar)
        })
        .inner
    }
}
