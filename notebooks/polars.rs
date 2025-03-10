#!/usr/bin/env watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.31"
//! egui_extras = "0.31.1"
//! polars = "0.46.0"
//! ```

use GORBIE::{md, notebook, state, view, Notebook, Card, CardCtx};
use egui::{FontId, RichText};
use egui_extras::TableBuilder;
use egui_extras::Column;
use polars::prelude::DataFrame;

pub struct DataFrameCard {
    data: DataFrame,
}

impl DataFrameCard {
    pub fn new(data: DataFrame) -> Self {
        Self { data }
    }

    pub fn show(&self, ui: &mut egui::Ui) {
        let nr_cols = self.data.width();
        let nr_rows = self.data.height();
        let cols = &self.data.get_column_names();

        TableBuilder::new(ui)
        .columns(Column::auto(), 2)
        .striped(true)
        .resizable(true)
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.label(RichText::new("alpha").font(FontId::monospace(16.0)));
            });
            header.col(|ui| {
                ui.label(RichText::new("beta").font(FontId::monospace(16.0)));
            });
        })
        .body(|mut body| {
            body.row(30.0, |mut row| {
                row.col(|ui| {
                    ui.label("Hello");
                });
                row.col(|ui| {
                    ui.label("world");
                });
            });
        });
    }
}

impl Card for DataFrameCard {
    fn update(&mut self, ctx: &mut CardCtx) {
        self.show(ctx.ui);
    }
}

pub fn dataframe(nb: &mut Notebook) {
    let card = DataFrameCard::new(polars::prelude::df!(
        "Fruit" => ["Apple", "Apple", "Pear"],
        "Color" => ["Red", "Yellow", "Green"]).unwrap());
    nb.push_card(Box::new(card));
}

fn intro(nb: &mut Notebook) {
    md(
        nb,
        "# Polars
In this notebook we're going to use the `polars` crate to create a simple dataframe.");

    dataframe(nb);
}

fn main() {
    notebook!(intro);
}
