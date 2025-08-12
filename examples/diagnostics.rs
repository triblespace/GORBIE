#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! egui-theme-switch = "0.4"
//! ```

use egui::{self, Color32};
use egui_theme_switch::global_theme_switch;
use GORBIE::{md, notebook, state, Notebook};

fn to_hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

// Note: use Color32 directly for the color-edit UI controls.

fn diagnostics(nb: &mut Notebook) {
    state!(
        nb,
        (),
        (Color32::from_hex("#35243E").unwrap()).into(),
        move |ui, value| {
            md!(ui, "# Diagnostics Storybook");

            ui.horizontal(|ui| {
                ui.label("override_text_color");
                ui.color_edit_button_srgba(&mut *value);
                let current = (&*value).clone();
                ui.colored_label(current, to_hex(current));
            });

            let original = ui.visuals().clone();
            ui.visuals_mut().override_text_color = Some(*value);
            ui.separator();
            ui.heading("H1 preview — The quick brown fox");
            ui.label("H2 preview — Subtitle example");
            ui.separator();
            *ui.visuals_mut() = original;
        }
    );

    // Separator: NotifiedState-backed editor + preview
    state!(
        nb,
        (),
        (Color32::from_hex("#CFC4D6").unwrap()).into(),
        |ui, value| {
            ui.horizontal(|ui| {
                ui.label("extreme_bg_color");
                ui.color_edit_button_srgba(&mut *value);
                let current = (&*value).clone();
                ui.colored_label(current, to_hex(current));
            });

            let original = ui.visuals().clone();
            ui.visuals_mut().extreme_bg_color = *value;
            ui.separator();
            ui.label("Above separator uses the current extreme_bg_color");
            ui.separator();
            *ui.visuals_mut() = original;
        }
    );

    // Theme switch: NotifiedState-backed tuple (indicator, icon)
    state!(
        nb,
        (),
        (
            Color32::from_hex("#6B5AE6").unwrap(),
            Color32::from_hex("#FBF6F1").unwrap()
        ),
        |ui, value| {
            let pair = &mut *value;
            ui.horizontal(|ui| {
                ui.label("indicator (selection.bg_fill)");
                ui.color_edit_button_srgba(&mut pair.0);
                ui.colored_label(pair.0, to_hex(pair.0));
                ui.label("icon (widgets.active.fg_stroke)");
                ui.color_edit_button_srgba(&mut pair.1);
                ui.colored_label(pair.1, to_hex(pair.1));
            });

            let original = ui.visuals().clone();
            ui.visuals_mut().selection.bg_fill = pair.0;
            ui.visuals_mut().widgets.active.fg_stroke.color = pair.1;
            ui.separator();
            ui.label("Preview: theme switch rendered with current selection & icon colors");
            global_theme_switch(ui);
            ui.separator();
            *ui.visuals_mut() = original;
        }
    );

    state!(
        nb,
        (),
        (
            Color32::from_hex("#6F6A79").unwrap(),
            Color32::from_hex("#35243E").unwrap()
        ),
        |ui, value| {
            let pair = &mut *value;
            ui.horizontal(|ui| {
                ui.label("inactive.fg");
                ui.color_edit_button_srgba(&mut pair.0);
                ui.colored_label(pair.0, to_hex(pair.0));
                ui.label("hovered.fg");
                ui.color_edit_button_srgba(&mut pair.1);
                ui.colored_label(pair.1, to_hex(pair.1));
            });

            let original = ui.visuals().clone();
            ui.visuals_mut().widgets.inactive.fg_stroke.color = pair.0;
            ui.visuals_mut().widgets.hovered.fg_stroke.color = pair.1;
            ui.separator();
            ui.add(egui::Slider::new(&mut 0.5f32, 0.0..=1.0).text("slider"));
            let _ = ui.button("Example button");
            *ui.visuals_mut() = original;
        }
    );
}

fn main() {
    notebook!(diagnostics);
}
