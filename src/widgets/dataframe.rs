use crate::widgets::{Button, Column, TableBuilder, TextField};
use eframe::egui;
use egui::{Align, FontId, Layout, Margin, RichText, TextStyle};
use polars::prelude::{
    AnyValue, CsvWriter, DataFrame, DataType, IntoLazy, PlSmallStr, SerWriter, Series,
    SeriesMethods, SortMultipleOptions,
};
use polars::sql::SQLContext;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;

pub fn data_summary_tiny(ui: &mut egui::Ui, active_df: &DataFrame, total_df: &DataFrame) {
    let visuals = ui.visuals();
    let base_fill = visuals.panel_fill;
    let body_text = visuals.widgets.inactive.fg_stroke.color;
    let base_mono = ui
        .style()
        .text_styles
        .get(&TextStyle::Monospace)
        .map(|font| font.size)
        .unwrap_or(14.0);
    let small_font = FontId::monospace((base_mono - 2.0).max(9.0));
    let summary_color = crate::themes::blend(body_text, base_fill, 0.55);

    let rows = active_df.height();
    let cols = active_df.width();
    let total_rows = total_df.height();
    let total_cols = total_df.width();
    let totals = if rows != total_rows || cols != total_cols {
        Some((total_rows, total_cols))
    } else {
        None
    };
    let summary = format_overview(rows, cols, totals);
    ui.label(
        RichText::new(summary)
            .font(small_font)
            .color(summary_color),
    );
}

pub fn data_export_tiny(ui: &mut egui::Ui, df: &DataFrame) {
    let visuals = ui.visuals();
    let base_fill = visuals.panel_fill;
    let body_text = visuals.widgets.inactive.fg_stroke.color;
    let base_mono = ui
        .style()
        .text_styles
        .get(&TextStyle::Monospace)
        .map(|font| font.size)
        .unwrap_or(14.0);
    let small_font = FontId::monospace((base_mono - 2.0).max(9.0));
    let summary_color = crate::themes::blend(body_text, base_fill, 0.55);

    let export_state_id = ui.id().with("dataframe_export_state");
    let mut state =
        ui.data_mut(|data| data.get_temp::<DataExportState>(export_state_id))
            .unwrap_or_default();

    let mut copy_clicked = false;
    let mut save_clicked = false;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Export")
                .font(small_font.clone())
                .color(summary_color),
        );
        ui.add(TextField::singleline(&mut state.path));
        copy_clicked = ui.add(Button::new("copy csv")).clicked();
        save_clicked = ui.add(Button::new("save")).clicked();
    });

    if copy_clicked || save_clicked {
        let mut buffer = Vec::new();
        let mut export_df = df.clone();
        if let Err(err) = CsvWriter::new(&mut buffer).finish(&mut export_df) {
            state.status = Some(format!("Export failed: {err}"));
        } else if copy_clicked {
            let text = String::from_utf8_lossy(&buffer).to_string();
            ui.ctx().copy_text(text);
            state.status = Some(format!("Copied {} rows", df.height()));
        } else if save_clicked {
            let path = if state.path.trim().is_empty() {
                "data.csv".to_string()
            } else {
                state.path.trim().to_string()
            };
            match fs::write(&path, &buffer) {
                Ok(()) => {
                    state.path = path.clone();
                    state.status = Some(format!("Saved {path}"));
                }
                Err(err) => {
                    state.status = Some(format!("Save failed: {err}"));
                }
            }
        }
    }

    if let Some(status) = state.status.as_ref() {
        ui.add_space(4.0);
        ui.label(
            RichText::new(status)
                .font(small_font.clone())
                .color(summary_color),
        );
    }

    ui.data_mut(|data| data.insert_temp(export_state_id, state));
}

#[derive(Clone)]
struct DataExportState {
    path: String,
    status: Option<String>,
}

impl Default for DataExportState {
    fn default() -> Self {
        Self {
            path: "data.csv".to_string(),
            status: None,
        }
    }
}

fn format_overview(rows: usize, cols: usize, totals: Option<(usize, usize)>) -> String {
    if let Some((total_rows, total_cols)) = totals {
        let rows_text = if rows != total_rows {
            format!("{rows} of {total_rows} rows")
        } else {
            format!("{rows} rows")
        };
        let cols_text = if cols != total_cols {
            format!("{cols} of {total_cols} columns")
        } else {
            format!("{cols} columns")
        };
        format!("{rows_text} × {cols_text}")
    } else {
        format!("{rows} rows × {cols} columns")
    }
}

