use eframe::egui::{
    self, pos2, vec2, Color32, Key, NumExt as _, Rect, Response, Sense, Stroke, TextStyle, Ui,
    Widget, WidgetInfo, WidgetText, WidgetType,
};

use crate::themes::{GorbieButtonStyle, GorbieChoiceToggleStyle, GorbieToggleButtonStyle};

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct Button {
    text: WidgetText,
    small: bool,
    selected: bool,
    fill: Option<Color32>,
    light: Option<Color32>,
    latched: bool,
    latch_on_click: bool,
    gorbie_style: Option<GorbieButtonStyle>,
}

impl Button {
    pub fn new(text: impl Into<WidgetText>) -> Self {
        Self {
            text: text.into(),
            small: false,
            selected: false,
            fill: None,
            light: None,
            latched: false,
            latch_on_click: false,
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

    pub fn light(mut self, color: Color32) -> Self {
        self.light = Some(color);
        self
    }

    pub fn latched(mut self, latched: bool) -> Self {
        self.latched = latched;
        self
    }

    pub fn latch_on_click(mut self, latch_on_click: bool) -> Self {
        self.latch_on_click = latch_on_click;
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
            light,
            latched,
            latch_on_click,
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

        let mut latched = latched;
        if latch_on_click && response.clicked() {
            latched = true;
        }
        let keyboard_down = response.has_focus()
            && ui.input(|input| input.key_down(Key::Space) || input.key_down(Key::Enter));
        let is_down = enabled && (response.is_pointer_button_down_on() || keyboard_down || latched);
        let prepress = enabled && !is_down && (response.hovered() || response.has_focus());

        let fill = if enabled { base_fill } else { disabled_fill };
        let stroke_color = if enabled && selected { accent } else { outline };
        let stroke_width = 1.0;

        let mut body_rect =
            Rect::from_min_max(outer_rect.min, outer_rect.max - shadow_inset).intersect(outer_rect);
        let press_offset = if is_down {
            shadow_offset
        } else if prepress {
            shadow_offset * 0.5
        } else {
            vec2(0.0, 0.0)
        };
        if press_offset != vec2(0.0, 0.0) {
            body_rect = body_rect.translate(press_offset);
        }

        let rounding = gstyle.rounding;
        let painter = ui.painter();

        if enabled && !is_down {
            let shadow_offset = if prepress {
                shadow_offset * 0.5
            } else {
                shadow_offset
            };
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

        if let Some(light) = light {
            let led_height = if small { 3.0 } else { 4.0 };
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
                let mut led_fill = light;
                if !enabled {
                    led_fill = crate::themes::blend(led_fill, visuals.window_fill, 0.6);
                }
                painter.rect_filled(led_rect, 1.0, led_fill);
            }
        }

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
        let shadow_color = gstyle.shadow;

        let base_fill = fill.unwrap_or(gstyle.fill);
        let disabled_fill = crate::themes::blend(base_fill, visuals.window_fill, 0.65);

        let keyboard_down = response.has_focus()
            && ui.input(|input| input.key_down(Key::Space) || input.key_down(Key::Enter));
        let is_down = enabled && (response.is_pointer_button_down_on() || keyboard_down);
        let toggled_on = *on;
        let prepress =
            enabled && !is_down && !toggled_on && (response.hovered() || response.has_focus());

        let fill = if enabled { base_fill } else { disabled_fill };
        let stroke_color = outline;

        let body_rect_up =
            Rect::from_min_max(outer_rect.min, outer_rect.max - shadow_inset).intersect(outer_rect);
        let body_rect = if is_down {
            body_rect_up.translate(shadow_offset)
        } else if toggled_on || prepress {
            body_rect_up.translate(shadow_offset / 2.0)
        } else {
            body_rect_up
        };

        let rounding = gstyle.rounding;
        let painter = ui.painter();

        if enabled && !is_down {
            let offset = if toggled_on || prepress {
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

        let led_height = if small { 3.0 } else { 4.0 };
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

struct ChoiceToggleOption<T> {
    value: T,
    text: WidgetText,
}

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct ChoiceToggle<'a, T> {
    value: &'a mut T,
    options: Vec<ChoiceToggleOption<T>>,
    small: bool,
    fill: Option<Color32>,
    light: Option<Color32>,
    gorbie_style: Option<GorbieChoiceToggleStyle>,
}

impl<'a, T> ChoiceToggle<'a, T> {
    pub fn new(value: &'a mut T) -> Self {
        Self {
            value,
            options: Vec::new(),
            small: false,
            fill: None,
            light: None,
            gorbie_style: None,
        }
    }

    pub fn choice(mut self, value: T, text: impl Into<WidgetText>) -> Self {
        self.options.push(ChoiceToggleOption {
            value,
            text: text.into(),
        });
        self
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

impl<'a> ChoiceToggle<'a, bool> {
    /// A two-position selector that renders both options explicitly.
    ///
    /// `false` corresponds to the left/off option, and `true` corresponds to the right/on option.
    pub fn binary(
        value: &'a mut bool,
        off_text: impl Into<WidgetText>,
        on_text: impl Into<WidgetText>,
    ) -> Self {
        ChoiceToggle::new(value)
            .choice(false, off_text)
            .choice(true, on_text)
    }
}

impl<T> Widget for ChoiceToggle<'_, T>
where
    T: Clone + PartialEq,
{
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            value,
            options,
            small,
            fill,
            light,
            gorbie_style,
        } = self;

        if options.is_empty() {
            let (_rect, response) = ui.allocate_exact_size(vec2(0.0, 0.0), Sense::hover());
            return response;
        }

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

        let wrap_mode = Some(egui::TextWrapMode::Truncate);
        let max_text_width = ui.available_width().at_least(0.0);

        struct RenderedChoice<T> {
            value: T,
            galley: std::sync::Arc<egui::Galley>,
        }

        let mut label_text = String::new();
        let mut choices: Vec<RenderedChoice<T>> = Vec::with_capacity(options.len());

        for (idx, option) in options.into_iter().enumerate() {
            if idx > 0 {
                label_text.push('/');
            }
            label_text.push_str(option.text.text().as_ref());

            let galley = option
                .text
                .into_galley(ui, wrap_mode, max_text_width, text_style.clone());
            choices.push(RenderedChoice {
                value: option.value,
                galley,
            });
        }

        let segment_count = choices.len();
        let segment_gap = gstyle.segment_gap;

        let mut segment_size = vec2(0.0, 0.0);
        for choice in &choices {
            segment_size = segment_size.max(choice.galley.size());
        }
        segment_size += padding * 2.0;

        let min_body_height = if small {
            (ui.spacing().interact_size.y - 6.0).at_least(0.0)
        } else {
            ui.spacing().interact_size.y
        };
        segment_size.y = segment_size.y.at_least(min_body_height);

        let body_width = segment_size.x * segment_count as f32
            + segment_gap * (segment_count.saturating_sub(1) as f32);
        let body_size = vec2(body_width, segment_size.y);
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

        let mut segment_slots = Vec::with_capacity(segment_count);
        let mut segment_responses = Vec::with_capacity(segment_count);

        for idx in 0..segment_count {
            let x0 = slot_rect.left() + idx as f32 * (segment_size.x + segment_gap);
            let x1 = (x0 + segment_size.x).min(slot_rect.right());

            let segment_slot =
                Rect::from_min_max(pos2(x0, slot_rect.top()), pos2(x1, slot_rect.bottom()));
            segment_slots.push(segment_slot);

            let id = ui.make_persistent_id((response.id, "choice-toggle", idx));
            segment_responses.push(ui.interact(segment_slot, id, Sense::click()));
        }

        let pointer_pressed = enabled && ui.input(|i| i.pointer.any_pressed());
        let keyboard_down =
            enabled && ui.input(|input| input.key_down(Key::Space) || input.key_down(Key::Enter));

        let mut changed = false;
        if pointer_pressed {
            for (idx, segment_response) in segment_responses.iter().enumerate() {
                if segment_response.is_pointer_button_down_on() {
                    let next_value = &choices[idx].value;
                    if next_value != &*value {
                        *value = next_value.clone();
                        changed = true;
                    }
                    break;
                }
            }
        }

        // Fallback for non-pointer activation (e.g. keyboard).
        if enabled && !changed {
            for (idx, segment_response) in segment_responses.iter().enumerate() {
                if segment_response.clicked() {
                    let next_value = &choices[idx].value;
                    if next_value != &*value {
                        *value = next_value.clone();
                        changed = true;
                    }
                    break;
                }
            }
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
        struct MaskStroke {
            left: bool,
            right: bool,
        }

        let draw_segment = |face_up: Rect,
                            rounding: egui::CornerRadius,
                            mask_stroke: MaskStroke,
                            galley: std::sync::Arc<egui::Galley>,
                            hovered: bool,
                            is_down: bool,
                            is_active: bool| {
            let painter = ui.painter();
            let fill = if enabled { base_fill } else { disabled_fill };
            let is_pressed = is_down || is_active;
            let prepress = enabled && !is_pressed && hovered;

            let pressed_offset = vec2(0.0, shadow_offset.y.max(0.0));
            let face_rect = if is_pressed {
                face_up.translate(pressed_offset)
            } else if prepress {
                face_up.translate(pressed_offset * 0.5)
            } else {
                face_up
            };

            if enabled && !is_pressed {
                let shadow_offset = if prepress {
                    pressed_offset * 0.5
                } else {
                    shadow_offset
                };
                painter.rect_filled(face_rect.translate(shadow_offset), rounding, shadow_color);
            }

            let stroke_color = if enabled && is_active {
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
            let stroke_width = 1.0;

            if mask_stroke.left {
                let mask_rect = Rect::from_min_max(
                    pos2(face_rect.left(), face_rect.top()),
                    pos2(
                        (face_rect.left() + stroke_width).min(face_rect.right()),
                        face_rect.bottom(),
                    ),
                );
                if mask_rect.is_positive() {
                    painter.rect_filled(mask_rect, 0, fill);
                }
            }
            if mask_stroke.right {
                let mask_rect = Rect::from_min_max(
                    pos2(
                        (face_rect.right() - stroke_width).max(face_rect.left()),
                        face_rect.top(),
                    ),
                    pos2(face_rect.right(), face_rect.bottom()),
                );
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

            let led_height = if small { 3.0 } else { 4.0 };
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
        };

        let active_index = choices
            .iter()
            .position(|choice| choice.value == *value)
            .unwrap_or(0);

        let mut draw_order: Vec<usize> = if shadow_offset.x >= 0.0 {
            (0..segment_count).collect()
        } else {
            (0..segment_count).rev().collect()
        };
        if let Some(pos) = draw_order.iter().position(|&idx| idx == active_index) {
            let active = draw_order.remove(pos);
            draw_order.push(active);
        }

        for idx in draw_order {
            let slot = segment_slots[idx];
            let left_inset = if idx == 0 { segment_margin } else { 0.0 };
            let right_inset = if idx + 1 == segment_count {
                segment_margin
            } else {
                0.0
            };

            let face_up = Rect::from_min_max(
                pos2(slot.left() + left_inset, slot.top() + segment_margin),
                pos2(
                    (slot.right() - right_inset).max(slot.left() + left_inset),
                    (slot.bottom() - segment_margin).max(slot.top() + segment_margin),
                ),
            );

            let rounding = egui::CornerRadius {
                nw: if idx == 0 { segment_rounding } else { 0 },
                ne: if idx + 1 == segment_count {
                    segment_rounding
                } else {
                    0
                },
                sw: if idx == 0 { segment_rounding } else { 0 },
                se: if idx + 1 == segment_count {
                    segment_rounding
                } else {
                    0
                },
            };

            let hovered = segment_responses[idx].hovered() || segment_responses[idx].has_focus();
            let is_down = enabled
                && (segment_responses[idx].is_pointer_button_down_on()
                    || (segment_responses[idx].has_focus() && keyboard_down));
            let is_active = idx == active_index;

            let mask_stroke = MaskStroke {
                left: idx > 0,
                right: idx + 1 < segment_count,
            };

            draw_segment(
                face_up,
                rounding,
                mask_stroke,
                choices[idx].galley.clone(),
                hovered,
                is_down,
                is_active,
            );
        }

        for segment_response in segment_responses {
            response |= segment_response;
        }

        if changed {
            response.mark_changed();
        }

        response
    }
}

impl<T> crate::themes::Styled for ChoiceToggle<'_, T> {
    type Style = GorbieChoiceToggleStyle;

    fn set_style(&mut self, style: Option<Self::Style>) {
        self.gorbie_style = style;
    }
}
