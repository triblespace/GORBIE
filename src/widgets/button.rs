use eframe::egui::{
    self, pos2, vec2, Color32, NumExt as _, Rect, Response, Sense, Stroke, TextStyle, Ui, Widget,
    WidgetInfo, WidgetText, WidgetType,
};

use crate::themes::GorbieSliderStyle;

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct Button {
    text: WidgetText,
    small: bool,
    selected: bool,
    fill: Option<Color32>,
}

impl Button {
    pub fn new(text: impl Into<WidgetText>) -> Self {
        Self {
            text: text.into(),
            small: false,
            selected: false,
            fill: None,
        }
    }

    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn fill(mut self, fill: Color32) -> Self {
        self.fill = Some(fill);
        self
    }
}

impl Widget for Button {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            text,
            small,
            selected,
            fill,
        } = self;

        let enabled = ui.is_enabled();
        let gstyle = GorbieSliderStyle::from(ui.style().as_ref());
        let shadow_offset = gstyle.shadow_offset;
        let shadow_inset = vec2(shadow_offset.x.max(0.0), shadow_offset.y.max(0.0));

        let padding = if small {
            ui.spacing().button_padding * 0.7
        } else {
            ui.spacing().button_padding
        };
        let text_style = if small {
            TextStyle::Small
        } else {
            TextStyle::Button
        };

        let label_text = text.text().to_string();
        let max_text_width =
            (ui.available_width() - padding.x * 2.0 - shadow_inset.x).at_least(0.0);
        let galley = text.into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            max_text_width,
            text_style,
        );

        let mut body_size = galley.size() + padding * 2.0;
        let min_body_height = if small {
            (ui.spacing().interact_size.y - 6.0).at_least(0.0)
        } else {
            ui.spacing().interact_size.y
        };
        body_size.y = body_size.y.at_least(min_body_height);
        let desired_size = body_size + shadow_inset;

        let (outer_rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        response.widget_info(move || {
            WidgetInfo::labeled(WidgetType::Button, enabled, label_text.as_str())
        });

        if !ui.is_rect_visible(outer_rect) {
            return response;
        }

        let visuals = ui.visuals();
        let outline = gstyle.rail_fill;
        let accent = visuals.selection.stroke.color;
        let shadow_color = gstyle.shadow;

        let base_fill = fill.unwrap_or(gstyle.knob);
        let disabled_fill = crate::themes::blend(base_fill, visuals.window_fill, 0.65);

        let is_down = enabled && response.is_pointer_button_down_on();
        let hovered = response.hovered() || response.has_focus();

        let fill = if enabled { base_fill } else { disabled_fill };
        let stroke_color = if enabled && (selected || hovered || is_down) {
            accent
        } else {
            outline
        };
        let stroke_width = 1.0;

        let mut body_rect =
            Rect::from_min_max(outer_rect.min, outer_rect.max - shadow_inset).intersect(outer_rect);
        if is_down {
            body_rect = body_rect.translate(shadow_offset);
        }

        let rounding = 2.0;
        let painter = ui.painter();

        if enabled && !is_down {
            let shadow_rect = body_rect.translate(shadow_offset);
            painter.rect_filled(shadow_rect, rounding, shadow_color);
        }

        painter.rect_filled(body_rect, rounding, fill);
        painter.rect_stroke(
            body_rect,
            rounding,
            Stroke::new(stroke_width, stroke_color),
            egui::StrokeKind::Inside,
        );

        let text_color = if enabled {
            crate::themes::ral(9011)
        } else {
            crate::themes::blend(crate::themes::ral(9011), fill, 0.55)
        };
        let text_pos = pos2(
            body_rect.center().x - galley.size().x / 2.0,
            body_rect.center().y - galley.size().y / 2.0,
        );
        painter.galley(text_pos, galley, text_color);

        response
    }
}