pub fn dataframe_summary(ui: &mut egui::Ui, df: &DataFrame) {
    let nr_cols = df.width();
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
    let base_font = FontId::monospace(base_mono.max(10.0));
    let small_font = FontId::monospace((base_mono - 2.0).max(9.0));

    let summary_color = crate::themes::blend(body_text, base_fill, 0.55);
    let stat_color = crate::themes::blend(body_text, base_fill, 0.25);

    let summaries = summarize_dataframe(df);
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(&base_font)).max(10.0);
    let header_height = ui.fonts_mut(|fonts| fonts.row_height(&small_font)).max(8.0) + 2.0;
    let cell_padding_x = 6.0;

    let label_width = |text: &str, font: &FontId| {
        ui.fonts_mut(|fonts| fonts.layout_no_wrap(text.to_string(), font.clone(), header_text))
            .size()
            .x
    };
    let max_width = |values: &[String], font: &FontId| {
        values
            .iter()
            .map(|text| label_width(text, font))
            .fold(0.0, f32::max)
    };
    let name_width = max_width(
        &summaries.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        &base_font,
    )
    .max(label_width("column", &small_font));
    let dtype_width = max_width(
        &summaries.iter().map(|s| s.dtype.clone()).collect::<Vec<_>>(),
        &small_font,
    )
    .max(label_width("type", &small_font));
    let nulls_width = max_width(
        &summaries.iter().map(|s| s.nulls.clone()).collect::<Vec<_>>(),
        &small_font,
    )
    .max(label_width("nulls", &small_font));

    let column_width = name_width + cell_padding_x * 2.0;
    let type_width = dtype_width + cell_padding_x * 2.0;
    let nulls_width = nulls_width + cell_padding_x * 2.0;

    egui::Frame::new()
        .fill(base_fill)
        .inner_margin(Margin {
            left: 4,
            right: 4,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut table = TableBuilder::new(ui)
                .resizable(false)
                .cell_layout(Layout::left_to_right(Align::Min))
                .dense_rows(true)
                .header_body_gap(0.0)
                .min_scrolled_height(0.0)
                .vscroll(false)
                .sense(egui::Sense::hover());

            table = table
                .column(Column::exact(column_width).clip(true))
                .column(Column::exact(type_width).clip(true))
                .column(Column::exact(nulls_width).clip(true))
                .column(Column::remainder().clip(true));

            let table = table.header(header_height, |mut header| {
                header.col(|ui| {
                    ui.add_space(cell_padding_x);
                    ui.label(
                        RichText::new("column")
                            .font(small_font.clone())
                            .color(summary_color),
                    );
                });
                header.col(|ui| {
                    ui.add_space(cell_padding_x);
                    ui.label(
                        RichText::new("type")
                            .font(small_font.clone())
                            .color(summary_color),
                    );
                });
                header.col(|ui| {
                    ui.add_space(cell_padding_x);
                    ui.label(
                        RichText::new("nulls")
                            .font(small_font.clone())
                            .color(summary_color),
                    );
                });
                header.col(|ui| {
                    ui.add_space(cell_padding_x);
                    ui.label(
                        RichText::new("stats")
                            .font(small_font.clone())
                            .color(summary_color),
                    );
                });
            });

            table.body(|body| {
                body.rows(row_height, summaries.len(), |mut row| {
                    let summary = &summaries[row.index()];
                    row.col(|ui| {
                        ui.add_space(cell_padding_x);
                        ui.label(
                            RichText::new(summary.name.clone())
                                .font(base_font.clone())
                                .color(header_text),
                        );
                    });
                    row.col(|ui| {
                        ui.add_space(cell_padding_x);
                        ui.label(
                            RichText::new(summary.dtype.clone())
                                .font(small_font.clone())
                                .color(summary_color),
                        );
                    });
                    row.col(|ui| {
                        ui.add_space(cell_padding_x);
                        ui.label(
                            RichText::new(summary.nulls.clone())
                                .font(small_font.clone())
                                .color(summary_color),
                        );
                    });
                    row.col(|ui| {
                        ui.add_space(cell_padding_x);
                        ui.add(
                            egui::Label::new(
                                RichText::new(summary.stats.clone())
                                    .font(base_font.clone())
                                    .color(stat_color),
                            )
                            .truncate(),
                        );
                    });
                });
            });
        });
}

