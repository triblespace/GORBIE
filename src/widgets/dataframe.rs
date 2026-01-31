use eframe::egui;
use egui::{Align, FontId, Layout, Margin, RichText, TextStyle};
use crate::widgets::{Column, TableBuilder};
use polars::prelude::DataFrame;

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

    let header_min_widths: Vec<f32> = cols
        .iter()
        .map(|head| {
            ui.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(head.to_string(), header_font.clone(), header_text)
                    .size()
                    .x
            })
        })
        .collect();

    let summary_color = crate::themes::blend(body_text, base_fill, 0.55);
    ui.label(RichText::new(format!("{nr_rows} rows × {nr_cols} columns")).color(summary_color));
    ui.add_space(4.0);

    egui::Frame::new()
        .fill(base_fill)
        .inner_margin(Margin::same(4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut table = TableBuilder::new(ui)
                .resizable(true)
                .cell_layout(Layout::left_to_right(Align::Min))
                .dense_rows(true)
                .header_body_gap(0.0);

            for min_width in &header_min_widths {
                table = table.column(Column::remainder().at_least(
                    min_width + cell_padding_x * 2.0,
                ));
            }

            table.header(header_height, |mut header| {
                    for head in cols {
                        header.col(|ui| {
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                ui.add_space(cell_padding_x);
                                ui.label(
                                    RichText::new(head.to_string())
                                        .color(header_text)
                                        .font(header_font.clone())
                                        .strong(),
                                );
                            });
                        });
                    }
                })
                .body(|body| {
                    body.rows(row_height, nr_rows, |mut row| {
                    let row_index = row.index();
                    for col in cols {
                        row.col(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), row_height),
                                Layout::left_to_right(Align::Min),
                                |ui| {
                                    ui.add_space(cell_padding_x);
                                    if let Ok(column) = &df.column(col) {
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
                });
            });
        });
}
