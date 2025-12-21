use std::borrow::Cow;

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, Rect, Response, Sense, Stroke, TextStyle, Ui, Widget,
};

use crate::themes::GorbieHistogramStyle;

#[derive(Clone, Copy, Debug)]
pub enum HistogramYAxis {
    Count,
    Bytes,
}

#[derive(Clone, Debug)]
pub struct HistogramBucket<'a> {
    pub value: u64,
    pub label: Cow<'a, str>,
    pub tooltip: Option<Cow<'a, str>>,
}

impl<'a> HistogramBucket<'a> {
    pub fn new(value: u64, label: impl Into<Cow<'a, str>>) -> Self {
        Self {
            value,
            label: label.into(),
            tooltip: None,
        }
    }

    pub fn tooltip(mut self, tooltip: impl Into<Cow<'a, str>>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

#[derive(Clone, Copy)]
struct CountScale {
    divisor: u64,
    suffix: &'static str,
}

impl CountScale {
    fn pick(max: u64) -> Self {
        if max >= 1_000_000_000 {
            Self {
                divisor: 1_000_000_000,
                suffix: "B",
            }
        } else if max >= 1_000_000 {
            Self {
                divisor: 1_000_000,
                suffix: "M",
            }
        } else if max >= 1_000 {
            Self {
                divisor: 1_000,
                suffix: "K",
            }
        } else {
            Self {
                divisor: 1,
                suffix: "",
            }
        }
    }

    fn format(self, value: u64) -> String {
        if value == 0 {
            return "0".to_owned();
        }

        if self.divisor == 1 {
            return format!("{value}");
        }

        let scaled = value as f64 / self.divisor as f64;
        if (scaled.fract() - 0.0).abs() < f64::EPSILON {
            format!(
                "{scaled}{suffix}",
                scaled = scaled as u64,
                suffix = self.suffix
            )
        } else {
            format!("{scaled:.1}{suffix}", suffix = self.suffix)
        }
    }
}

#[derive(Clone, Copy)]
struct BytesScale {
    divisor: u64,
    suffix: &'static str,
}

impl BytesScale {
    fn pick(step: u64) -> Self {
        if step >= (1u64 << 30) {
            Self {
                divisor: 1u64 << 30,
                suffix: "GiB",
            }
        } else if step >= (1u64 << 20) {
            Self {
                divisor: 1u64 << 20,
                suffix: "MiB",
            }
        } else if step >= (1u64 << 10) {
            Self {
                divisor: 1u64 << 10,
                suffix: "KiB",
            }
        } else {
            Self {
                divisor: 1,
                suffix: "B",
            }
        }
    }

    fn format(self, value: u64) -> String {
        if self.divisor == 1 {
            return format!("{value} B");
        }

        let scaled = value / self.divisor;
        format!("{scaled} {suffix}", suffix = self.suffix)
    }
}

fn paint_hatching(painter: &egui::Painter, rect: Rect, color: Color32) {
    let spacing = 8.0;
    let stroke = Stroke::new(1.0, color);

    let h = rect.height();
    let mut x = rect.left() - h;
    while x < rect.right() + h {
        painter.line_segment([pos2(x, rect.top()), pos2(x + h, rect.bottom())], stroke);
        x += spacing;
    }
}

fn nice_decimal_step(max_value: u64, segments: u64) -> u64 {
    let segments = segments.max(1);
    let raw_step = max_value.div_ceil(segments).max(1);
    let magnitude = 10u64.pow(raw_step.ilog10());
    for mult in [1u64, 2, 5, 10] {
        let step = mult.saturating_mul(magnitude);
        if step >= raw_step {
            return step;
        }
    }
    10u64.saturating_mul(magnitude)
}

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct Histogram<'a> {
    buckets: &'a [HistogramBucket<'a>],
    y_axis: HistogramYAxis,
    desired_width: Option<f32>,
    plot_height: f32,
    y_segments: u64,
    max_x_labels: usize,
    gorbie_style: Option<GorbieHistogramStyle>,
}

impl<'a> Histogram<'a> {
    pub fn new(buckets: &'a [HistogramBucket<'a>], y_axis: HistogramYAxis) -> Self {
        Self {
            buckets,
            y_axis,
            desired_width: None,
            plot_height: 80.0,
            y_segments: 4,
            max_x_labels: 7,
            gorbie_style: None,
        }
    }

    pub fn desired_width(mut self, desired_width: f32) -> Self {
        self.desired_width = Some(desired_width);
        self
    }

    pub fn plot_height(mut self, plot_height: f32) -> Self {
        self.plot_height = plot_height.max(16.0);
        self
    }

    pub fn y_segments(mut self, segments: u64) -> Self {
        self.y_segments = segments.max(1);
        self
    }

    pub fn max_x_labels(mut self, max_labels: usize) -> Self {
        self.max_x_labels = max_labels;
        self
    }
}

