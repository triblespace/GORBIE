use crate::widgets::{Column, TableBuilder, TextField};
use eframe::egui;
use egui::{Align, FontId, Layout, Margin, RichText, TextStyle};
use polars::prelude::{AnyValue, DataFrame, DataType, IntoLazy, Series};
use polars::sql::SQLContext;
use std::collections::HashMap;

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
    let type_font = FontId::monospace((base_mono - 4.0).max(8.0));
    let body_height = ui.fonts_mut(|fonts| fonts.row_height(&body_font));
    let header_text_height = ui.fonts_mut(|fonts| fonts.row_height(&header_font));
    let type_text_height = ui.fonts_mut(|fonts| fonts.row_height(&type_font));
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
    let filter_state_id = ui.id().with("dataframe_filter_state");
    let filter_state =
        ui.data_mut(|data| data.get_temp::<DataframeFilterState>(filter_state_id))
            .unwrap_or_default();
    let selection_state_id = ui.id().with("dataframe_selection_state");
    let selection_state =
        ui.data_mut(|data| data.get_temp::<DataframeSelectionState>(selection_state_id))
            .unwrap_or_default();

    let summary_color = crate::themes::blend(body_text, base_fill, 0.55);
    let stat_color = crate::themes::blend(body_text, base_fill, 0.3);
    let null_color = crate::themes::blend(body_text, base_fill, 0.4);

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
            let previews = load_column_previews(ui, active_df, query, &active_cols);

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

            if let Some(selected_row) = next_selection_state.row {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("Selected row {}", selected_row + 1))
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
                                .font(header_font.clone())
                                .strong(),
                        );
                        header_ui.add_space(header_gap);
                        if let Some(preview) = preview {
                            header_ui.label(
                                RichText::new(preview.dtype.clone())
                                    .color(summary_color)
                                    .font(type_font.clone()),
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
                                            let (text, is_null) = format_cell_value(value);
                                            let color = if is_null { null_color } else { body_text };
                                            ui.label(
                                                RichText::new(text)
                                                    .color(color)
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
    let mut counts: HashMap<String, u32> = HashMap::new();
    let mut seen = 0usize;
    let sample_limit = 2048usize;
    let mut push_value = |value: String| {
        *counts.entry(value).or_insert(0) += 1;
    };

    if let Some(strings) = series.try_str() {
        for value in strings.into_iter().flatten() {
            push_value(value.to_string());
            seen += 1;
            if seen >= sample_limit {
                break;
            }
        }
    } else {
        for value in series.iter() {
            push_value(format!("{value}"));
            seen += 1;
            if seen >= sample_limit {
                break;
            }
        }
    }

    if counts.is_empty() {
        return vec![0.0; max_bars];
    }

    let mut items: Vec<(String, u32)> = counts.into_iter().collect();
    items.sort_by(|a, b| b.1.cmp(&a.1));
    items.truncate(max_bars);
    let counts: Vec<u32> = items.into_iter().map(|(_, count)| count).collect();
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

fn is_default_query(query: &str) -> bool {
    query.trim().eq_ignore_ascii_case(DEFAULT_QUERY)
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
