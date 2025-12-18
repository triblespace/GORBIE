use eframe::egui::{self, pos2, Color32, DragValue, Rect, Response, Stroke, TextEdit, Ui, Widget};

use crate::themes::{GorbieNumberFieldStyle, GorbieTextFieldStyle};

fn paint_scanline(painter: &egui::Painter, rect: Rect, color: Color32, height: f32) {
    let inset = 2.0;
    let available_h = (rect.height() - inset * 2.0).max(0.0);
    let height = height.min(available_h);
    if height <= 0.0 {
        return;
    }

    let y1 = rect.bottom() - inset;
    let y0 = y1 - height;
    let scan_rect = Rect::from_min_max(pos2(rect.left() + inset, y0), pos2(rect.right() - inset, y1));
    if scan_rect.is_positive() {
        painter.rect_filled(scan_rect, 0.0, color);
    }
}

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct NumberField<'a> {
    inner: DragValue<'a>,
    gorbie_style: Option<GorbieNumberFieldStyle>,
}

impl<'a> NumberField<'a> {
    pub fn new(inner: DragValue<'a>) -> Self {
        Self {
            inner,
            gorbie_style: None,
        }
    }

    pub fn value<Num: egui::emath::Numeric>(value: &'a mut Num) -> Self {
        Self::new(DragValue::new(value))
    }
}

impl Widget for NumberField<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            inner,
            gorbie_style,
        } = self;

        let enabled = ui.is_enabled();
        let gstyle =
            gorbie_style.unwrap_or_else(|| GorbieNumberFieldStyle::from(ui.style().as_ref()));
        let dark_mode = ui.visuals().dark_mode;

        let outline = gstyle.outline;
        let fill = if enabled {
            gstyle.fill
        } else {
            crate::themes::blend(gstyle.fill, ui.visuals().window_fill, 0.65)
        };
        let base_text_color = if dark_mode {
            crate::themes::ral(6027)
        } else {
            crate::themes::ral(9011)
        };
        let text_color = if enabled {
            base_text_color
        } else {
            crate::themes::blend(base_text_color, fill, 0.55)
        };

        let response = ui
            .scope(|ui| {
                ui.style_mut().drag_value_text_style = egui::TextStyle::Name("LCD".into());

                let visuals = ui.visuals_mut();
                visuals.override_text_color = Some(text_color);
                visuals.text_edit_bg_color = Some(fill);
                visuals.extreme_bg_color = fill;

                let widgets = &mut visuals.widgets;
                let outline_stroke = Stroke::new(1.0, outline);
                visuals.selection.stroke = outline_stroke;

                widgets.inactive.bg_stroke = outline_stroke;
                widgets.inactive.bg_fill = fill;
                widgets.inactive.weak_bg_fill = fill;
                widgets.inactive.corner_radius = gstyle.rounding.into();

                widgets.hovered.bg_stroke = outline_stroke;
                widgets.hovered.bg_fill = fill;
                widgets.hovered.weak_bg_fill = fill;
                widgets.hovered.corner_radius = gstyle.rounding.into();

                widgets.active.bg_stroke = outline_stroke;
                widgets.active.bg_fill = fill;
                widgets.active.weak_bg_fill = fill;
                widgets.active.corner_radius = gstyle.rounding.into();

                widgets.open.bg_stroke = outline_stroke;
                widgets.open.bg_fill = fill;
                widgets.open.weak_bg_fill = fill;
                widgets.open.corner_radius = gstyle.rounding.into();

                let response = ui.add(inner);

                if ui.is_rect_visible(response.rect) {
                    let painter = ui.painter();

                    if enabled && response.has_focus() {
                        paint_scanline(painter, response.rect, base_text_color, gstyle.scanline_height);
                    }
                }

                response
            })
            .inner;

        response
    }
}

impl crate::themes::Styled for NumberField<'_> {
    type Style = GorbieNumberFieldStyle;

    fn set_style(&mut self, style: Option<Self::Style>) {
        self.gorbie_style = style;
    }
}

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct TextField<'a> {
    inner: TextEdit<'a>,
    gorbie_style: Option<GorbieTextFieldStyle>,
}

impl<'a> TextField<'a> {
    pub fn new(inner: TextEdit<'a>) -> Self {
        Self {
            inner,
            gorbie_style: None,
        }
    }

    pub fn singleline(text: &'a mut dyn egui::TextBuffer) -> Self {
        Self::new(TextEdit::singleline(text))
    }

    pub fn multiline(text: &'a mut dyn egui::TextBuffer) -> Self {
        Self::new(TextEdit::multiline(text))
    }
}

impl Widget for TextField<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            inner,
            gorbie_style,
        } = self;

        let enabled = ui.is_enabled();
        let gstyle =
            gorbie_style.unwrap_or_else(|| GorbieTextFieldStyle::from(ui.style().as_ref()));
        let dark_mode = ui.visuals().dark_mode;

        let outline = gstyle.outline;
        let fill = if enabled {
            gstyle.fill
        } else {
            crate::themes::blend(gstyle.fill, ui.visuals().window_fill, 0.65)
        };
        let base_text_color = if dark_mode {
            crate::themes::ral(6027)
        } else {
            crate::themes::ral(9011)
        };
        let text_color = if enabled {
            base_text_color
        } else {
            crate::themes::blend(base_text_color, fill, 0.55)
        };

        ui.scope(|ui| {
            let visuals = ui.visuals_mut();
            visuals.override_text_color = Some(text_color);
            visuals.text_edit_bg_color = Some(fill);
            visuals.extreme_bg_color = fill;

            let widgets = &mut visuals.widgets;
            let outline_stroke = Stroke::new(1.0, outline);
            visuals.selection.stroke = outline_stroke;

            widgets.inactive.bg_stroke = outline_stroke;
            widgets.inactive.bg_fill = fill;
            widgets.inactive.weak_bg_fill = fill;
            widgets.inactive.corner_radius = gstyle.rounding.into();

            widgets.hovered.bg_stroke = outline_stroke;
            widgets.hovered.bg_fill = fill;
            widgets.hovered.weak_bg_fill = fill;
            widgets.hovered.corner_radius = gstyle.rounding.into();

            widgets.active.bg_stroke = outline_stroke;
            widgets.active.bg_fill = fill;
            widgets.active.weak_bg_fill = fill;
            widgets.active.corner_radius = gstyle.rounding.into();

            widgets.open.bg_stroke = outline_stroke;
            widgets.open.bg_fill = fill;
            widgets.open.weak_bg_fill = fill;
            widgets.open.corner_radius = gstyle.rounding.into();

            let response = ui.add(
                inner
                    .font(egui::TextStyle::Name("LCD".into()))
                    .frame(true)
                    .margin(ui.spacing().button_padding)
                    .min_size(ui.spacing().interact_size),
            );

            if ui.is_rect_visible(response.rect) {
                let painter = ui.painter();

                if enabled && response.has_focus() {
                    paint_scanline(painter, response.rect, base_text_color, gstyle.scanline_height);
                }
            }

            response
        })
        .inner
    }
}

impl crate::themes::Styled for TextField<'_> {
    type Style = GorbieTextFieldStyle;

    fn set_style(&mut self, style: Option<Self::Style>) {
        self.gorbie_style = style;
    }
}
