#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.33"
//! ```

use GORBIE::cards::with_padding;
use GORBIE::md;
use GORBIE::notebook;
use GORBIE::widgets;
use GORBIE::Notebook;

#[notebook]
fn main(nb: &mut Notebook) {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;
    let _prompt = nb.state("prompt", "", move |ui, value| {
        with_padding(ui, padding, |ui| {
            md!(ui,
            "# Candle
In this notebook we're going to use huggingfaces `candle` crate, to create a simple prompt based chatbot.
");

            ui.horizontal(|ui| {
                ui.label("Prompt:");
                ui.add(widgets::TextField::singleline(value));
                if ui.button("Send").clicked() {
                    // send the prompt to the chatbot
                }
            });
        });
    });

    nb.view(move |ui| {
        with_padding(ui, padding, |_ui| {});
    });
}