impl Widget for Histogram<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let Histogram {
            buckets,
            y_axis,
            desired_width,
            plot_height,
            y_segments,
            max_x_labels,
            gorbie_style,
        } = self;

        let gstyle =
            gorbie_style.unwrap_or_else(|| GorbieHistogramStyle::from(ui.style().as_ref()));

        let desired_width = desired_width.unwrap_or_else(|| ui.available_width().max(128.0));
        let font_id = TextStyle::Small.resolve(ui.style());
        let tick_len = 4.0;
        let tick_pad = 2.0;
        let text_height = ui.fonts(|fonts| fonts.row_height(&font_id));
        let label_row_h = tick_len + tick_pad + text_height;

        let total_h = plot_height + label_row_h;
        let (outer_rect, response) =
            ui.allocate_exact_size(vec2(desired_width, total_h), Sense::hover());
        if !ui.is_rect_visible(outer_rect) {
            return response;
        }

        let outline = gstyle.outline;
        let ink = gstyle.ink;
        let stroke = Stroke::new(1.0, outline);
        let grid_color = gstyle.grid;

        let max_value = buckets.iter().map(|bucket| bucket.value).max().unwrap_or(0);

        let y_step = match y_axis {
            HistogramYAxis::Bytes => max_value.div_ceil(y_segments).max(1).next_power_of_two(),
            HistogramYAxis::Count => nice_decimal_step(max_value, y_segments),
        };
        let y_max = y_step.saturating_mul(y_segments).max(1);
        let y_ticks: Vec<u64> = (0..=y_segments).map(|i| y_step.saturating_mul(i)).collect();

        let bytes_scale = matches!(y_axis, HistogramYAxis::Bytes).then(|| BytesScale::pick(y_step));
        let count_scale = matches!(y_axis, HistogramYAxis::Count).then(|| CountScale::pick(y_max));

        let y_label_width = ui.fonts(|fonts| {
            y_ticks
                .iter()
                .map(|&value| {
                    let text = match (bytes_scale, count_scale) {
                        (Some(scale), _) => scale.format(value),
                        (_, Some(scale)) => scale.format(value),
                        _ => unreachable!(),
                    };
                    fonts.layout_no_wrap(text, font_id.clone(), ink).size().x
                })
                .fold(0.0, f32::max)
        });
        let y_axis_w = (y_label_width + 10.0).clamp(24.0, 80.0);
        let y_axis_pad = 6.0;

        let plot_rect = Rect::from_min_max(
            pos2(
                (outer_rect.left() + y_axis_w + y_axis_pad).min(outer_rect.right()),
                outer_rect.top(),
            ),
            pos2(outer_rect.right(), outer_rect.bottom() - label_row_h),
        );
        let plot_area = plot_rect.shrink(4.0);

        let painter = ui.painter().with_clip_rect(outer_rect);
        painter.rect_stroke(plot_rect, 0.0, stroke, egui::StrokeKind::Inside);

        for value in &y_ticks {
            let frac = (*value as f64 / y_max as f64) as f32;
            let y = plot_area.bottom() - frac * plot_area.height();
            painter.line_segment(
                [pos2(plot_area.left(), y), pos2(plot_area.right(), y)],
                Stroke::new(1.0, grid_color),
            );

            let text = match (bytes_scale, count_scale) {
                (Some(scale), _) => scale.format(*value),
                (_, Some(scale)) => scale.format(*value),
                _ => unreachable!(),
            };
            painter.text(
                pos2(plot_rect.left() - 4.0, y),
                Align2::RIGHT_CENTER,
                text,
                font_id.clone(),
                ink,
            );
        }

        let bucket_count = buckets.len();
        if bucket_count == 0 || !plot_area.is_positive() {
            return response;
        }

        let gap = 2.0;
        let bar_w = ((plot_area.width() - gap * (bucket_count.saturating_sub(1) as f32))
            / bucket_count as f32)
            .max(1.0);

        for (i, bucket) in buckets.iter().enumerate() {
            let value = bucket.value;
            if value == 0 {
                continue;
            }

            let frac = (value as f64 / y_max as f64) as f32;
            let bar_h = (frac * plot_area.height()).clamp(1.0, plot_area.height());

            let x0 = plot_area.left() + i as f32 * (bar_w + gap);
            let x1 = (x0 + bar_w).min(plot_area.right());
            let bar_rect = Rect::from_min_max(
                pos2(x0, plot_area.bottom() - bar_h),
                pos2(x1, plot_area.bottom()),
            );

            let id = response.id.with(("histogram_bar", i));
            let resp = ui.interact(bar_rect, id, Sense::hover());
            let stroke_color = if resp.hovered() {
                gstyle.accent
            } else {
                outline
            };
            let bar_stroke = Stroke::new(1.0, stroke_color);

            let hatch_rect = bar_rect.shrink(1.0);
            if hatch_rect.is_positive() {
                paint_hatching(&painter.with_clip_rect(hatch_rect), hatch_rect, ink);
            }
            painter.rect_stroke(bar_rect, 0.0, bar_stroke, egui::StrokeKind::Inside);

            if let Some(tooltip) = bucket.tooltip.as_deref() {
                let _ = resp.on_hover_text(tooltip);
            }
        }

        if max_x_labels > 0 {
            let step = (bucket_count.div_ceil(max_x_labels)).max(1);
            let tick_top = plot_rect.bottom();

            for i in (0..bucket_count).step_by(step) {
                let x = plot_area.left() + i as f32 * (bar_w + gap) + bar_w * 0.5;
                painter.line_segment(
                    [pos2(x, tick_top), pos2(x, tick_top + tick_len)],
                    Stroke::new(1.0, outline),
                );
                painter.text(
                    pos2(x, tick_top + tick_len + tick_pad),
                    Align2::CENTER_TOP,
                    buckets[i].label.as_ref(),
                    font_id.clone(),
                    ink,
                );
            }
        }

        response
    }
}

impl crate::themes::Styled for Histogram<'_> {
    type Style = GorbieHistogramStyle;

    fn set_style(&mut self, style: Option<Self::Style>) {
        self.gorbie_style = style;
    }
}
