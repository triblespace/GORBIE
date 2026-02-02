use crate::widgets::{Column, TableBuilder, TextField};
use eframe::egui;
use egui::{Align, FontId, Layout, Margin, RichText, TextStyle};
use polars::prelude::{DataFrame, IntoLazy};
use polars::sql::SQLContext;

pub fn dataframe(ui: &mut egui::Ui, df: &DataFrame) {
    let nr_cols = df.width();
    let nr_rows = df.height();
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
    let header_pad_y = 2.0;
    let header_height = header_text_height + header_pad_y * 2.0;
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
            let text_width = ui.available_width().max(120.0);
            ui.spacing_mut().text_edit_width = text_width;
            ui.spacing_mut().interact_size.y = row_height;
            let query_response = ui
                .push_id("dataframe_filter_input", |ui| {
                    ui.add(TextField::multiline(&mut next_filter_state.query).rows(1))
                })
                .inner;
            if !query_response.has_focus() && next_filter_state.query.trim().is_empty() {
                next_filter_state.query = DEFAULT_QUERY.to_string();
            }

            let query = next_filter_state.query.trim();
            let query_active = !query.is_empty() && !is_default_query(query);
            let mut query_error: Option<String> = None;
            let mut sql_df: Option<DataFrame> = None;
            if query_active {
                let sql = build_sql_query(query);
                let mut ctx = SQLContext::new();
                ctx.register("self", df.clone().lazy());
                match ctx.execute(&sql).and_then(|lf| lf.collect()) {
                    Ok(result) => sql_df = Some(result),
                    Err(err) => query_error = Some(err.to_string()),
                }
            }

            let active_df = sql_df.as_ref().unwrap_or(df);
            let active_cols: Vec<String> = active_df
                .get_column_names()
                .iter()
                .map(|name| name.to_string())
                .collect();
            let active_nr_cols = active_df.width();
            let active_nr_rows = active_df.height();

            let display_rows = active_nr_rows;
            if let Some(selected_row) = next_selection_state.row {
                if selected_row >= active_nr_rows {
                    next_selection_state.row = None;
                }
            }

            let header_min_widths: Vec<f32> = active_cols
                .iter()
                .map(|head| {
                    ui.fonts_mut(|fonts| {
                        fonts
                            .layout_no_wrap(
                                head.to_string(),
                                header_font.clone(),
                                header_text,
                            )
                            .size()
                            .x
                    })
                })
                .collect();

            let rows_filtered = active_nr_rows != nr_rows;
            let cols_filtered = active_nr_cols != nr_cols;
            let summary = if rows_filtered || cols_filtered {
                let rows_text = if rows_filtered {
                    format!("{display_rows} of {nr_rows} rows")
                } else {
                    format!("{nr_rows} rows")
                };
                let cols_text = if cols_filtered {
                    format!("{active_nr_cols} of {nr_cols} columns")
                } else {
                    format!("{nr_cols} columns")
                };
                format!("{rows_text} × {cols_text}")
            } else {
                format!("{nr_rows} rows × {nr_cols} columns")
            };
            ui.add_space(4.0);
            if let Some(error) = query_error.as_deref() {
                ui.label(
                    RichText::new(format!("Query error: {error}")).color(summary_color),
                );
            } else {
                ui.label(RichText::new(summary).color(summary_color));
            }
            ui.add_space(4.0);

            let spacing_x = ui.spacing().item_spacing.x;
            let mut column_mins: Vec<f32> = header_min_widths
                .iter()
                .map(|min_width| min_width + cell_padding_x * 2.0)
                .collect();
            let spacing_total = spacing_x * (active_nr_cols.saturating_sub(1) as f32);
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

            let table = table.header(header_height, |mut header| {
                for head in &active_cols {
                    header.col(|ui| {
                        let desired_size = egui::vec2(ui.available_width(), header_height);
                        let (rect, response) = ui.allocate_exact_size(
                            desired_size,
                            egui::Sense::hover(),
                        );
                        let mut header_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(rect)
                                .layout(Layout::top_down(Align::Min)),
                        );
                        header_ui.add_space(header_pad_y);
                        header_ui.add_space(cell_padding_x);
                        header_ui.label(
                            RichText::new(head.to_string())
                                .color(header_text)
                                .font(header_font.clone())
                                .strong(),
                        );
                        let _ = response;
                    });
                }
            });
            table.body(|body| {
                body.rows(row_height, display_rows, |mut row| {
                    let row_index = row.index();
                    let is_selected = next_selection_state.row == Some(row_index);
                    row.set_selected(is_selected);
                    for col in &active_cols {
                        row.col(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), row_height),
                                Layout::left_to_right(Align::Min),
                                |ui| {
                                    ui.add_space(cell_padding_x);
                                    if let Ok(column) = &active_df.column(col.as_str()) {
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

            if let Some(selected_row) = next_selection_state.row {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("Selected row {selected_row}"))
                        .color(summary_color),
                );
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    for col_name in &active_cols {
                        if let Ok(column) = active_df.column(col_name.as_str()) {
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
            } else if query_active && display_rows == 0 {
                ui.add_space(6.0);
                ui.label(RichText::new("No rows returned").color(summary_color));
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
struct DataframeFilterState {
    query: String,
}

#[derive(Clone, Default, PartialEq)]
struct DataframeSelectionState {
    row: Option<usize>,
}

fn build_sql_query(raw: &str) -> String {
    raw.trim().to_string()
}

fn is_default_query(query: &str) -> bool {
    query.trim().eq_ignore_ascii_case(DEFAULT_QUERY)
}

const DEFAULT_QUERY: &str = "select * from self";
