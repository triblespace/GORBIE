#!/usr/bin/env watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.31"
//! egui_extras = "0.31.1"
//! polars = "0.46.0"
//! parking_lot = "0.12.3"
//! ```

use polars::prelude::*;
use GORBIE::widgets::{load_auto, dataframe};
use GORBIE::{md, notebook, state, view, Notebook};

fn polars(nb: &mut Notebook) {
    md(
        nb,
        "# Polars
In this notebook we're going to use the `polars` crate to create a simple dataframe.");

let df = state!(nb, |ctx, value| {
    if let Some(df) = load_auto(ctx.ui, value, || {
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