pub fn dataframe(ui: &mut egui::Ui, df: &DataFrame) -> Result<DataFrame, String> {
    let filter_state_id = ui.id().with("dataframe_filter_state");
    let mut filter_state =
        ui.data_mut(|data| data.get_temp::<DataframeFilterState>(filter_state_id))
            .unwrap_or_default();
    let selection_state_id = ui.id().with("dataframe_selection_state");
    let mut selection_state =
        ui.data_mut(|data| data.get_temp::<DataframeSelectionState>(selection_state_id))
            .unwrap_or_default();

    let prev_filter_state = filter_state.clone();
    let prev_selection_state = selection_state.clone();

    let result = dataframe_core(
        ui,
        df,
        &mut filter_state.query,
        &mut selection_state.row,
    );

    let _ = (filter_state.query.clone(), selection_state.row);

    if filter_state != prev_filter_state {
        ui.data_mut(|data| data.insert_temp(filter_state_id, filter_state));
    }
    if selection_state != prev_selection_state {
        ui.data_mut(|data| data.insert_temp(selection_state_id, selection_state));
    }

    let _ = ();
    result
}

fn dataframe_core(
    ui: &mut egui::Ui,
    df: &DataFrame,
    query: &mut String,
    selection: &mut Option<usize>,
) -> Result<DataFrame, String> {
    let nr_cols = df.width();
    if nr_cols == 0 {
        ui.label("Empty dataframe");
        return Ok(df.clone());
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
    let base_font = FontId::monospace(base_mono.max(10.0));
    let small_font = FontId::monospace((base_mono - 2.0).max(9.0));
    let body_height = ui.fonts_mut(|fonts| fonts.row_height(&base_font));
    let header_text_height = ui.fonts_mut(|fonts| fonts.row_height(&base_font));
    let type_text_height = ui.fonts_mut(|fonts| fonts.row_height(&small_font));
    let row_height = body_height.max(10.0);
    let cell_padding_x = 4.0;
    let header_pad_y = 2.0;
    let header_gap = 2.0;
    let header_viz_height = (row_height * 0.9).max(12.0);
    let header_height = header_pad_y * 2.0
        + header_text_height
        + header_gap
        + type_text_height
        + header_gap
        + header_viz_height;

    let summary_color = crate::themes::blend(body_text, base_fill, 0.55);
    let stat_color = crate::themes::blend(body_text, base_fill, 0.3);
    let null_color = crate::themes::blend(body_text, base_fill, 0.4);

    let mut sql_result: Result<DataFrame, String> = Ok(df.clone());

    egui::Frame::new()
        .fill(base_fill)
        .inner_margin(Margin {
            left: 4,
            right: 4,
            top: 4,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut next_query = query.clone();
            let mut next_selection = *selection;
            let text_width = ui.available_width().max(120.0);
            ui.spacing_mut().text_edit_width = text_width;
            ui.spacing_mut().interact_size.y = row_height;
            let query_response = ui
                .push_id("dataframe_filter_input", |ui| {
                    ui.add(TextField::multiline(&mut next_query).rows(1))
                })
                .inner;
            if !query_response.has_focus() && next_query.trim().is_empty() {
                next_query = DEFAULT_QUERY.to_string();
            }

            let query_text = next_query.trim();
            let eval_query = if query_text.is_empty() {
                DEFAULT_QUERY
            } else {
                query_text
            };
            let mut sql_df: Option<DataFrame> = None;
            let sql = build_sql_query(eval_query);
            let mut ctx = SQLContext::new();
            ctx.register("self", df.clone().lazy());
            match ctx.execute(&sql).and_then(|lf| lf.collect()) {
                Ok(result) => {
                    sql_df = Some(result.clone());
                    sql_result = Ok(result);
                }
                Err(err) => {
                    sql_result = Err(err.to_string());
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
            let previews = load_column_previews(ui, active_df, eval_query, &active_cols);

            let display_rows = active_nr_rows;
            if let Some(selected_row) = next_selection {
                if selected_row >= active_nr_rows {
                    next_selection = None;
                }
            }

            let header_min_widths: Vec<f32> = active_cols
                .iter()
                .map(|head| {
                    ui.fonts_mut(|fonts| {
                        fonts
                            .layout_no_wrap(head.to_string(), base_font.clone(), header_text)
                            .size()
                            .x
                    })
                })
                .collect();

            ui.add_space(4.0);
            let mut wrote_status = false;
            if let Err(error) = sql_result.as_ref() {
                ui.label(
                    RichText::new(format!("Query error: {error}"))
                        .font(small_font.clone())
                        .color(summary_color),
                );
                wrote_status = true;
            }

            if let Some(selected_row) = next_selection {
                if wrote_status {
                    ui.add_space(6.0);
                } else {
                    ui.add_space(4.0);
                }
                ui.label(
                    RichText::new(format!("Selected row {}", selected_row + 1))
                        .font(small_font.clone())
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
                                        .font(base_font.clone()),
                                );
                            }
                        }
                    }
                });
            } else if display_rows == 0 {
                if wrote_status {
                    ui.add_space(6.0);
                } else {
                    ui.add_space(4.0);
                }
                ui.label(
                    RichText::new("No rows returned")
                        .font(small_font.clone())
                        .color(summary_color),
                );
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
                .min_scrolled_height(0.0)
                .sense(egui::Sense::click_and_drag());
            for min_width in &column_mins {
                table = table.column(Column::remainder().at_least(*min_width).clip(true));
            }

            let table = table.header(header_height, |mut header| {
                for (index, head) in active_cols.iter().enumerate() {
                    let preview = previews.get(index);
                    header.col(|ui| {
                        let desired_size = egui::vec2(ui.available_width(), header_height);
                        let (rect, _response) = ui.allocate_exact_size(
                            desired_size,
                            egui::Sense::hover(),
                        );
                        let content_rect =
                            rect.shrink2(egui::vec2(cell_padding_x, 0.0));
                        let mut header_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(content_rect)
                                .layout(Layout::top_down(Align::Min)),
                        );
                        header_ui.add_space(header_pad_y);
                        header_ui.label(
                            RichText::new(head.to_string())
                                .color(header_text)
                                .font(base_font.clone())
                                .strong(),
                        );
                        header_ui.add_space(header_gap);
                        if let Some(preview) = preview {
                            header_ui.label(
                                RichText::new(preview.dtype.clone())
                                    .color(summary_color)
                                    .font(small_font.clone()),
                            );
                            header_ui.add_space(header_gap);
                            let (viz_rect, _) = header_ui.allocate_exact_size(
                                egui::vec2(header_ui.available_width(), header_viz_height),
                                egui::Sense::hover(),
                            );
                            paint_column_preview(
                                header_ui.painter(),
                                viz_rect,
                                preview,
                                stat_color,
                            );
                        }
                    });
                }
            });
            table.body(|body| {
                body.rows(row_height, display_rows, |mut row| {
                    let row_index = row.index();
                    let is_selected = next_selection == Some(row_index);
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
                                            let (text, is_null) = format_cell_value(value);
                                            let color = if is_null { null_color } else { body_text };
                                            ui.label(
                                                RichText::new(text)
                                                    .color(color)
                                                    .font(base_font.clone()),
                                            );
                                        }
                                    }
                                },
                            );
                        });
                    }
                    if row.response().clicked() {
                        next_selection = if is_selected {
                            None
                        } else {
                            Some(row_index)
                        };
                    }
                });
            });

            *query = next_query;
            *selection = next_selection;
        });

    sql_result
}

