use crate::widgets::{Button, Column, TableBuilder, TextField};
use eframe::egui;
use egui::{Align, Align2, FontId, Layout, Margin, RichText, TextStyle};
use polars::prelude::{
    DataFrame, IdxCa, IdxSize, IntoSeries, NamedFrom, SortMultipleOptions,
};

pub fn dataframe(ui: &mut egui::Ui, df: &DataFrame) {
    let nr_cols = df.width();
    let nr_rows = df.height();
    let cols = &df.get_column_names();
    if nr_cols == 0 {
        ui.label("Empty dataframe");
        return;
    }

    let visuals = ui.visuals();
    let base_fill = visuals.panel_fill;
    let header_text = visuals.widgets.noninteractive.fg_stroke.color;
    let body_text = visuals.widgets.inactive.fg_stroke.color;
    let base_mono = ui
        .style()
        .text_styles
        .get(&TextStyle::Monospace)
        .map(|font| font.size)
        .unwrap_or(14.0);
    let body_font = FontId::monospace((base_mono - 2.0).max(9.0));
    let header_font = FontId::monospace(base_mono.max(10.0));
    let body_height = ui.fonts_mut(|fonts| fonts.row_height(&body_font));
    let header_text_height = ui.fonts_mut(|fonts| fonts.row_height(&header_font));
    let row_height = body_height.max(10.0);
    let cell_padding_x = 4.0;
    let header_height = header_text_height + 2.0;
    let sort_marker_text = " ↑88";
    let sort_marker_width = ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(sort_marker_text.to_string(), header_font.clone(), header_text)
            .size()
            .x
    });

    let header_min_widths: Vec<f32> = cols
        .iter()
        .map(|head| {
            ui.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(head.to_string(), header_font.clone(), header_text)
                    .size()
                    .x
                    + sort_marker_width
            })
        })
        .collect();

    let sort_state_id = ui.id().with("dataframe_sort_state");
    let sort_state =
        ui.data_mut(|data| data.get_temp::<DataframeSortState>(sort_state_id))
            .unwrap_or_default();
    let filter_state_id = ui.id().with("dataframe_filter_state");
    let filter_state =
        ui.data_mut(|data| data.get_temp::<DataframeFilterState>(filter_state_id))
            .unwrap_or_default();
    let selection_state_id = ui.id().with("dataframe_selection_state");
    let selection_state =
        ui.data_mut(|data| data.get_temp::<DataframeSelectionState>(selection_state_id))
            .unwrap_or_default();

    let summary_color = crate::themes::blend(body_text, base_fill, 0.55);

    egui::Frame::new()
        .fill(base_fill)
        .inner_margin(Margin::same(4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut next_filter_state = filter_state.clone();
            let mut next_selection_state = selection_state.clone();

            let clear_label = "clear";
            ui.horizontal(|ui| {
                ui.label(RichText::new("Filter").color(summary_color));
                let show_clear = !next_filter_state.query.is_empty();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.set_width(ui.available_width());
                    let clear_button = Button::new(clear_label).small();
                    let clear_response = if show_clear {
                        ui.add(clear_button)
                    } else {
                        ui.add_enabled(false, clear_button)
                    };
                    if show_clear && clear_response.clicked() {
                        next_filter_state.query.clear();
                    }
                    let text_width = ui.available_width().max(120.0);
                    let text_height = ui.spacing().interact_size.y;
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(text_width, text_height),
                        egui::Sense::hover(),
                    );
                    let mut text_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect)
                            .layout(Layout::left_to_right(Align::Center))
                            .id_salt(ui.id().with("dataframe_filter_input")),
                    );
                    text_ui.spacing_mut().text_edit_width = rect.width();
                    text_ui.add(TextField::singleline(&mut next_filter_state.query));
                });
            });

            let query = next_filter_state.query.trim();
            let filter_active = !query.is_empty();
            let query_lower = query.to_lowercase();
            let row_order = if !sort_state.keys.is_empty() || filter_active {
                let mut order = (0..nr_rows).collect::<Vec<_>>();

                if filter_active {
                    order.retain(|&row_index| {
                        cols.iter().any(|col_name| {
                            if let Ok(column) = df.column(col_name.as_str()) {
                                if let Ok(value) = column.get(row_index) {
                                    let value_text = value.to_string().to_lowercase();
                                    return value_text.contains(&query_lower);
                                }
                            }
                            false
                        })
                    });
                }

                if !sort_state.keys.is_empty() && !order.is_empty() {
                    let idx_values =
                        order.iter().map(|value| *value as IdxSize).collect::<Vec<_>>();
                    let idx = IdxCa::new("idx".into(), idx_values.clone());
                    if let Ok(mut subset) = df.take(&idx) {
                        let row_id_series = IdxCa::new("__row_id".into(), idx_values);
                        let _ = subset.with_column(row_id_series.into_series());
                        let sort_cols: Vec<_> = sort_state
                            .keys
                            .iter()
                            .filter_map(|key| cols.get(key.column).map(|name| (*name).clone()))
                            .collect();
                        let descending: Vec<bool> =
                            sort_state.keys.iter().map(|key| key.descending).collect();
                        if !sort_cols.is_empty() {
                            let options = SortMultipleOptions::new()
                                .with_order_descending_multi(descending);
                            if let Ok(sorted) = subset.sort(sort_cols, options) {
                                if let Ok(row_ids) = sorted.column("__row_id") {
                                    if let Ok(idxs) = row_ids.idx() {
                                        order = idxs
                                            .into_no_null_iter()
                                            .map(|value| value as usize)
                                            .collect();
                                    }
                                }
                            }
                        }
                    }
                }

                Some(order)
            } else {
                None
            };
            let display_rows = row_order.as_ref().map_or(nr_rows, |order| order.len());
            if let Some(selected_row) = next_selection_state.row {
                if let Some(order) = row_order.as_ref() {
                    if !order.contains(&selected_row) {
                        next_selection_state.row = None;
                    }
                }
            }

            let summary = if filter_active {
                format!("{display_rows} of {nr_rows} rows × {nr_cols} columns")
            } else {
                format!("{nr_rows} rows × {nr_cols} columns")
            };
            ui.add_space(4.0);
            ui.label(RichText::new(summary).color(summary_color));
            ui.add_space(4.0);

            let spacing_x = ui.spacing().item_spacing.x;
            let mut column_mins: Vec<f32> = header_min_widths
                .iter()
                .map(|min_width| min_width + cell_padding_x * 2.0)
                .collect();
            let spacing_total = spacing_x * (nr_cols.saturating_sub(1) as f32);
            let available_for_columns = (ui.available_width() - spacing_total).max(0.0);
            let min_total: f32 = column_mins.iter().sum();
            if min_total > available_for_columns && available_for_columns > 0.0 {
                let scale = available_for_columns / min_total;
                for width in &mut column_mins {
                    *width *= scale;
                }
            }

            let mut table = TableBuilder::new(ui)
                .resizable(true)
                .cell_layout(Layout::left_to_right(Align::Min))
                .dense_rows(true)
                .header_body_gap(0.0)
                .sense(egui::Sense::click_and_drag());

            for min_width in &column_mins {
                table = table.column(Column::remainder().at_least(*min_width).clip(true));
            }

            let mut next_sort_state = sort_state.clone();
            let table = table.header(header_height, |mut header| {
                for (col_idx, head) in cols.iter().enumerate() {
                    header.col(|ui| {
                        let key_index = sort_state
                            .keys
                            .iter()
                            .position(|key| key.column == col_idx);
                        let desired_size = egui::vec2(ui.available_width(), header_height);
                        let (rect, response) = ui.allocate_exact_size(
                            desired_size,
                            egui::Sense::click(),
                        );
                        let mut header_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(rect)
                                .layout(Layout::left_to_right(Align::Center)),
                        );
                        header_ui.add_space(cell_padding_x);
                        header_ui.label(
                            RichText::new(head.to_string())
                                .color(header_text)
                                .font(header_font.clone())
                                .strong(),
                        );
                        if let Some(sort_pos) = key_index {
                            let descending = sort_state.keys[sort_pos].descending;
                            let arrow = if descending { "↓" } else { "↑" };
                            let marker = if sort_state.keys.len() > 1 {
                                format!("{arrow}{}", sort_pos + 1)
                            } else {
                                arrow.to_string()
                            };
                            let marker_pos = egui::pos2(
                                rect.max.x - cell_padding_x,
                                rect.center().y,
                            );
                            ui.painter().text(
                                marker_pos,
                                Align2::RIGHT_CENTER,
                                marker,
                                header_font.clone(),
                                header_text,
                            );
                        }

                        if response.clicked() {
                            let shift = ui.input(|i| i.modifiers.shift);
                            if shift {
                                if let Some(existing) = key_index {
                                    if sort_state.keys[existing].descending {
                                        next_sort_state.keys.remove(existing);
                                    } else {
                                        next_sort_state.keys[existing].descending = true;
                                    }
                                } else {
                                    next_sort_state.keys.push(SortKey {
                                        column: col_idx,
                                        descending: false,
                                    });
                                }
                            } else if let Some(existing) = key_index {
                                if existing == 0 && !sort_state.keys[existing].descending {
                                    next_sort_state.keys[existing].descending = true;
                                } else if existing == 0 {
                                    next_sort_state.keys.clear();
                                } else {
                                    next_sort_state.keys.clear();
                                    next_sort_state.keys.push(SortKey {
                                        column: col_idx,
                                        descending: false,
                                    });
                                }
                            } else {
                                next_sort_state.keys.clear();
                                next_sort_state.keys.push(SortKey {
                                    column: col_idx,
                                    descending: false,
                                });
                            }
                        }
                    });
                }
            });
            let row_order_ref = row_order.as_deref();
            table.body(|body| {
                body.rows(row_height, display_rows, |mut row| {
                    let row_index = row.index();
                    let row_index = row_order_ref.map_or(row_index, |order| order[row_index]);
                    let is_selected = next_selection_state.row == Some(row_index);
                    row.set_selected(is_selected);
                    for col in cols {
                        row.col(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), row_height),
                                Layout::left_to_right(Align::Min),
                                |ui| {
                                    ui.add_space(cell_padding_x);
                                    if let Ok(column) = &df.column(col.as_str()) {
                                        if let Ok(value) = column.get(row_index) {
                                            ui.label(
                                                RichText::new(format!("{value}"))
                                                    .color(body_text)
                                                    .font(body_font.clone()),
                                            );
                                        }
                                    }
                                },
                            );
                        });
                    }
                    if row.response().clicked() {
                        next_selection_state.row = if is_selected {
                            None
                        } else {
                            Some(row_index)
                        };
                    }
                });
            });

            if next_sort_state != sort_state {
                ui.data_mut(|data| data.insert_temp(sort_state_id, next_sort_state));
            }
            if let Some(selected_row) = next_selection_state.row {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("Selected row {selected_row}"))
                        .color(summary_color),
                );
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    for col_name in cols {
                        if let Ok(column) = df.column(col_name.as_str()) {
                            if let Ok(value) = column.get(selected_row) {
                                ui.label(
                                    RichText::new(format!("{col_name}: {value}"))
                                        .color(body_text)
                                        .font(body_font.clone()),
                                );
                            }
                        }
                    }
                });
            } else if filter_active && display_rows == 0 {
                ui.add_space(6.0);
                ui.label(RichText::new("No rows match this filter").color(summary_color));
            }

            if next_filter_state != filter_state {
                ui.data_mut(|data| data.insert_temp(filter_state_id, next_filter_state));
            }
            if next_selection_state != selection_state {
                ui.data_mut(|data| data.insert_temp(selection_state_id, next_selection_state));
            }
        });
}

#[derive(Clone, Default, PartialEq)]
struct DataframeSortState {
    keys: Vec<SortKey>,
}

#[derive(Clone, PartialEq)]
struct SortKey {
    column: usize,
    descending: bool,
}

#[derive(Clone, Default, PartialEq)]
struct DataframeFilterState {
    query: String,
}

#[derive(Clone, Default, PartialEq)]
struct DataframeSelectionState {
    row: Option<usize>,
}
