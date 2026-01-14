#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! ```

use GORBIE::cards::{stateful_card, stateless_card, UiExt as _};
use GORBIE::md;
use GORBIE::notebook;
use GORBIE::widgets;
use GORBIE::Notebook;

#[notebook]
fn main(nb: &mut Notebook) {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;
    let _prompt = stateful_card(nb, "", move |ui, value| {
        ui.with_padding(padding, |ui| {
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

    stateless_card(nb, move |ui| {
        ui.with_padding(padding, |_ui| {});
    });
}