#[derive(Clone, Default, PartialEq)]
struct DataframeFilterState {
    query: String,
}

#[derive(Clone, PartialEq)]
struct DataframePreviewKey {
    query: String,
    rows: usize,
    cols: usize,
    schema: Vec<(String, String)>,
}

#[derive(Clone, Default)]
struct DataframePreviewCache {
    key: Option<DataframePreviewKey>,
    previews: Vec<ColumnPreview>,
}

#[derive(Clone)]
struct ColumnPreview {
    dtype: String,
    kind: PreviewKind,
    bars: Vec<f32>,
}

#[derive(Clone, Copy)]
enum PreviewKind {
    Numeric,
    Categorical,
}

fn load_column_previews(
    ui: &mut egui::Ui,
    df: &DataFrame,
    query: &str,
    columns: &[String],
) -> Vec<ColumnPreview> {
    let preview_id = ui.id().with("dataframe_preview_cache");
    let mut cache =
        ui.data_mut(|data| data.get_temp::<DataframePreviewCache>(preview_id))
            .unwrap_or_default();
    let schema = df
        .get_columns()
        .iter()
        .map(|column| {
            let series = column.as_materialized_series();
            (
                series.name().to_string(),
                format!("{:?}", series.dtype()).to_lowercase(),
            )
        })
        .collect::<Vec<_>>();
    let key = DataframePreviewKey {
        query: query.to_string(),
        rows: df.height(),
        cols: df.width(),
        schema,
    };

    if cache.key.as_ref() != Some(&key) {
        cache.previews = columns
            .iter()
            .map(|name| {
                df.column(name.as_str())
                    .map(|column| compute_preview(column.as_materialized_series()))
                    .unwrap_or_else(|_| ColumnPreview {
                        dtype: "unknown".to_string(),
                        kind: PreviewKind::Categorical,
                        bars: Vec::new(),
                    })
            })
            .collect();
        cache.key = Some(key);
        ui.data_mut(|data| data.insert_temp(preview_id, cache.clone()));
    }

    cache.previews
}

