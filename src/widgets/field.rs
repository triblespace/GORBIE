use std::ops::RangeInclusive;

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CursorIcon, Event, EventFilter, FontId, Id, Key,
    Margin, NumExt as _, Pos2, Rect, Response, Stroke, StrokeKind, Ui, Vec2, Widget,
};

use egui::text::{CCursor, CCursorRange, LayoutJob};

use crate::themes::{GorbieNumberFieldStyle, GorbieTextFieldStyle};

type NumFormatter<'a> = dyn Fn(f64, RangeInclusive<usize>) -> String + 'a;
type NumParser<'a> = dyn Fn(&str) -> Option<f64> + 'a;

fn paint_scanline(painter: &egui::Painter, rect: Rect, color: Color32, height: f32) {
    let inset = 2.0;
    let available_h = (rect.height() - inset * 2.0).max(0.0);
    let height = height.min(available_h);
    if height <= 0.0 {
        return;
    }

    let y1 = rect.bottom() - inset;
    let y0 = y1 - height;
    let scan_rect = Rect::from_min_max(
        pos2(rect.left() + inset, y0),
        pos2(rect.right() - inset, y1),
    );
    if scan_rect.is_positive() {
        painter.rect_filled(scan_rect, 0.0, color);
    }
}

fn selection_rects(galley: &egui::Galley, cursor_range: egui::text::CCursorRange) -> Vec<Rect> {
    if cursor_range.is_empty() {
        return Vec::new();
    }

    let [min, max] = cursor_range.sorted_cursors();
    let min = galley.layout_from_cursor(min);
    let max = galley.layout_from_cursor(max);

    let mut rects = Vec::new();

    for row_idx in min.row..=max.row {
        let row = &galley.rows[row_idx];

        let left = if row_idx == min.row {
            row.x_offset(min.column)
        } else {
            0.0
        };

        let right = if row_idx == max.row {
            row.x_offset(max.column)
        } else {
            let newline_size = if row.ends_with_newline {
                row.size.y / 2.0
            } else {
                0.0
            };
            row.size.x + newline_size
        };

        let rect = Rect::from_min_max(pos2(left, 0.0), pos2(right, row.size.y))
            .translate(row.pos.to_vec2());
        if rect.is_positive() {
            rects.push(rect);
        }
    }

    rects
}

#[derive(Debug, Clone, Copy)]
struct GalleyPlacement {
    rect: Rect,
    pos: Pos2,
}

fn place_galley(galley: &egui::Galley, rect: Rect, align: Align2) -> GalleyPlacement {
    let placement_rect = align
        .align_size_within_rect(galley.size(), rect)
        .intersect(rect);
    let galley_pos = placement_rect.min - galley.rect.min.to_vec2();

    GalleyPlacement {
        rect: placement_rect,
        pos: galley_pos,
    }
}

fn is_scrollable_singleline(
    clip_text: bool,
    rect: Rect,
    placement_rect: Rect,
    galley: &egui::Galley,
) -> bool {
    clip_text && placement_rect.left() == rect.left() && galley.rect.left() == 0.0
}

fn cursor_rect_in_galley(galley: &egui::Galley, cursor: CCursor) -> Rect {
    let cursor = galley.layout_from_cursor(cursor);
    galley.rows.get(cursor.row).map_or_else(
        || Rect::ZERO,
        |row| {
            let x = row.pos.x + row.x_offset(cursor.column);
            Rect::from_min_max(pos2(x, row.min_y()), pos2(x, row.max_y()))
        },
    )
}

fn paint_field_frame(
    painter: &egui::Painter,
    rect: Rect,
    fill: Color32,
    outline: Color32,
    rounding: f32,
) {
    painter.rect_filled(rect, rounding, fill);
    painter.rect_stroke(
        rect,
        rounding,
        Stroke::new(1.0, outline),
        StrokeKind::Inside,
    );
}

fn lcd_ink_color(dark_mode: bool) -> Color32 {
    if dark_mode {
        crate::themes::ral(6027)
    } else {
        crate::themes::ral(9011)
    }
}

