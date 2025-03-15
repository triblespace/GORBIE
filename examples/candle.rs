#!/usr/bin/env watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.31"
//! ```

use GORBIE::{md, notebook, state, view, Notebook};

fn candle(nb: &mut Notebook) {
    md(nb,
        "# Candle
In this notebook we're going to use huggingfaces `candle` crate, to create a simple prompt based chatbot.
");

    let prompt = state!(nb, "", |ctx, value| {
        ctx.ui().horizontal(|ui| {
            ui.label("Prompt:");
            ui.text_edit_singleline(value);
            if ui.button("Send").clicked() {
                // send the prompt to the chatbot
            }
        });
    });

    view!(nb, (), |ctx| {});
}

fn main() {
    notebook!(candle);
}