fn compute_preview(series: &Series) -> ColumnPreview {
    let dtype = format!("{:?}", series.dtype()).to_lowercase();
    if is_numeric_dtype(series.dtype()) {
        ColumnPreview {
            dtype,
            kind: PreviewKind::Numeric,
            bars: histogram_numeric(series, 12),
        }
    } else if matches!(series.dtype(), DataType::Boolean) {
        ColumnPreview {
            dtype,
            kind: PreviewKind::Categorical,
            bars: histogram_boolean(series),
        }
    } else {
        ColumnPreview {
            dtype,
            kind: PreviewKind::Categorical,
            bars: histogram_categorical(series, 6),
        }
    }
}

fn is_numeric_dtype(dtype: &DataType) -> bool {
    matches!(
        dtype,
        DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Float32
            | DataType::Float64
            | DataType::Date
            | DataType::Datetime(_, _)
            | DataType::Duration(_)
            | DataType::Time
    )
}

fn histogram_numeric(series: &Series, bins: usize) -> Vec<f32> {
    let bins = bins.max(2);
    let casted = match series.cast(&DataType::Float64) {
        Ok(series) => series,
        Err(_) => return vec![0.0; bins],
    };
    let values = match casted.f64() {
        Ok(values) => values,
        Err(_) => return vec![0.0; bins],
    };

    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut sample = Vec::new();
    for value in values.into_iter().flatten() {
        if value.is_finite() {
            min = min.min(value);
            max = max.max(value);
            sample.push(value);
        }
    }
    if sample.is_empty() {
        return vec![0.0; bins];
    }
    if (max - min).abs() < f64::EPSILON {
        let mut out = vec![0.0; bins];
        out[bins / 2] = 1.0;
        return out;
    }

    let stride = (sample.len() / 2048).max(1);
    let mut counts = vec![0u32; bins];
    for (idx, value) in sample.iter().enumerate() {
        if idx % stride != 0 {
            continue;
        }
        let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
        let bin = (t * bins as f64).floor().min((bins - 1) as f64) as usize;
        counts[bin] += 1;
    }

    normalize_counts_max(counts)
}

fn histogram_boolean(series: &Series) -> Vec<f32> {
    let bools = match series.bool() {
        Ok(values) => values,
        Err(_) => return vec![0.0, 0.0],
    };
    let mut counts = [0u32; 2];
    for value in bools.into_iter().flatten() {
        if value {
            counts[1] += 1;
        } else {
            counts[0] += 1;
        }
    }
    normalize_counts_total(counts.to_vec())
}

fn histogram_categorical(series: &Series, max_bars: usize) -> Vec<f32> {
    let max_bars = max_bars.max(2);
    let top_values = top_value_samples(series, max_bars);
    if top_values.is_empty() {
        return vec![0.0; max_bars];
    }
    let counts: Vec<u32> = top_values.into_iter().map(|(_, count)| count).collect();
    normalize_counts_total(counts)
}