fn default_event_filter() -> EventFilter {
    EventFilter {
        horizontal_arrows: true,
        vertical_arrows: true,
        tab: false,
        ..Default::default()
    }
}

fn default_parser(text: &str) -> Option<f64> {
    let text: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == '−' { '-' } else { c })
        .collect();

    text.parse().ok()
}

fn parse_number(custom_parser: Option<&NumParser<'_>>, value_text: &str) -> Option<f64> {
    custom_parser.map_or_else(|| default_parser(value_text), |parser| parser(value_text))
}

fn layout_lcd_job(
    text: &str,
    font_id: FontId,
    normal_color: Color32,
    wrap_width: f32,
    multiline: bool,
    halign: Align,
) -> LayoutJob {
    let mut job = if multiline {
        LayoutJob::simple(text.to_owned(), font_id, normal_color, wrap_width)
    } else {
        LayoutJob::simple_singleline(text.to_owned(), font_id, normal_color)
    };
    job.halign = halign;
    job
}

#[derive(Clone, Default)]
struct LcdTextEditState {
    cursor: egui::text_selection::TextCursorState,
    singleline_offset: f32,
    last_interaction_time: f64,
}

impl LcdTextEditState {
    fn load(ctx: &egui::Context, id: Id) -> Self {
        ctx.data_mut(|data| data.get_temp(id)).unwrap_or_default()
    }

    fn store(self, ctx: &egui::Context, id: Id) {
        ctx.data_mut(|data| data.insert_temp(id, self));
    }
}

struct LcdTextEditOutput {
    response: Response,
    changed: bool,
}

