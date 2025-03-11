#!/usr/bin/env watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.31"
//! egui_extras = "0.31.1"
//! polars = "0.46.0"
//! parking_lot = "0.12.3"
//! tribles = "0.5.1"
//! ```

use polars::prelude::*;
use GORBIE::widgets::auto_spawn;
use GORBIE::{md, notebook, state, view, Notebook, Card, CardCtx};
use egui::{FontId, RichText};
use egui_extras::TableBuilder;
use egui_extras::Column;
use polars::prelude::DataFrame;
use std::sync::Arc;
use parking_lot::RwLock;
use tribles::prelude::Id;

pub struct DataFrameView<'a> {
    df: &'a DataFrame,
    id: Option<Id>,
}

impl<'a> DataFrameView<'a> {
    pub fn new(df: &'a DataFrame) -> Self {
        Self { df, id: None }
    }

    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    pub fn show(&self, ui: &mut egui::Ui) {
        let nr_cols = self.df.width();
        let nr_rows = self.df.height();
        let cols = &self.df.get_column_names();

        TableBuilder::new(ui)
            //.id_salt(self.id)
            .columns(Column::remainder(), nr_cols)
            .striped(true)
            .header(30.0, |mut header| {
                for head in cols {
                    header.col(|ui| {
                        ui.label(egui::RichText::new(format!("{}", head)).heading().size(16.0));
                    });
                }
            })
            .body(|body| {
                body.rows(20.0, nr_rows, |mut row| {
                    for col in cols {
                        let row_index = row.index();
                        row.col(|ui| {
                            if let Ok(column) = &self.df.column(col) {
                                if let Ok(value) = column.get(row_index) {
                                    ui.label(format!("{}", value));
                                }
                            }
                        });
                    }
                });
            });
    }
}

pub fn dataframe(ui: &mut egui::Ui, df: &DataFrame) {
    let nr_cols = df.width();
    let nr_rows = df.height();
    let cols = &df.get_column_names();

    TableBuilder::new(ui)
        .id_salt(0)
        .columns(Column::remainder(), nr_cols)
        .striped(true)
        .header(30.0, |mut header| {
            for head in cols {
                header.col(|ui| {
                    ui.label(egui::RichText::new(format!("{}", head)).heading().size(16.0));
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

fn polars(nb: &mut Notebook) {
    md(
        nb,
        "# Polars
In this notebook we're going to use the `polars` crate to create a simple dataframe.");

let df = state!(nb, |ctx, value| {
    if let Some(df) = auto_spawn(ctx.ui, value, || {
        CsvReadOptions::default()
        .try_into_reader_with_file_path(Some("./assets/datasets/iris.csv".into()))
        .unwrap()
        .finish()
        .unwrap()
    }) {
        dataframe(ctx.ui, df);
    }
});

view!(nb, move |ctx| {
    if let Some(df) = &df.read().ready() {
        dataframe(ctx.ui, df);
    }
});
}

fn main() {
    notebook!(polars);
}
