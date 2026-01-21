#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.33"
//! ```

use GORBIE::cards::DEFAULT_CARD_PADDING;
use GORBIE::prelude::*;

#[notebook]
fn main(nb: &mut NotebookCtx) {
    let left = nb.state("left", 2_i64, |ui, value| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label("Left counter");
            ui.horizontal(|ui| {
                if ui.add(widgets::Button::new("-1")).clicked() {
                    *value -= 1;
                }
                if ui.add(widgets::Button::new("+1")).clicked() {
                    *value += 1;
                }
            });
            widgets::markdown(ui, &format!("Value: `{value}`"));
        });
    });

    let right = nb.state("right", 5_i64, |ui, value| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label("Right counter");
            ui.horizontal(|ui| {
                if ui.add(widgets::Button::new("-1")).clicked() {
                    *value -= 1;
                }
                if ui.add(widgets::Button::new("+1")).clicked() {
                    *value += 1;
                }
            });
            widgets::markdown(ui, &format!("Value: `{value}`"));
        });
    });

    nb.view(move |ui| {
        let left = left.read(ui);
        let right = right.read(ui);
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label("Combined view (reads both states together)");
            widgets::markdown(
                ui,
                &format!(
                    "Left: `{}`\nRight: `{}`\nSum: `{}`",
                    *left,
                    *right,
                    *left + *right
                ),
            );
        });
    });

    nb.view(move |ui| {
        let mut left = left.read_mut(ui);
        let mut right = right.read_mut(ui);
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label("Combined update (locks both states)");

            if ui.add(widgets::Button::new("Add 10 to both")).clicked() {
                *left += 10;
                *right += 10;
            }
            if ui.add(widgets::Button::new("Swap values")).clicked() {
                std::mem::swap(&mut *left, &mut *right);
            }

            widgets::markdown(ui, &format!("Left: `{}`\nRight: `{}`", *left, *right));
        });
    });
}