#[allow(clippy::too_many_arguments)]
fn lcd_text_edit(
    ui: &mut Ui,
    id: Id,
    text: &mut dyn egui::TextBuffer,
    multiline: bool,
    expand_width: bool,
    desired_width: f32,
    desired_height_rows: usize,
    min_size: Vec2,
    margin: Margin,
    align: Align2,
    clip_text: bool,
    fill: Color32,
    outline: Color32,
    rounding: f32,
    ink: Color32,
    text_color: Color32,
    scanline_height: f32,
) -> LcdTextEditOutput {
    let interactive = ui.is_enabled() && text.is_mutable();
    let event_filter = default_event_filter();

    let font_id = egui::TextStyle::Name("LCD".into()).resolve(ui.style());
    let row_height = ui.fonts(|fonts| fonts.row_height(&font_id));

    const MIN_WIDTH: f32 = 24.0;
    let available_width = (ui.available_width() - margin.sum().x).at_least(MIN_WIDTH);
    let wrap_width = if expand_width {
        available_width
    } else {
        desired_width.min(available_width)
    };

    let mut galley = ui.fonts(|fonts| {
        fonts.layout_job(layout_lcd_job(
            text.as_str(),
            font_id.clone(),
            text_color,
            wrap_width,
            multiline,
            align.x(),
        ))
    });

    let desired_inner_width = if clip_text {
        wrap_width
    } else {
        galley.size().x.max(wrap_width)
    };
    let desired_height = (desired_height_rows.at_least(1) as f32) * row_height;
    let desired_inner_size = vec2(desired_inner_width, galley.size().y.max(desired_height));
    let desired_outer_size = (desired_inner_size + margin.sum()).at_least(min_size);
    let (_auto_id, outer_rect) = ui.allocate_space(desired_outer_size);
    let rect = outer_rect - margin;
    let text_clip_rect = outer_rect.shrink(1.0);

    let allow_drag_to_select =
        !ui.input(|i| i.has_touch_screen()) || ui.memory(|mem| mem.has_focus(id));
    let sense = if interactive {
        if allow_drag_to_select {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::click()
        }
    } else {
        egui::Sense::hover()
    };
    let mut response = ui.interact(outer_rect, id, sense);

    paint_field_frame(ui.painter(), outer_rect, fill, outline, rounding);

    let mut state = LcdTextEditState::load(ui.ctx(), id);
    let galley_placement = place_galley(&galley, rect, align);
    let galley_pos_unscrolled = galley_placement.pos;
    let scrollable_singleline =
        is_scrollable_singleline(clip_text, rect, galley_placement.rect, &galley);

    if interactive {
        if let Some(pointer_pos) = response.interact_pointer_pos() {
            let scroll_offset = if scrollable_singleline {
                state.singleline_offset
            } else {
                0.0
            };
            let cursor_at_pointer = galley
                .cursor_from_pos(pointer_pos - galley_pos_unscrolled + vec2(scroll_offset, 0.0));

            let is_being_dragged = ui.ctx().is_being_dragged(response.id);
            let did_interact = state.cursor.pointer_interaction(
                ui,
                &response,
                cursor_at_pointer,
                &galley,
                is_being_dragged,
            );

            if did_interact || response.clicked() {
                ui.memory_mut(|mem| mem.request_focus(response.id));
                state.last_interaction_time = ui.input(|i| i.time);
            }
        }

        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Text);
        }
    }

    let mut changed = false;
    let has_focus = ui.memory(|mem| mem.has_focus(id));
    let os = ui.ctx().os();

    let mut cursor_range = state
        .cursor
        .range(&galley)
        .unwrap_or_else(|| CCursorRange::one(galley.end()));

    if interactive && has_focus {
        ui.memory_mut(|mem| mem.set_focus_lock_filter(id, event_filter));

        let prev_cursor_range = cursor_range;
        let mut selection_changed = false;

        for event in ui.input(|i| i.filtered_events(&event_filter)).iter() {
            let did_mutate_text = match event {
                event if cursor_range.on_event(os, event, &galley, id) => {
                    selection_changed = selection_changed || prev_cursor_range != cursor_range;
                    None
                }
                Event::Copy => {
                    if !cursor_range.is_empty() {
                        ui.ctx()
                            .copy_text(cursor_range.slice_str(text.as_str()).to_owned());
                    }
                    None
                }
                Event::Cut => {
                    if cursor_range.is_empty() {
                        None
                    } else {
                        ui.ctx()
                            .copy_text(cursor_range.slice_str(text.as_str()).to_owned());
                        Some(CCursorRange::one(text.delete_selected(&cursor_range)))
                    }
                }
                Event::Paste(text_to_insert) => {
                    if text_to_insert.is_empty() {
                        None
                    } else {
                        let mut ccursor = text.delete_selected(&cursor_range);
                        if multiline {
                            text.insert_text_at(&mut ccursor, text_to_insert, usize::MAX);
                        } else {
                            let single_line = text_to_insert.replace(['\r', '\n'], " ");
                            text.insert_text_at(&mut ccursor, &single_line, usize::MAX);
                        }
                        Some(CCursorRange::one(ccursor))
                    }
                }
                Event::Text(text_to_insert) => {
                    if text_to_insert.is_empty() || text_to_insert == "\n" || text_to_insert == "\r"
                    {
                        None
                    } else {
                        let mut ccursor = text.delete_selected(&cursor_range);
                        text.insert_text_at(&mut ccursor, text_to_insert, usize::MAX);
                        Some(CCursorRange::one(ccursor))
                    }
                }
                Event::Key {
                    key: Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.is_none() => {
                    if multiline {
                        let mut ccursor = text.delete_selected(&cursor_range);
                        text.insert_text_at(&mut ccursor, "\n", usize::MAX);
                        Some(CCursorRange::one(ccursor))
                    } else {
                        ui.memory_mut(|mem| mem.surrender_focus(id));
                        break;
                    }
                }
                Event::Key {
                    key: Key::Backspace,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    let ccursor = if modifiers.mac_cmd {
                        text.delete_paragraph_before_cursor(&galley, &cursor_range)
                    } else if let Some(cursor) = cursor_range.single() {
                        if modifiers.alt || modifiers.ctrl {
                            text.delete_previous_word(cursor)
                        } else {
                            text.delete_previous_char(cursor)
                        }
                    } else {
                        text.delete_selected(&cursor_range)
                    };
                    Some(CCursorRange::one(ccursor))
                }
                Event::Key {
                    key: Key::Delete,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.shift || os != egui::os::OperatingSystem::Windows => {
                    let ccursor = if modifiers.mac_cmd {
                        text.delete_paragraph_after_cursor(&galley, &cursor_range)
                    } else if let Some(cursor) = cursor_range.single() {
                        if modifiers.alt || modifiers.ctrl {
                            text.delete_next_word(cursor)
                        } else {
                            text.delete_next_char(cursor)
                        }
                    } else {
                        text.delete_selected(&cursor_range)
                    };
                    Some(CCursorRange::one(CCursor {
                        prefer_next_row: true,
                        ..ccursor
                    }))
                }
                _ => None,
            };

            if let Some(new_cursor_range) = did_mutate_text {
                changed = true;
                selection_changed = true;

                galley = ui.fonts(|fonts| {
                    fonts.layout_job(layout_lcd_job(
                        text.as_str(),
                        font_id.clone(),
                        text_color,
                        wrap_width,
                        multiline,
                        align.x(),
                    ))
                });
                cursor_range = new_cursor_range;
            }
        }

        state.cursor.set_char_range(Some(cursor_range));

        if changed || selection_changed {
            state.last_interaction_time = ui.input(|i| i.time);
        }
    }

    let galley_placement = place_galley(&galley, rect, align);
    let mut galley_pos = galley_placement.pos;
    let scrollable_singleline =
        is_scrollable_singleline(clip_text, rect, galley_placement.rect, &galley);

    if scrollable_singleline {
        let cursor_pos = if has_focus {
            galley.pos_from_cursor(cursor_range.primary).min.x
        } else {
            0.0
        };

        let mut offset_x = state.singleline_offset;
        let visible_range = offset_x..=offset_x + desired_inner_size.x;

        if !visible_range.contains(&cursor_pos) {
            if cursor_pos < *visible_range.start() {
                offset_x = cursor_pos;
            } else {
                offset_x = cursor_pos - desired_inner_size.x;
            }
        }

        offset_x = offset_x
            .at_most(galley.size().x - desired_inner_size.x)
            .at_least(0.0);

        state.singleline_offset = offset_x;
        galley_pos -= vec2(offset_x, 0.0);
    } else {
        state.singleline_offset = 0.0;
    }

    let text_painter = ui.painter_at(text_clip_rect);

    if interactive && has_focus {
        let mut highlight_rects = Vec::new();
        let mut invert_text_rects = Vec::new();

        if !cursor_range.is_empty() {
            for selection_rect in selection_rects(&galley, cursor_range) {
                let selection_rect = selection_rect.translate(galley_pos.to_vec2());
                if selection_rect.is_positive() {
                    highlight_rects.push(selection_rect);
                    invert_text_rects.push(selection_rect);
                }
            }
        } else {
            let now = ui.input(|i| i.time);
            if state.last_interaction_time == 0.0 {
                state.last_interaction_time = now;
            }

            let show_block = if ui.visuals().text_cursor.blink {
                let cursor_style = &ui.visuals().text_cursor;
                let total_duration = cursor_style.on_duration + cursor_style.off_duration;

                let time_since_last_interaction = (now - state.last_interaction_time).max(0.0);
                let time_in_cycle = (time_since_last_interaction % (total_duration as f64)) as f32;

                let (show, wake_in) = if time_in_cycle < cursor_style.on_duration {
                    (true, cursor_style.on_duration - time_in_cycle)
                } else {
                    (false, total_duration - time_in_cycle)
                };

                ui.ctx().request_repaint_after_secs(wake_in);
                show
            } else {
                true
            };

            if show_block {
                let cursor = cursor_range.primary;
                let char_count = text.as_str().chars().count();
                let cursor_rect = cursor_rect_in_galley(&galley, cursor);
                let glyph_width = ui.fonts(|fonts| fonts.glyph_width(&font_id, '0'));

                let cursor_width = if cursor.index < char_count {
                    let next = cursor_rect_in_galley(&galley, cursor + 1);
                    let width = next.min.x - cursor_rect.min.x;
                    if width > 0.0 {
                        width
                    } else {
                        glyph_width
                    }
                } else {
                    glyph_width
                };

                let caret_rect = Rect::from_min_max(
                    pos2(cursor_rect.min.x, cursor_rect.min.y),
                    pos2(cursor_rect.min.x + cursor_width, cursor_rect.max.y),
                )
                .translate(galley_pos.to_vec2());

                if caret_rect.is_positive() {
                    highlight_rects.push(caret_rect);
                    if cursor.index < char_count {
                        invert_text_rects.push(caret_rect);
                    }
                }
            }
        }

        for rect in &highlight_rects {
            text_painter.rect_filled(*rect, 0.0, ink);
        }

        text_painter.galley(galley_pos, galley.clone(), text_color);

        for rect in invert_text_rects {
            let clip = text_clip_rect.intersect(rect);
            if clip.is_positive() {
                let overlay = ui.painter_at(clip);
                overlay.galley_with_override_text_color(galley_pos, galley.clone(), fill);
            }
        }

        paint_scanline(ui.painter(), outer_rect, ink, scanline_height);
    } else {
        text_painter.galley(galley_pos, galley.clone(), text_color);
    }

    if changed {
        response.mark_changed();
    }

    state.store(ui.ctx(), id);

    LcdTextEditOutput { response, changed }
}

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct NumberField<'a, Num: egui::emath::Numeric> {
    value: &'a mut Num,
    speed: f64,
    constrain_value: Option<&'a dyn Fn(Num) -> Num>,
    prefix: String,
    suffix: String,
    min_decimals: usize,
    max_decimals: Option<usize>,
    custom_formatter: Option<&'a NumFormatter<'a>>,
    custom_parser: Option<&'a NumParser<'a>>,
    update_while_editing: bool,
    gorbie_style: Option<GorbieNumberFieldStyle>,
}

