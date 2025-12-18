use eframe::egui::{
    self, pos2, vec2, Color32, NumExt as _, Rect, Response, Sense, Stroke, TextStyle, Ui, Widget,
    WidgetInfo, WidgetText, WidgetType,
};

use crate::themes::{GorbieButtonStyle, GorbieChoiceToggleStyle, GorbieToggleButtonStyle};

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct Button {
    text: WidgetText,
    small: bool,
    selected: bool,
    fill: Option<Color32>,
    gorbie_style: Option<GorbieButtonStyle>,
}

impl Button {
    pub fn new(text: impl Into<WidgetText>) -> Self {
        Self {
            text: text.into(),
            small: false,
            selected: false,
            fill: None,
            gorbie_style: None,
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
            gorbie_style,
        } = self;

        let enabled = ui.is_enabled();
        let gstyle = gorbie_style.unwrap_or_else(|| GorbieButtonStyle::from(ui.style().as_ref()));
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
        let outline = gstyle.outline;
        let accent = gstyle.accent;
        let shadow_color = gstyle.shadow;

        let base_fill = fill.unwrap_or(gstyle.fill);
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

        let rounding = gstyle.rounding;
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

impl crate::themes::Styled for Button {
    type Style = GorbieButtonStyle;

    fn set_style(&mut self, style: Option<Self::Style>) {
        self.gorbie_style = style;
    }
}

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct ToggleButton<'a> {
    on: &'a mut bool,
    text: WidgetText,
    small: bool,
    fill: Option<Color32>,
    light: Option<Color32>,
    gorbie_style: Option<GorbieToggleButtonStyle>,
}

impl<'a> ToggleButton<'a> {
    pub fn new(on: &'a mut bool, text: impl Into<WidgetText>) -> Self {
        Self {
            on,
            text: text.into(),
            small: false,
            fill: None,
            light: None,
            gorbie_style: None,
        }
    }

    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn fill(mut self, fill: Color32) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn light(mut self, color: Color32) -> Self {
        self.light = Some(color);
        self
    }
}

impl Widget for ToggleButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            on,
            text,
            small,
            fill,
            light,
            gorbie_style,
        } = self;

        let enabled = ui.is_enabled();
        let gstyle =
            gorbie_style.unwrap_or_else(|| GorbieToggleButtonStyle::from(ui.style().as_ref()));
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
        let min_body_height = if self.small {
            (ui.spacing().interact_size.y - 6.0).at_least(0.0)
        } else {
            ui.spacing().interact_size.y
        };
        body_size.y = body_size.y.at_least(min_body_height);

        let desired_size = body_size + shadow_inset;
        let (outer_rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());

        if response.clicked() && enabled {
            *on = !*on;
            response.mark_changed();
        }

        response.widget_info(move || WidgetInfo::labeled(WidgetType::Button, enabled, &label_text));

        if !ui.is_rect_visible(outer_rect) {
            return response;
        }

        let visuals = ui.visuals();
        let outline = gstyle.outline;
        let accent = gstyle.accent;
        let shadow_color = gstyle.shadow;

        let base_fill = fill.unwrap_or(gstyle.fill);
        let disabled_fill = crate::themes::blend(base_fill, visuals.window_fill, 0.65);

        let is_down = enabled && response.is_pointer_button_down_on();
        let hovered = response.hovered() || response.has_focus();
        let toggled_on = *on;

        let fill = if enabled { base_fill } else { disabled_fill };
        let stroke_color = if enabled && hovered { accent } else { outline };

        let body_rect_up =
            Rect::from_min_max(outer_rect.min, outer_rect.max - shadow_inset).intersect(outer_rect);
        let body_rect = if is_down {
            body_rect_up.translate(shadow_offset)
        } else if toggled_on {
            body_rect_up.translate(shadow_offset / 2.0)
        } else {
            body_rect_up
        };

        let rounding = gstyle.rounding;
        let painter = ui.painter();

        if enabled && !is_down {
            let offset = if toggled_on {
                shadow_offset / 2.0
            } else {
                shadow_offset
            };
            painter.rect_filled(body_rect.translate(offset), rounding, shadow_color);
        }

        painter.rect_filled(body_rect, rounding, fill);
        painter.rect_stroke(
            body_rect,
            rounding,
            Stroke::new(1.0, stroke_color),
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

        let led_height = if small { 1.5 } else { 2.0 };
        let led_inset_x = 2.0;
        let led_inset_y = 2.0;
        let led_rect = Rect::from_min_max(
            pos2(
                body_rect.left() + led_inset_x,
                body_rect.top() + led_inset_y,
            ),
            pos2(
                body_rect.right() - led_inset_x,
                (body_rect.top() + led_inset_y + led_height).min(body_rect.bottom()),
            ),
        );
        if led_rect.is_positive() {
            let on_color = light.unwrap_or(gstyle.led_on);
            let off_color = crate::themes::blend(gstyle.rail_bg, fill, gstyle.led_off_towards_fill);

            let mut led_fill = if toggled_on { on_color } else { off_color };
            if !enabled {
                led_fill = crate::themes::blend(led_fill, visuals.window_fill, 0.6);
            }

            painter.rect_filled(led_rect, 1.0, led_fill);
        }

        response
    }
}

