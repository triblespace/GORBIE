#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = "../.." }
//! egui = "0.33"
//! ```

use egui::RichText;
use GORBIE::cards::{with_padding, DEFAULT_CARD_PADDING};
use GORBIE::prelude::*;

#[derive(Default)]
struct QuizState {
    add_result: Option<i32>,
    subtract_amount: i32,
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "# Hello, state\n\
                 A **variable** is a named box that holds a value.\n\n\
                 - The *name* tells us which box we mean.\n\
                 - The *value* is what is inside the box.\n\
                 - We can change the value over time."
            );
        });
    });

    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "## A tiny story\n\
                 We have a box called `apples`.\n\
                 At the start, the box has **3** apples.\n\n\
                 If we add one apple, the number grows.\n\
                 If we take one apple, the number shrinks."
            );
        });
    });

    let apples = nb.state("apples", 3_i32, |ui, value| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label(RichText::new("Try changing the value.").heading());
            ui.add_space(6.0);

            ui.label(RichText::new(format!("apples = {value}")).heading());
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.add(widgets::Button::new("+1")).clicked() {
                    *value = value.saturating_add(1);
                }
                if ui.add(widgets::Button::new("-1")).clicked() {
                    *value = value.saturating_sub(1);
                }
                if ui.add(widgets::Button::new("double")).clicked() {
                    *value = value.saturating_mul(2);
                }
                if ui.add(widgets::Button::new("reset")).clicked() {
                    *value = 3;
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Set value:");
                ui.add(
                    widgets::NumberField::new(value)
                        .speed(1.0)
                        .min_decimals(0)
                        .max_decimals(0),
                );
            });

            if *value == 0 {
                ui.add_space(6.0);
                ui.label("We cannot go below zero apples.");
            }
        });
    });

    nb.state("assignment_step", 0_usize, |ui, step| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            let max_step = 3_usize;
            if *step > max_step {
                *step = max_step;
            }

            let arrow = "\u{2190}";
            let lines = [
                format!("apples {arrow} 3"),
                format!("apples {arrow} apples + 1"),
                format!("apples {arrow} apples - 1"),
                format!("apples {arrow} apples * 2"),
            ];

            ui.label(RichText::new("Code idea (pseudocode)").heading());
            ui.add_space(4.0);
            ui.label("Use the buttons to move the marker.");
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if ui.add(widgets::Button::new("Prev")).clicked() {
                    *step = step.saturating_sub(1);
                }
                if ui.add(widgets::Button::new("Next")).clicked() {
                    *step = (*step + 1).min(max_step);
                }
                if ui.add(widgets::Button::new("Reset")).clicked() {
                    *step = 0;
                }
                ui.add_space(6.0);
                let step_value = *step;
                ui.label(format!("Step {step_value}/{max_step}"));
            });

            ui.add_space(8.0);
            let mut code = String::new();
            for (index, line) in lines.iter().enumerate() {
                let marker = if index == *step { "> " } else { "  " };
                code.push_str(marker);
                code.push_str(line);
                if index + 1 < lines.len() {
                    code.push('\n');
                }
            }
            md!(
                ui,
                "```text\n{code}\n```\n\n\
                 The arrow ({arrow}) means \"update the box\".\n\
                 The name stays the same. The value changes."
            );
        });
    });

    nb.view(move |ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            let value = apples.read(ui);
            md!(
                ui,
                "## What just happened\n\
                 A variable keeps its value until you change it.\n\
                 Buttons change the value, so the number updates.\n\n\
                 Current value: **{value}**"
            );
        });
    });

    nb.state("quiz_state", QuizState::default(), |ui, state| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label(RichText::new("Quick checks").heading());
            ui.add_space(6.0);

            let arrow = "\u{2190}";
            ui.label(format!(
                "1) Start with apples = 3. Then do: apples {arrow} apples + 1."
            ));
            ui.add_space(4.0);
            ui.add(
                widgets::ChoiceToggle::new(&mut state.add_result)
                    .choice(Some(2), "2")
                    .choice(Some(3), "3")
                    .choice(Some(4), "4")
                    .small(),
            );
            ui.add_space(4.0);
            match state.add_result {
                Some(4) => ui.label("Correct!"),
                Some(_) => ui.label("Not quite. Try again."),
                None => ui.label("Pick an answer."),
            }

            ui.add_space(12.0);
            ui.label("2) Start with apples = 10. Subtract ___ to get 6.");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("10 -");
                ui.add(
                    widgets::NumberField::new(&mut state.subtract_amount)
                        .speed(1.0)
                        .min_decimals(0)
                        .max_decimals(0)
                        .constrain_value(&|_, proposed| proposed.clamp(0, 10)),
                );
                ui.label("= 6");
            });
            ui.add_space(4.0);
            if state.subtract_amount == 4 {
                ui.label("Correct!");
            } else {
                ui.label("Try a different number.");
            }
        });
    });
}
