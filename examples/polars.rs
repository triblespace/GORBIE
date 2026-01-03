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
use GORBIE::dataflow::ComputedState;
use GORBIE::md;
use GORBIE::notebook;
use GORBIE::state;
use GORBIE::view;
use GORBIE::widgets::dataframe;
use GORBIE::widgets::load_auto;

#[notebook]
fn main() {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;
    state!(df = ComputedState::default(), move |ui, value| {
        ui.with_padding(padding, |ui| {
            md!(
                ui,
                "# Polars
In this notebook we're going to use the `polars` crate to create a simple dataframe."
            );
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
    });

    view!(move |ui| {
        ui.with_padding(padding, |ui| {
            let Some(df) = ui.try_ready(df) else {
                return;
            };
            dataframe(ui, &df);
        });
    });
}
