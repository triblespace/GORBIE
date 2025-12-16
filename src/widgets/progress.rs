use eframe::egui::{
    self, pos2, vec2, Align2, Color32, NumExt as _, Rect, Response, Sense, Stroke, TextStyle,
    TextWrapMode, Ui, Widget, WidgetInfo, WidgetText, WidgetType,
};

use crate::themes::GorbieSliderStyle;

#[derive(Clone, Debug)]
struct ScaleLabel {
    fraction: f32,
    text: String,
}

#[derive(Clone, Copy, Debug)]
struct MeterZone {
    start: f32,
    end: f32,
    color: Color32,
}

impl MeterZone {
    fn contains(self, t: f32) -> bool {
        t >= self.start && t <= self.end
    }
}

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct ProgressBar {
    progress: f32,
    desired_width: Option<f32>,
    desired_height: Option<f32>,
    text: Option<WidgetText>,
    fill: Option<Color32>,
    segments: Option<usize>,
    scale_labels: Vec<ScaleLabel>,
    zones: Vec<MeterZone>,
}

impl ProgressBar {
    /// Progress in the `[0, 1]` range, where `1` means "completed".
    pub fn new(progress: f32) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            desired_width: None,
            desired_height: None,
            text: None,
            fill: None,
            segments: None,
            scale_labels: Vec::new(),
            zones: Vec::new(),
        }
    }

    /// The desired width of the bar. Will use all horizontal space if not set.
    pub fn desired_width(mut self, desired_width: f32) -> Self {
        self.desired_width = Some(desired_width);
        self
    }

    /// The desired height of the bar. Will use the default interaction size if not set.
    pub fn desired_height(mut self, desired_height: f32) -> Self {
        self.desired_height = Some(desired_height);
        self
    }

    /// The fill color of the bar. Defaults to `selection.bg_fill`.
    pub fn fill(mut self, fill: Color32) -> Self {
        self.fill = Some(fill);
        self
    }

    /// A custom text to display on the progress bar.
    pub fn text(mut self, text: impl Into<WidgetText>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Force a specific segment count for the meter. Defaults to an automatic count based on
    /// the available width.
    pub fn segments(mut self, segments: usize) -> Self {
        self.segments = Some(segments.max(1));
        self
    }

    /// Show a simple `0 50 100` scale.
    pub fn scale_percent(self) -> Self {
        self.scale_labels(vec![(0.0, "0"), (0.5, "50"), (1.0, "100")])
    }

    /// Add scale labels. Each `fraction` is in the `[0, 1]` range.
    pub fn scale_labels<I, S>(mut self, labels: I) -> Self
    where
        I: IntoIterator<Item = (f32, S)>,
        S: Into<String>,
    {
        self.scale_labels = labels
            .into_iter()
            .map(|(fraction, text)| ScaleLabel {
                fraction: fraction.clamp(0.0, 1.0),
                text: text.into(),
            })
            .collect();
        self
    }

    /// Override fill colors for specific ranges (by segment position along the meter).
    ///
    /// `range` is in the normalized `[0, 1]` domain, where `0` is the left edge and `1` is the
    /// right edge.
    pub fn zone(mut self, range: std::ops::RangeInclusive<f32>, color: Color32) -> Self {
        let (start, end) = (*range.start(), *range.end());
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        self.zones.push(MeterZone {
            start: start.clamp(0.0, 1.0),
            end: end.clamp(0.0, 1.0),
            color,
        });
        self
    }
}

