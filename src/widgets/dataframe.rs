use egui_extras::{Column, TableBuilder};
use polars::prelude::DataFrame;
use eframe::egui;

pub fn dataframe(ui: &mut egui::Ui, df: &DataFrame) {
    let nr_cols = df.width();
    let nr_rows = df.height();
    let cols = &df.get_column_names();

    TableBuilder::new(ui)
        .columns(Column::remainder(), nr_cols)
        .striped(true)
        .header(30.0, |mut header| {
            for head in cols {
                header.col(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{}", head))
                            .heading()
                            .size(16.0),
                    );
                });
            }
        })
        .body(|body| {
            body.rows(20.0, nr_rows, |mut row| {
                for col in cols {
                    let row_index = row.index();
                    row.col(|ui| {
                        if let Ok(column) = &df.column(col) {
                            if let Ok(value) = column.get(row_index) {
                                ui.label(format!("{}", value));
                            }
                        }
                    });
                }
            });
        });
}