impl crate::themes::Styled for ToggleButton<'_> {
    type Style = GorbieToggleButtonStyle;

    fn set_style(&mut self, style: Option<Self::Style>) {
        self.gorbie_style = style;
    }
}

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct ChoiceToggle<'a> {
    value: &'a mut bool,
    off_text: WidgetText,
    on_text: WidgetText,
    small: bool,
    fill: Option<Color32>,
    light: Option<Color32>,
    gorbie_style: Option<GorbieChoiceToggleStyle>,
}

impl<'a> ChoiceToggle<'a> {
    /// A two-position selector that renders both options explicitly.
    ///
    /// `false` corresponds to the left/off option, and `true` corresponds to the right/on option.
    pub fn new(
        value: &'a mut bool,
        off_text: impl Into<WidgetText>,
        on_text: impl Into<WidgetText>,
    ) -> Self {
        Self {
            value,
            off_text: off_text.into(),
            on_text: on_text.into(),
            small: false,
            fill: None,
            light: None,
            gorbie_style: None,
        }
    }

    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn fill(mut self, fill: Color32) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn light(mut self, color: Color32) -> Self {
        self.light = Some(color);
        self
    }
}

impl Widget for ChoiceToggle<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            value,
            off_text,
            on_text,
            small,
            fill,
            light,
            gorbie_style,
        } = self;

        let enabled = ui.is_enabled();
        let gstyle =
            gorbie_style.unwrap_or_else(|| GorbieChoiceToggleStyle::from(ui.style().as_ref()));
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

        let off_label = off_text.text().to_string();
        let on_label = on_text.text().to_string();
        let label_text = format!("{off_label}/{on_label}");

        let max_text_width = ui.available_width().at_least(0.0);
        let off_galley = off_text.into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            max_text_width,
            text_style.clone(),
        );
        let on_galley = on_text.into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            max_text_width,
            text_style,
        );

        let mut segment_size = vec2(off_galley.size().x, off_galley.size().y)
            .max(vec2(on_galley.size().x, on_galley.size().y))
            + padding * 2.0;
        let min_body_height = if self.small {
            (ui.spacing().interact_size.y - 6.0).at_least(0.0)
        } else {
            ui.spacing().interact_size.y
        };
        segment_size.y = segment_size.y.at_least(min_body_height);

        let body_size = vec2(segment_size.x * 2.0, segment_size.y);
        let desired_size = body_size + shadow_inset;
        let (outer_rect, outer_response) = ui.allocate_exact_size(desired_size, Sense::hover());

        let mut response = outer_response;
        response.widget_info(move || {
            WidgetInfo::labeled(WidgetType::Button, enabled, label_text.as_str())
        });

        if !ui.is_rect_visible(outer_rect) {
            return response;
        }

        let visuals = ui.visuals();
        let outline = gstyle.outline;
        let accent = gstyle.accent;
        let shadow_color = gstyle.shadow;

        let base_fill = fill.unwrap_or(gstyle.fill);
        let disabled_fill = crate::themes::blend(base_fill, visuals.window_fill, 0.65);
        let disabled_slot_fill = crate::themes::blend(gstyle.rail_bg, visuals.window_fill, 0.65);

        let slot_rect =
            Rect::from_min_max(outer_rect.min, outer_rect.max - shadow_inset).intersect(outer_rect);
        let slot_fill = if enabled {
            gstyle.rail_bg
        } else {
            disabled_slot_fill
        };

        let segment_gap = gstyle.segment_gap;
        let half_gap = segment_gap * 0.5;

        let split_x = slot_rect.left() + slot_rect.width() / 2.0;
        let left_slot = Rect::from_min_max(
            slot_rect.left_top(),
            pos2(
                (split_x - half_gap).max(slot_rect.left()),
                slot_rect.bottom(),
            ),
        );
        let right_slot = Rect::from_min_max(
            pos2((split_x + half_gap).min(slot_rect.right()), slot_rect.top()),
            slot_rect.right_bottom(),
        );

        let left_id = ui.make_persistent_id((response.id, "choice-toggle-left"));
        let right_id = ui.make_persistent_id((response.id, "choice-toggle-right"));

        let left_response = ui.interact(left_slot, left_id, Sense::click());
        let right_response = ui.interact(right_slot, right_id, Sense::click());

        let pointer_pressed = enabled && ui.input(|i| i.pointer.any_pressed());

        let mut changed = false;
        if pointer_pressed && left_response.is_pointer_button_down_on() && *value {
            *value = false;
            changed = true;
        }
        if pointer_pressed && right_response.is_pointer_button_down_on() && !*value {
            *value = true;
            changed = true;
        }

        // Fallback for non-pointer activation (e.g. keyboard).
        if enabled && left_response.clicked() && *value {
            *value = false;
            changed = true;
        }
        if enabled && right_response.clicked() && !*value {
            *value = true;
            changed = true;
        }
        if changed {
            response.mark_changed();
        }

        let slot_rounding = gstyle.slot_rounding;
        let segment_rounding = gstyle.segment_rounding;
        let painter = ui.painter();

        painter.rect_filled(slot_rect, slot_rounding, slot_fill);
        painter.rect_stroke(
            slot_rect,
            slot_rounding,
            Stroke::new(1.0, outline),
            egui::StrokeKind::Inside,
        );

        let segment_margin = shadow_offset.x.max(shadow_offset.y).max(2.0);

        #[derive(Clone, Copy)]
        enum InnerEdge {
            Left,
            Right,
        }

        fn draw_segment(
            ui: &Ui,
            gstyle: &GorbieChoiceToggleStyle,
            face_up: Rect,
            rounding: egui::CornerRadius,
            mask_stroke: Option<InnerEdge>,
            galley: std::sync::Arc<egui::Galley>,
            hovered: bool,
            is_down: bool,
            is_active: bool,
            enabled: bool,
            base_fill: Color32,
            disabled_fill: Color32,
            outline: Color32,
            accent: Color32,
            shadow_color: Color32,
            shadow_offset: egui::Vec2,
            light: Option<Color32>,
            small: bool,
        ) {
            let painter = ui.painter();
            let fill = if enabled { base_fill } else { disabled_fill };
            let is_pressed = is_down || is_active;

            let pressed_offset = vec2(0.0, shadow_offset.y.max(0.0));
            let face_rect = if is_pressed {
                face_up.translate(pressed_offset)
            } else {
                face_up
            };

            if enabled && !is_pressed {
                painter.rect_filled(face_rect.translate(shadow_offset), rounding, shadow_color);
            }

            let stroke_color = if enabled && (hovered || is_down) {
                accent
            } else {
                outline
            };

            painter.rect_filled(face_rect, rounding, fill);
            painter.rect_stroke(
                face_rect,
                rounding,
                Stroke::new(1.0, stroke_color),
                egui::StrokeKind::Inside,
            );
            if let Some(mask_stroke) = mask_stroke {
                let stroke_width = 1.0;
                let mask_rect = match mask_stroke {
                    InnerEdge::Left => Rect::from_min_max(
                        pos2(face_rect.left(), face_rect.top()),
                        pos2(
                            (face_rect.left() + stroke_width).min(face_rect.right()),
                            face_rect.bottom(),
                        ),
                    ),
                    InnerEdge::Right => Rect::from_min_max(
                        pos2(
                            (face_rect.right() - stroke_width).max(face_rect.left()),
                            face_rect.top(),
                        ),
                        pos2(face_rect.right(), face_rect.bottom()),
                    ),
                };
                if mask_rect.is_positive() {
                    painter.rect_filled(mask_rect, 0, fill);
                }
            }

            let text_color = if enabled {
                crate::themes::ral(9011)
            } else {
                crate::themes::blend(crate::themes::ral(9011), fill, 0.55)
            };

            let text_pos = pos2(
                face_rect.center().x - galley.size().x / 2.0,
                face_rect.center().y - galley.size().y / 2.0,
            );
            painter.galley(text_pos, galley, text_color);

            let led_height = if small { 1.5 } else { 2.0 };
            let led_inset_x = 2.0;
            let led_inset_y = 2.0;
            let led_rect = Rect::from_min_max(
                pos2(
                    face_rect.left() + led_inset_x,
                    face_rect.top() + led_inset_y,
                ),
                pos2(
                    face_rect.right() - led_inset_x,
                    (face_rect.top() + led_inset_y + led_height).min(face_rect.bottom()),
                ),
            );
            if led_rect.is_positive() {
                let on_color = light.unwrap_or(gstyle.led_on);
                let off_color =
                    crate::themes::blend(gstyle.rail_bg, fill, gstyle.led_off_towards_fill);

                let mut led_fill = if is_active { on_color } else { off_color };
                if !enabled {
                    led_fill = crate::themes::blend(led_fill, ui.visuals().window_fill, 0.6);
                }
                painter.rect_filled(led_rect, 1, led_fill);
            }
        }

        let inner_margin = 0.0;
        let left_face_up = Rect::from_min_max(
            pos2(
                left_slot.left() + segment_margin,
                left_slot.top() + segment_margin,
            ),
            pos2(
                left_slot.right() - inner_margin,
                left_slot.bottom() - segment_margin,
            ),
        );
        let right_face_up = Rect::from_min_max(
            pos2(
                right_slot.left() + inner_margin,
                right_slot.top() + segment_margin,
            ),
            pos2(
                right_slot.right() - segment_margin,
                right_slot.bottom() - segment_margin,
            ),
        );

        let left_hovered = left_response.hovered() || left_response.has_focus();
        let right_hovered = right_response.hovered() || right_response.has_focus();
        let left_down = enabled && left_response.is_pointer_button_down_on();
        let right_down = enabled && right_response.is_pointer_button_down_on();

        let left_active = !*value;
        let right_active = *value;

        let fill = if enabled { base_fill } else { disabled_fill };
        let left_rounding = egui::CornerRadius {
            nw: segment_rounding,
            ne: 0,
            sw: segment_rounding,
            se: 0,
        };
        let right_rounding = egui::CornerRadius {
            nw: 0,
            ne: segment_rounding,
            sw: 0,
            se: segment_rounding,
        };

        // Draw segments.
        if left_active {
            draw_segment(
                ui,
                &gstyle,
                left_face_up,
                left_rounding,
                Some(InnerEdge::Right),
                off_galley,
                left_hovered,
                left_down,
                left_active,
                enabled,
                fill,
                disabled_fill,
                outline,
                accent,
                shadow_color,
                shadow_offset,
                light,
                small,
            );
            draw_segment(
                ui,
                &gstyle,
                right_face_up,
                right_rounding,
                Some(InnerEdge::Left),
                on_galley,
                right_hovered,
                right_down,
                right_active,
                enabled,
                fill,
                disabled_fill,
                outline,
                accent,
                shadow_color,
                shadow_offset,
                light,
                small,
            );
        } else {
            draw_segment(
                ui,
                &gstyle,
                right_face_up,
                right_rounding,
                Some(InnerEdge::Left),
                on_galley,
                right_hovered,
                right_down,
                right_active,
                enabled,
                fill,
                disabled_fill,
                outline,
                accent,
                shadow_color,
                shadow_offset,
                light,
                small,
            );
            draw_segment(
                ui,
                &gstyle,
                left_face_up,
                left_rounding,
                Some(InnerEdge::Right),
                off_galley,
                left_hovered,
                left_down,
                left_active,
                enabled,
                fill,
                disabled_fill,
                outline,
                accent,
                shadow_color,
                shadow_offset,
                light,
                small,
            );
        }

        response = response | left_response | right_response;
        response
    }
}

impl crate::themes::Styled for ChoiceToggle<'_> {
    type Style = GorbieChoiceToggleStyle;

    fn set_style(&mut self, style: Option<Self::Style>) {
        self.gorbie_style = style;
    }
}
