#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! ```

use GORBIE::md;
use GORBIE::notebook;
use GORBIE::state;
use GORBIE::view;
use GORBIE::widgets;

#[notebook]
fn main() {
    state!(_prompt = "", move |ui, value| {
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

    view!(|_ui| {});
}
