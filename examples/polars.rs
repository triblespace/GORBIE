#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = "..", features = ["polars"] }
//! egui = "0.33"
//! polars = "0.50.0"
//! parking_lot = "0.12.3"
//! ```

use polars::prelude::*;
use GORBIE::cards::with_padding;
use GORBIE::dataflow::ComputedState;
use GORBIE::md;
use GORBIE::notebook;
use GORBIE::widgets::{data_export_tiny, data_summary_tiny, dataframe, dataframe_summary, load_auto};
use GORBIE::NotebookCtx;
use egui::Margin;

#[notebook]
fn main(nb: &mut NotebookCtx) {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;
    let summary_padding = padding;
    let dataframe_padding = Margin {
        left: padding.left,
        right: padding.right,
        top: padding.top,
        bottom: 0,
    };
    nb.view(|ui| {
        md!(
            ui,
            "# Polars
In this notebook we're going to use the `polars` crate to create a simple dataframe."
        );
    });
    let df_state = nb.state(
        "dataframe",
        ComputedState::<Option<DataFrame>>::default(),
        move |ui, value| {
            with_padding(ui, summary_padding, |ui| {
                md!(ui, "*Overview*: row/column counts for quick context.");
                ui.add_space(6.0);
                let df = load_auto(ui, value, Option::is_none, || {
                    let df = CsvReadOptions::default()
                        .try_into_reader_with_file_path(Some("./assets/datasets/iris.csv".into()))
                        .unwrap()
                        .finish()
                        .unwrap();
                    Some(df)
                });
                if let Some(df) = df.as_ref() {
                    data_summary_tiny(ui, df, df);
                }
            });
        },
    );

    let view_state = nb.state(
        "dataframe_view_response",
        Ok(DataFrame::default()),
        move |ui, view_state| {
            let Some(state) = df_state.try_read(ui) else {
                return;
            };
            let Some(df) = state.value().as_ref() else {
                return;
            };
            with_padding(ui, dataframe_padding, |ui| {
                md!(ui, "*Table*: SQL query + sortable view.");
                ui.add_space(6.0);
                *view_state = dataframe(ui, df);
            });
        },
    );

    nb.view(move |ui| {
        let Some(state) = df_state.try_read(ui) else {
            return;
        };
        let Some(df) = state.value().as_ref() else {
            return;
        };
        let view_state = view_state.try_read(ui);
        with_padding(ui, summary_padding, |ui| {
            md!(ui, "*Summary*: per-column nulls and quick stats.");
            ui.add_space(6.0);
            let active_df = view_state
                .as_ref()
                .and_then(|state| state.as_ref().ok())
                .unwrap_or(df);
            dataframe_summary(ui, active_df);
        });
    });

    nb.view(move |ui| {
        let Some(state) = df_state.try_read(ui) else {
            return;
        };
        let Some(df) = state.value().as_ref() else {
            return;
        };
        let view_state = view_state.try_read(ui);
        with_padding(ui, summary_padding, |ui| {
            md!(ui, "*Export*: copy or save the dataframe as CSV.");
            ui.add_space(6.0);
            let active_df = view_state
                .as_ref()
                .and_then(|state| state.as_ref().ok())
                .unwrap_or(df);
            data_export_tiny(ui, active_df);
        });
    });
}