impl<'a, Num: egui::emath::Numeric> NumberField<'a, Num> {
    pub fn new(value: &'a mut Num) -> Self {
        Self {
            value,
            speed: 1.0,
            constrain_value: None,
            prefix: String::new(),
            suffix: String::new(),
            min_decimals: 0,
            max_decimals: None,
            custom_formatter: None,
            custom_parser: None,
            update_while_editing: true,
            gorbie_style: None,
        }
    }

    /// Apply a constraint function to all user-produced changes (drag or committed edit).
    ///
    /// This is useful for clamping, snapping, or other domain-specific normalization without
    /// baking policy into the widget.
    pub fn constrain_value(mut self, constrain: &'a dyn Fn(Num) -> Num) -> Self {
        self.constrain_value = Some(constrain);
        self
    }

    pub fn speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    pub fn min_decimals(mut self, min_decimals: usize) -> Self {
        self.min_decimals = min_decimals;
        self
    }

    pub fn max_decimals(mut self, max_decimals: usize) -> Self {
        self.max_decimals = Some(max_decimals);
        self
    }

    pub fn max_decimals_opt(mut self, max_decimals: Option<usize>) -> Self {
        self.max_decimals = max_decimals;
        self
    }

    pub fn update_while_editing(mut self, update: bool) -> Self {
        self.update_while_editing = update;
        self
    }

