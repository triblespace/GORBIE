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
use GORBIE::cards::with_padding;
use GORBIE::dataflow::ComputedState;
use GORBIE::md;
use GORBIE::notebook;
use GORBIE::widgets::{dataframe, load_auto};
use GORBIE::Notebook;

#[notebook]
fn main(nb: &mut Notebook) {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;
    let _df = nb.state(
        "dataframe",
        ComputedState::<Option<DataFrame>>::default(),
        move |ui, value| {
        with_padding(ui, padding, |ui| {
            md!(
                ui,
                "# Polars
In this notebook we're going to use the `polars` crate to create a simple dataframe."
            );
            let df = load_auto(ui, value, Option::is_none, || {
                let df = CsvReadOptions::default()
                    .try_into_reader_with_file_path(Some("./assets/datasets/iris.csv".into()))
                    .unwrap()
                    .finish()
                    .unwrap();
                Some(df)
            });
            if let Some(df) = df.as_ref() {
                dataframe(ui, df);
            }
        });
    });
}