fn normalize_counts_max(counts: Vec<u32>) -> Vec<f32> {
    let max = counts.iter().copied().max().unwrap_or(0).max(1) as f32;
    counts
        .into_iter()
        .map(|count| (count as f32) / max)
        .collect()
}

fn normalize_counts_total(counts: Vec<u32>) -> Vec<f32> {
    let total: f32 = counts.iter().map(|count| *count as f32).sum();
    let total = total.max(1.0);
    counts
        .into_iter()
        .map(|count| (count as f32) / total)
        .collect()
}

fn paint_column_preview(
    painter: &egui::Painter,
    rect: egui::Rect,
    preview: &ColumnPreview,
    color: egui::Color32,
) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 || preview.bars.is_empty() {
        return;
    }
    let bar_count = preview.bars.len().max(1);
    let bar_width = rect.width() / bar_count as f32;
    if matches!(preview.kind, PreviewKind::Categorical) {
        let mut x = rect.min.x;
        let gap = 1.0;
        for value in &preview.bars {
            let width = rect.width() * value.clamp(0.0, 1.0);
            if width <= 0.0 {
                continue;
            }
            let seg_rect = egui::Rect::from_min_max(
                egui::pos2(x, rect.min.y),
                egui::pos2((x + width).min(rect.max.x), rect.max.y),
            );
            painter.rect_filled(seg_rect, 0.0, color);
            x = (seg_rect.right() + gap).min(rect.max.x);
            if x >= rect.max.x {
                break;
            }
        }
        let baseline = egui::Rect::from_min_max(
            egui::pos2(rect.min.x, rect.max.y - 1.0),
            egui::pos2(rect.max.x, rect.max.y),
        );
        painter.rect_filled(baseline, 0.0, color);
        return;
    }

    for (i, value) in preview.bars.iter().enumerate() {
        let height = rect.height() * value.clamp(0.0, 1.0);
        let x0 = rect.min.x + i as f32 * bar_width;
        let x1 = x0 + bar_width;
        let y0 = rect.max.y - height;
        let bar_rect = egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, rect.max.y));
        painter.rect_filled(bar_rect, 0.0, color);
    }
}

#[derive(Clone, Default, PartialEq)]
struct DataframeSelectionState {
    row: Option<usize>,
}



fn build_sql_query(raw: &str) -> String {
    raw.trim().to_string()
}

const DEFAULT_QUERY: &str = "select * from self";

fn format_cell_value(value: AnyValue<'_>) -> (String, bool) {
    match value {
        AnyValue::Null => ("∅".to_string(), true),
        AnyValue::Float32(value) if value.is_nan() => ("NaN".to_string(), true),
        AnyValue::Float64(value) if value.is_nan() => ("NaN".to_string(), true),
        other => (format!("{other}"), false),
    }
}

#[derive(Clone)]
struct ColumnSummary {
    name: String,
    dtype: String,
    nulls: String,
    stats: String,
}

fn summarize_dataframe(df: &DataFrame) -> Vec<ColumnSummary> {
    let row_count = df.height();
    df.get_column_names()
        .iter()
        .filter_map(|name| df.column(name).ok())
        .map(|column| summarize_series(column.as_materialized_series(), row_count))
        .collect()
}

fn summarize_series(series: &Series, row_count: usize) -> ColumnSummary {
    let dtype = format!("{:?}", series.dtype()).to_lowercase();
    let nulls = series.null_count();
    let null_pct = if row_count > 0 {
        (nulls as f32 / row_count as f32) * 100.0
    } else {
        0.0
    };
    let nulls_text = format!("{nulls} ({null_pct:.1}%)");

    let stats = if matches!(series.dtype(), DataType::Boolean) {
        summarize_boolean(series)
    } else if is_numeric_dtype(series.dtype()) {
        summarize_numeric(series)
    } else {
        summarize_categorical(series)
    };

    ColumnSummary {
        name: series.name().to_string(),
        dtype,
        nulls: nulls_text,
        stats,
    }
}