    pub fn custom_formatter(mut self, formatter: &'a NumFormatter) -> Self {
        self.custom_formatter = Some(formatter);
        self
    }

    pub fn custom_parser(mut self, parser: &'a NumParser<'a>) -> Self {
        self.custom_parser = Some(parser);
        self
    }
}

impl<Num: egui::emath::Numeric> Widget for NumberField<'_, Num> {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            value,
            speed,
            constrain_value,
            prefix,
            suffix,
            min_decimals,
            max_decimals,
            custom_formatter,
            custom_parser,
            update_while_editing,
            gorbie_style,
        } = self;

        let enabled = ui.is_enabled();
        let gstyle =
            gorbie_style.unwrap_or_else(|| GorbieNumberFieldStyle::from(ui.style().as_ref()));

        let dark_mode = ui.visuals().dark_mode;
        let ink = lcd_ink_color(dark_mode);

        let outline = gstyle.outline;
        let fill = if enabled {
            gstyle.fill
        } else {
            crate::themes::blend(gstyle.fill, ui.visuals().window_fill, 0.65)
        };
        let text_color = if enabled {
            ink
        } else {
            crate::themes::blend(ink, fill, 0.55)
        };

        let margin: Margin = ui.spacing().button_padding.into();
        let font_id = egui::TextStyle::Name("LCD".into()).resolve(ui.style());
        let row_height = ui.fonts(|fonts| fonts.row_height(&font_id));
        let mut desired_width = (ui.spacing().interact_size.x - margin.sum().x).at_least(24.0);

        let id = ui.next_auto_id();
        let is_editing = enabled
            && ui.memory_mut(|mem| {
                mem.interested_in_focus(id, ui.layer_id());
                mem.has_focus(id)
            });

        let aim_rad = ui.input(|i| i.aim_radius() as f64);
        let is_slow_speed = ui.input(|i| i.modifiers.shift_only()) && ui.ctx().is_being_dragged(id);

        let auto_decimals = if Num::INTEGRAL {
            0
        } else {
            (aim_rad / speed.abs()).log10().ceil().clamp(0.0, 15.0) as usize
        };
        let auto_decimals = auto_decimals + is_slow_speed as usize;
        let max_decimals = max_decimals
            .unwrap_or(auto_decimals + 2)
            .at_least(min_decimals);
        let auto_decimals = auto_decimals.clamp(min_decimals, max_decimals);

        let value_f64 = value.to_f64();

        let value_text = match custom_formatter {
            Some(formatter) => formatter(value_f64, auto_decimals..=max_decimals),
            None => ui
                .style()
                .number_formatter
                .format(value_f64, auto_decimals..=max_decimals),
        };
        let display_text = format!("{prefix}{value_text}{suffix}");
        let display_galley = ui.fonts(|fonts| {
            fonts.layout_job(layout_lcd_job(
                &display_text,
                font_id.clone(),
                text_color,
                desired_width,
                false,
                Align::Center,
            ))
        });
        desired_width = desired_width.max(display_galley.size().x);

        if is_editing {
            let mut edit_text = ui
                .data_mut(|data| data.remove_temp::<String>(id))
                .unwrap_or_else(|| value_text.clone());

            let output = lcd_text_edit(
                ui,
                id,
                &mut edit_text,
                false,
                false,
                desired_width,
                1,
                ui.spacing().interact_size,
                margin,
                Align2::CENTER_CENTER,
                true,
                fill,
                outline,
                gstyle.rounding,
                ink,
                text_color,
                gstyle.scanline_height,
            );
            let mut response = output.response;

            let commit = if update_while_editing {
                output.changed
            } else {
                response.lost_focus() && !ui.input(|i| i.key_pressed(Key::Escape))
            };

            if commit {
                if let Some(mut parsed_value) = parse_number(custom_parser, &edit_text) {
                    if Num::INTEGRAL {
                        parsed_value = parsed_value.round();
                    }
                    let mut new_value = Num::from_f64(parsed_value);
                    if let Some(constrain_value) = constrain_value {
                        new_value = constrain_value(new_value);
                    }
                    if new_value != *value {
                        *value = new_value;
                        response.mark_changed();
                    }
                }
            }

            if ui.memory(|mem| mem.has_focus(id)) {
                ui.data_mut(|data| data.insert_temp(id, edit_text));
            } else {
                ui.data_mut(|data| data.remove::<String>(id));
            }

            response
        } else {
            let desired_inner_width = desired_width.max(display_galley.size().x);
            let desired_inner_height = (ui.spacing().interact_size.y - margin.sum().y)
                .max(row_height)
                .max(display_galley.size().y);
            let desired_inner_size = vec2(desired_inner_width, desired_inner_height);
            let desired_outer_size =
                (desired_inner_size + margin.sum()).at_least(ui.spacing().interact_size);
            let (_auto_id, outer_rect) = ui.allocate_space(desired_outer_size);
            let rect = outer_rect - margin;
            let mut response = ui.interact(outer_rect, id, egui::Sense::click_and_drag());

            paint_field_frame(ui.painter(), outer_rect, fill, outline, gstyle.rounding);

            let galley_pos = place_galley(&display_galley, rect, Align2::CENTER_CENTER).pos;
            ui.painter_at(outer_rect.shrink(1.0))
                .galley(galley_pos, display_galley, text_color);

            if enabled {
                if response.clicked() {
                    ui.memory_mut(|mem| mem.request_focus(id));
                    ui.data_mut(|data| data.insert_temp(id, value_text.clone()));

                    let mut state = LcdTextEditState::load(ui.ctx(), id);
                    let len = value_text.chars().count();
                    state.cursor.set_char_range(Some(CCursorRange::two(
                        CCursor::default(),
                        CCursor::new(len),
                    )));
                    state.last_interaction_time = ui.input(|i| i.time);
                    state.store(ui.ctx(), id);
                } else if response.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                    let mdelta = response.drag_delta();
                    let delta_points = mdelta.x - mdelta.y;
                    let mut new_value = value_f64 + delta_points as f64 * speed;
                    if Num::INTEGRAL {
                        new_value = new_value.round();
                    }
                    let mut new_value = Num::from_f64(new_value);
                    if let Some(constrain_value) = constrain_value {
                        new_value = constrain_value(new_value);
                    }
                    if new_value != *value {
                        *value = new_value;
                        response.mark_changed();
                    }
                }
            }

            response
        }
    }
}