impl Widget for ProgressBar {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            progress,
            desired_width,
            desired_height,
            text,
            fill,
            segments,
            scale_labels,
            zones,
        } = self;

        let label_text = text.as_ref().map(|text| text.text().to_string());
        let desired_width =
            desired_width.unwrap_or_else(|| ui.available_size_before_wrap().x.at_least(96.0));
        let has_scale = !scale_labels.is_empty();
        let font_id = TextStyle::Small.resolve(ui.style());
        let scale_row_height = if has_scale {
            let tick_len = 4.0;
            let tick_pad = 2.0;
            let text_height = ui.fonts(|fonts| fonts.row_height(&font_id));
            tick_len + tick_pad + text_height
        } else {
            0.0
        };
        let height = desired_height.unwrap_or(ui.spacing().interact_size.y + scale_row_height);
        let enabled = ui.is_enabled();
        let (outer_rect, response) =
            ui.allocate_exact_size(vec2(desired_width, height), Sense::hover());

        response.widget_info(move || {
            let mut info = if let Some(label_text) = label_text.as_deref() {
                WidgetInfo::labeled(WidgetType::ProgressIndicator, enabled, label_text)
            } else {
                WidgetInfo::new(WidgetType::ProgressIndicator)
            };
            info.value = Some((progress as f64 * 100.0).floor());

            info
        });

        if ui.is_rect_visible(outer_rect) {
            let (mut slot_area_rect, scale_rect) = if has_scale {
                let scale_row_height = scale_row_height.min(outer_rect.height()).at_least(0.0);
                let slot_area_rect = Rect::from_min_max(
                    outer_rect.left_top(),
                    pos2(outer_rect.right(), outer_rect.bottom() - scale_row_height),
                );
                let scale_rect = Rect::from_min_max(
                    pos2(outer_rect.left(), slot_area_rect.bottom()),
                    outer_rect.right_bottom(),
                );
                (slot_area_rect, scale_rect)
            } else {
                (outer_rect, Rect::NOTHING)
            };

            let gstyle = GorbieSliderStyle::from(ui.style().as_ref());

            let outline = gstyle.rail_fill;
            let accent = ui.visuals().selection.bg_fill;
            let stroke_color = if response.hovered() || response.has_focus() {
                accent
            } else {
                outline
            };
            let stroke = Stroke::new(1.0, stroke_color);

            let mut label = None;
            if let Some(text) = text {
                let label_max_width = slot_area_rect.width() * 0.35;
                let galley = text.into_galley(
                    ui,
                    Some(TextWrapMode::Truncate),
                    label_max_width,
                    TextStyle::Small,
                );
                let label_gap = ui.spacing().item_spacing.x;

                let label_w = galley.size().x;
                if label_w + label_gap + 32.0 < slot_area_rect.width() {
                    let label_rect = Rect::from_min_max(
                        slot_area_rect.left_top(),
                        pos2(slot_area_rect.left() + label_w, slot_area_rect.bottom()),
                    );
                    slot_area_rect.min.x =
                        (label_rect.right() + label_gap).min(slot_area_rect.max.x);
                    label = Some((label_rect, galley));
                }
            }

            let slot_margin = slot_area_rect.height().at_most(28.0) * 0.18;
            let slot_rect = slot_area_rect.shrink2(vec2(0.0, slot_margin));
            let slot_radius = 0.0;

            let painter = ui.painter();
            painter.rect_filled(slot_rect, slot_radius, gstyle.rail_bg);
            painter.rect_stroke(slot_rect, slot_radius, stroke, egui::StrokeKind::Inside);

            let fill_color = fill.unwrap_or(accent);
            let fill_inset = 2.0;
            let meter_rect = slot_rect.shrink(fill_inset);

            if meter_rect.is_positive() {
                let segment_height = meter_rect.height();
                let default_gap = (segment_height * 0.35).clamp(2.0, 12.0);
                let requested_width = (segment_height * 0.65).clamp(4.0, 12.0);

                let (segment_count, segment_gap) = if let Some(segments) = segments {
                    let segment_count = segments.max(1);
                    if segment_count <= 1 {
                        (1, 0.0)
                    } else {
                        let min_width = 1.0;
                        let max_gap = (meter_rect.width() - min_width * segment_count as f32)
                            / (segment_count as f32 - 1.0);
                        let segment_gap = default_gap.min(max_gap.max(0.0));
                        (segment_count, segment_gap)
                    }
                } else {
                    let mut segment_count = ((meter_rect.width() + default_gap)
                        / (requested_width + default_gap))
                        .floor()
                        .at_least(1.0) as usize;

                    while segment_count > 1 {
                        let total_gap = default_gap * (segment_count as f32 - 1.0);
                        if meter_rect.width() - total_gap >= segment_count as f32 {
                            break;
                        }
                        segment_count -= 1;
                    }

                    (segment_count, default_gap)
                };

                let total_gap = segment_gap * (segment_count as f32 - 1.0);
                let segment_width =
                    ((meter_rect.width() - total_gap) / segment_count as f32).at_least(1.0);

                let filled = (progress * segment_count as f32).clamp(0.0, segment_count as f32);
                let full_segments = filled.floor() as usize;
                let partial = filled - full_segments as f32;

                let off_color = crate::themes::blend(gstyle.rail_bg, outline, 0.18);
                let has_zones = !zones.is_empty();
                for i in 0..segment_count {
                    let segment_fill_color = if has_zones {
                        let t = (i as f32 + 0.5) / segment_count as f32;
                        zones
                            .iter()
                            .rev()
                            .find(|zone| zone.contains(t))
                            .map(|zone| zone.color)
                            .unwrap_or(fill_color)
                    } else {
                        fill_color
                    };
                    let x = meter_rect.left() + i as f32 * (segment_width + segment_gap);
                    let seg_rect = Rect::from_min_max(
                        pos2(x, meter_rect.top()),
                        pos2(x + segment_width, meter_rect.bottom()),
                    );

                    painter.rect_filled(seg_rect, 0.0, off_color);
                    if i < full_segments {
                        painter.rect_filled(seg_rect, 0.0, segment_fill_color);
                    } else if i == full_segments && partial > 0.0 && full_segments < segment_count {
                        let dim_color =
                            crate::themes::blend(off_color, segment_fill_color, partial);
                        painter.rect_filled(seg_rect, 0.0, dim_color);
                    }
                }
            }

            if let Some((label_rect, galley)) = label {
                let label_color = ui.visuals().weak_text_color();
                let text_pos = pos2(
                    label_rect.left(),
                    slot_rect.center().y - galley.size().y / 2.0,
                );
                painter.galley(text_pos, galley, label_color);
            }

            if has_scale && scale_rect.is_positive() && meter_rect.is_positive() {
                let tick_len = 4.0;
                let tick_pad = 2.0;
                let tick_y0 = scale_rect.top();
                let tick_y1 = (tick_y0 + tick_len).min(scale_rect.bottom());
                let label_y = (tick_y1 + tick_pad).min(scale_rect.bottom());
                let scale_color = ui.visuals().weak_text_color();
                let tick_stroke = Stroke::new(1.0, outline);

                for ScaleLabel { fraction, text } in &scale_labels {
                    let x = meter_rect.left() + meter_rect.width() * *fraction;

                    painter.line_segment([pos2(x, tick_y0), pos2(x, tick_y1)], tick_stroke);

                    let align = if *fraction <= 0.001 {
                        Align2::LEFT_TOP
                    } else if *fraction >= 0.999 {
                        Align2::RIGHT_TOP
                    } else {
                        Align2::CENTER_TOP
                    };
                    painter.text(pos2(x, label_y), align, text, font_id.clone(), scale_color);
                }
            }
        }

        response
    }
}