fn summarize_numeric(series: &Series) -> String {
    let min = series.min::<f64>().ok().flatten();
    let max = series.max::<f64>().ok().flatten();
    let mean = series.mean();
    format!(
        "min {} · mean {} · max {}",
        format_float_option(min),
        format_float_option(mean),
        format_float_option(max)
    )
}

fn summarize_boolean(series: &Series) -> String {
    let mut true_count = 0u32;
    let mut false_count = 0u32;
    if let Ok(values) = series.bool() {
        for value in values.into_iter().flatten() {
            if value {
                true_count += 1;
            } else {
                false_count += 1;
            }
        }
    }
    format!("true {true_count} · false {false_count}")
}

fn summarize_categorical(series: &Series) -> String {
    let unique = series.n_unique().ok();
    let top_values = top_value_samples(series, 3);
    let mut parts = Vec::new();
    if let Some(unique) = unique {
        parts.push(format!("unique {unique}"));
    }
    if !top_values.is_empty() {
        let top = top_values
            .into_iter()
            .map(|(value, count)| format!("{value} ({count})"))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("top {top}"));
    }
    if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join(" · ")
    }
}

fn top_value_samples(series: &Series, limit: usize) -> Vec<(String, u32)> {
    if limit == 0 {
        return Vec::new();
    }
    let count_name = if series.name().as_str() == "count" {
        PlSmallStr::from_static("__count")
    } else {
        PlSmallStr::from_static("count")
    };
    if let Ok(mut counts_df) = series.value_counts(false, false, count_name.clone(), false) {
        let value_name = series.name().clone();
        let sort_options =
            SortMultipleOptions::default().with_order_descending_multi([true, false]);
        if let Ok(sorted) = counts_df.sort([count_name.clone(), value_name.clone()], sort_options)
        {
            counts_df = sorted;
        }
        let values = match counts_df.column(value_name.as_str()) {
            Ok(values) => values.as_materialized_series(),
            Err(_) => return Vec::new(),
        };
        let counts = match counts_df.column(count_name.as_str()) {
            Ok(counts) => counts.as_materialized_series(),
            Err(_) => return Vec::new(),
        };
        let mut items = Vec::with_capacity(limit);
        for (value, count) in values.iter().zip(counts.iter()) {
            let value = match value {
                AnyValue::Null => continue,
                other => other,
            };
            let count = match count {
                AnyValue::UInt32(value) => value,
                AnyValue::UInt64(value) => value.min(u32::MAX as u64) as u32,
                AnyValue::Int32(value) if value >= 0 => value as u32,
                AnyValue::Int64(value) if value >= 0 => value.min(u32::MAX as i64) as u32,
                AnyValue::UInt16(value) => value as u32,
                AnyValue::UInt8(value) => value as u32,
                AnyValue::Int16(value) if value >= 0 => value as u32,
                AnyValue::Int8(value) if value >= 0 => value as u32,
                _ => continue,
            };
            items.push((format!("{value}"), count));
            if items.len() >= limit {
                break;
            }
        }
        return items;
    }
    let mut counts: HashMap<String, u32> = HashMap::new();
    let iter_series = if series.chunks().len() == 1 {
        Cow::Borrowed(series)
    } else {
        Cow::Owned(series.rechunk())
    };
    let total = iter_series.len();
    let stride = (total / 2048).max(1);
    for (idx, value) in iter_series.iter().enumerate() {
        if idx % stride != 0 {
            continue;
        }
        let value = match value {
            AnyValue::Null => continue,
            other => other,
        };
        let key = format!("{value}");
        *counts.entry(key).or_insert(0) += 1;
    }
    let mut items: Vec<(String, u32)> = counts.into_iter().collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(limit);
    items
}

fn format_float_option(value: Option<f64>) -> String {
    value.map(format_float_compact).unwrap_or_else(|| "—".to_string())
}

fn format_float_compact(value: f64) -> String {
    if !value.is_finite() {
        return "NaN".to_string();
    }
    let abs = value.abs();
    let raw = if abs >= 1_000_000.0 || (abs > 0.0 && abs < 0.001) {
        format!("{value:.2e}")
    } else if abs >= 10_000.0 {
        format!("{value:.0}")
    } else if abs >= 1_000.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.3}")
    };
    trim_float(raw)
}

fn trim_float(raw: String) -> String {
    if !raw.contains('.') {
        return raw;
    }
    let trimmed = raw.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}
