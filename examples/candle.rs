#!/usr/bin/env watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.31"
//! ```

use GORBIE::{md, notebook, stateful, stateless, Notebook};

fn candle(nb: &mut Notebook) {
    md(nb,
        "# Candle
In this notebook we're going to use huggingfaces `candle` crate, to create a simple prompt based chatbot.
");

    let prompt = stateful!(nb, |ctx, prev| {
        let mut prompt = prev.unwrap_or_else(|| "".to_string());
            ctx.ui.horizontal(|ui| {
                ui.label("Prompt:");
                ui.text_edit_singleline(&mut prompt);
                if ui.button("Send").clicked() {
                    // send the prompt to the chatbot
                }
            });
            
        prompt
    });

    stateless!(nb, |ctx| {
    });
}

fn main() {
    notebook!(candle);
}