impl<Num: egui::emath::Numeric> crate::themes::Styled for NumberField<'_, Num> {
    type Style = GorbieNumberFieldStyle;

    fn set_style(&mut self, style: Option<Self::Style>) {
        self.gorbie_style = style;
    }
}

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct TextField<'a> {
    text: &'a mut dyn egui::TextBuffer,
    multiline: bool,
    gorbie_style: Option<GorbieTextFieldStyle>,
}

impl<'a> TextField<'a> {
    pub fn singleline(text: &'a mut dyn egui::TextBuffer) -> Self {
        Self {
            text,
            multiline: false,
            gorbie_style: None,
        }
    }

    pub fn multiline(text: &'a mut dyn egui::TextBuffer) -> Self {
        Self {
            text,
            multiline: true,
            gorbie_style: None,
        }
    }
}

impl Widget for TextField<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            text,
            multiline,
            gorbie_style,
        } = self;

        let enabled = ui.is_enabled();
        let gstyle =
            gorbie_style.unwrap_or_else(|| GorbieTextFieldStyle::from(ui.style().as_ref()));

        let outline = gstyle.outline;
        let fill = if enabled {
            gstyle.fill
        } else {
            crate::themes::blend(gstyle.fill, ui.visuals().window_fill, 0.65)
        };

        let ink = lcd_ink_color(ui.visuals().dark_mode);
        let text_color = if enabled {
            ink
        } else {
            crate::themes::blend(ink, fill, 0.55)
        };

        let margin: Margin = ui.spacing().button_padding.into();
        let align = Align2([ui.layout().horizontal_align(), ui.layout().vertical_align()]);

        let output = lcd_text_edit(
            ui,
            ui.next_auto_id(),
            text,
            multiline,
            ui.layout().horizontal_justify(),
            ui.spacing().text_edit_width,
            if multiline { 4 } else { 1 },
            ui.spacing().interact_size,
            margin,
            align,
            !multiline,
            fill,
            outline,
            gstyle.rounding,
            ink,
            text_color,
            gstyle.scanline_height,
        );

        output.response
    }
}

impl crate::themes::Styled for TextField<'_> {
    type Style = GorbieTextFieldStyle;

    fn set_style(&mut self, style: Option<Self::Style>) {
        self.gorbie_style = style;
    }
}
