use std::collections::VecDeque;

use eframe::egui::{Response, Stroke, Ui};
use egui_plot::{Axis, Line, Plot, PlotPoints, Span};

use crate::themes;
use crate::widgets::field::lcd_readout;

/// A rolling time-series strip: a bounded window of samples plotted as
/// a line, with an inline LCD readout of the current value and an
/// optional p50/p95 percentile band.
///
/// Feed it `(t, value)` samples over time with [`push`]; samples
/// beyond the window are dropped from the front. The percentile band
/// (enabled via [`percentile_band`]) shades the p50..=p95 range of the
/// current window and outlines it with the RAL 2005 accent — the
/// overlay reads through luminance and the orange accent only, no
/// red/green semantics.
///
/// ```ignore
/// // In notebook state:
/// let mut strip = MetricStrip::new("frame time", 600).suffix(" ms").percentile_band(true);
/// strip.push(t, frame_ms);
/// // In the card closure:
/// strip.show(ui);
/// ```
///
/// [`push`]: Self::push
/// [`percentile_band`]: Self::percentile_band
pub struct MetricStrip {
    name: String,
    samples: VecDeque<[f64; 2]>,
    window: usize,
    height: f32,
    suffix: String,
    precision: usize,
    band: bool,
}

impl MetricStrip {
    /// An empty strip named `name` (also the plot's identity — keep it
    /// unique within a card) retaining at most `window` samples.
    pub fn new(name: impl Into<String>, window: usize) -> Self {
        Self {
            name: name.into(),
            samples: VecDeque::new(),
            window: window.max(2),
            height: 96.0,
            suffix: String::new(),
            precision: 1,
            band: false,
        }
    }

    /// Unit suffix appended to the readout (e.g. `" ms"`).
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    /// Decimal places in the readout. Default is 1.
    pub fn precision(mut self, precision: usize) -> Self {
        self.precision = precision;
        self
    }

    /// Plot height in pixels. Default is 96.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height.max(24.0);
        self
    }

    /// Overlay a shaded p50..=p95 band computed over the current window.
    pub fn percentile_band(mut self, band: bool) -> Self {
        self.band = band;
        self
    }

    /// Append a sample; `t` should be monotonically increasing (e.g.
    /// seconds since start). Drops the oldest sample beyond the window.
    pub fn push(&mut self, t: f64, value: f64) {
        self.samples.push_back([t, value]);
        while self.samples.len() > self.window {
            self.samples.pop_front();
        }
    }

    /// The most recent value, if any.
    pub fn latest(&self) -> Option<f64> {
        self.samples.back().map(|sample| sample[1])
    }

    /// Number of samples currently retained.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// True when no samples are retained.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Drop all samples.
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Nearest-rank percentile (`q` in `0.0..=1.0`) over the current window.
    pub fn percentile(&self, q: f64) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let mut values: Vec<f64> = self.samples.iter().map(|sample| sample[1]).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let rank = ((q.clamp(0.0, 1.0) * values.len() as f64).ceil() as usize)
            .clamp(1, values.len());
        Some(values[rank - 1])
    }

    /// Render the strip into `ui`: LCD readout on the left, plot on the right.
    pub fn show(&mut self, ui: &mut Ui) -> Response {
        let precision = self.precision;
        let readout = self
            .latest()
            .map(|value| format!("{value:.precision$}{suffix}", suffix = self.suffix))
            .unwrap_or_else(|| format!("--{suffix}", suffix = self.suffix));

        let ink = ui.visuals().widgets.noninteractive.fg_stroke.color;
        let background = ui.visuals().window_fill;
        let band_fill = themes::blend(background, ink, 0.15).gamma_multiply(0.6);
        let accent = themes::button_light_on();

        let band = self
            .band
            .then(|| self.percentile(0.5).zip(self.percentile(0.95)))
            .flatten();

        ui.horizontal(|ui| {
            lcd_readout(ui, &readout, None);

            let points: PlotPoints =
                self.samples.iter().copied().collect::<Vec<[f64; 2]>>().into();
            Plot::new(("gorbie_metric_strip", self.name.as_str()))
                .height(self.height)
                .show_axes([false, true])
                .allow_drag(false)
                .allow_zoom(false)
                .allow_scroll(false)
                .allow_boxed_zoom(false)
                .allow_double_click_reset(false)
                .show(ui, |plot_ui| {
                    if let Some((p50, p95)) = band {
                        plot_ui.add(
                            Span::new("p50..p95", p50..=p95)
                                .axis(Axis::Y)
                                .fill(band_fill)
                                .border(Stroke::new(1.0, accent)),
                        );
                    }
                    plot_ui.line(
                        Line::new(self.name.clone(), points).color(ink).width(1.5),
                    );
                })
                .response
        })
        .inner
    }
}
