use eframe::egui::{
    self, pos2, vec2, Color32, NumExt as _, Rect, Response, Sense, Stroke, TextStyle, Ui, Widget,
    WidgetInfo, WidgetText, WidgetType,
};

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
        let shadow_offset = vec2(2.0, 2.0);
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
        let ink = visuals.widgets.inactive.fg_stroke.color;
        let outline = visuals.widgets.noninteractive.bg_stroke.color;
        let accent = visuals.selection.stroke.color;
        let shadow_color = crate::themes::ral(9004);

        let base_fill = fill.unwrap_or(visuals.window_fill);
        let hover_fill = crate::themes::blend(base_fill, ink, 0.05);
        let active_fill = crate::themes::blend(hover_fill, crate::themes::ral(9011), 0.12);

        let is_down = enabled && response.is_pointer_button_down_on();
        let hovered = response.hovered() || response.has_focus();

        let (fill, stroke_color, stroke_width) = if !enabled {
            (base_fill, outline, 1.0)
        } else if selected {
            (visuals.selection.bg_fill, accent, 1.4)
        } else if is_down {
            (active_fill, accent, 1.4)
        } else if hovered {
            (hover_fill, accent, 1.4)
        } else {
            (base_fill, outline, 1.0)
        };

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
            visuals.text_color()
        } else {
            visuals.weak_text_color()
        };
        let text_pos = pos2(
            body_rect.center().x - galley.size().x / 2.0,
            body_rect.center().y - galley.size().y / 2.0,
        );
        painter.galley(text_pos, galley, text_color);

        response
    }
}
