#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! egui_extras = "0.32"
//! polars = "0.50.0"
//! parking_lot = "0.12.3"
//! ```

use polars::prelude::*;
use GORBIE::widgets::{dataframe, load_auto};
use GORBIE::{md, notebook, state, view, Notebook};

fn polars(nb: &mut Notebook) {
    md(
        nb,
        "# Polars
In this notebook we're going to use the `polars` crate to create a simple dataframe.",
    );

    let df = state!(nb, (), |ui, value| {
        if let Some(df) = load_auto(ui, value, || {
            CsvReadOptions::default()
                .try_into_reader_with_file_path(Some("./assets/datasets/iris.csv".into()))
                .unwrap()
                .finish()
                .unwrap()
        }) {
            dataframe(ui, df);
        }
    });

    view!(nb, (df), move |ui| {
        if let Some(df) = &df.read().ready() {
            dataframe(ui, df);
        }
    });
}

fn main() {
    notebook!(polars);
}
