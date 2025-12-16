#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! egui-theme-switch = "0.4"
//! ```

use egui::Color32;
use egui::{self};
use std::ops::DerefMut;
use GORBIE::dataflow::NotifiedState;
use GORBIE::md;
use GORBIE::notebook;
use GORBIE::state;
use GORBIE::widgets;
use GORBIE::Notebook;

fn to_hex(c: Color32) -> String {
    let r = c.r();
    let g = c.g();
    let b = c.b();
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn swatch(ui: &mut egui::Ui, color: Color32, label: &str) {
    ui.vertical(|ui| {
        let (_id, rect) = ui.allocate_space(ui.spacing().interact_size);
        ui.painter().rect_filled(rect, 4.0, color);
        ui.label(label);
        ui.colored_label(color, to_hex(color));
    });
}

// local blend util for this playbook so it's independent from themes internals
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let r = (a.r() as f32 * (1.0 - t) + b.r() as f32 * t).round() as u8;
    let g = (a.g() as f32 * (1.0 - t) + b.g() as f32 * t).round() as u8;
    let bch = (a.b() as f32 * (1.0 - t) + b.b() as f32 * t).round() as u8;
    Color32::from_rgb(r, g, bch)
}

fn playbook(nb: &mut Notebook) {
    state!(nb, (), (), |ui, _| {
        md!(
            ui,
            "# Palette Playbook\n\nA short story of how our industrial theme palette is constructed from four base tokens."
        );

        let light_foreground = GORBIE::themes::ral(9011);
        let light_background = GORBIE::themes::ral(7047);
        let light_surface = GORBIE::themes::ral(7047);
        let accent = GORBIE::themes::ral(2009);

        let dark_foreground = GORBIE::themes::ral(9003);
        let dark_background = GORBIE::themes::ral(7046);
        let dark_surface = GORBIE::themes::ral(7047);

        // Derived samples (same rules used in `themes::industrial`)
        let light_surface_muted = blend(light_surface, light_background, 0.2);
        let light_border = blend(light_foreground, light_background, 0.4);
        let light_control_fill_hover = blend(light_background, light_foreground, 0.05);

        let dark_surface_muted = blend(dark_surface, dark_background, 0.2);
        let dark_border = blend(dark_foreground, dark_background, 0.4);
        let dark_control_fill_hover = blend(dark_background, dark_foreground, 0.05);

        ui.vertical(|ui| {
            ui.group(|ui| {
                md!(ui, "## Base tokens (light)");
                ui.horizontal(|ui| {
                    swatch(ui, light_foreground, "Foreground — RAL 9011");
                    swatch(ui, light_background, "Background — RAL 7047 (Telegrey 4)");
                    swatch(ui, light_surface, "Surface — RAL 7047");
                    swatch(ui, accent, "Accent — RAL 2009");
                });
            });

            ui.group(|ui| {
                md!(ui, "## Derived samples (light, industrial)");
                ui.horizontal(|ui| {
                    swatch(ui, light_surface_muted, "faint_bg_color (muted)");
                    swatch(ui, light_control_fill_hover, "extreme_bg_color (hover)");
                    swatch(ui, light_border, "Border (stroke)");
                });
            });

            ui.group(|ui| {
                md!(ui, "## Base tokens (dark)");
                ui.horizontal(|ui| {
                    swatch(ui, dark_foreground, "Foreground — RAL 9003");
                    swatch(ui, dark_background, "Background — RAL 7046 (Telegrey 2)");
                    swatch(ui, dark_surface, "Surface — RAL 7047");
                    swatch(ui, accent, "Accent — RAL 2009");
                });
            });

            ui.group(|ui| {
                md!(ui, "## Derived samples (dark, industrial)");
                ui.horizontal(|ui| {
                    swatch(ui, dark_surface_muted, "faint_bg_color (muted)");
                    swatch(ui, dark_control_fill_hover, "extreme_bg_color (hover)");
                    swatch(ui, dark_border, "Border (stroke)");
                });
            });
        });

        md!(
            ui,
            "---\nThese colors are the primitives we use to build the two themes. Both themes share the same accent (RAL 2009), and vary the base background to RAL 7047/7046."
        );
    });

    state!(
        nb,
        (),
        (0.5_f32).into(),
        |ui, value: &mut NotifiedState<_>| {
            md!(
                ui,
                "## Widget Playbook\n\nA quick showcase of our custom widgets (slider + segmented meter). The value is normalized to `[0, 1]`."
            );

            md!(ui, "### Buttons");
            ui.horizontal(|ui| {
                let _ = ui.add(widgets::Button::new("BUTTON"));
                let _ = ui.add(widgets::Button::new("SMALL").small());
                ui.add_enabled(false, widgets::Button::new("DISABLED"));
                let _ = ui.add(widgets::Button::new("SELECTED").selected(true));
                let _ = ui.add(widgets::Button::new("TOGGLE"));
            });

            if ui
                .add(widgets::Slider::new(value.deref_mut(), 0.0..=1.0).text("LEVEL"))
                .changed()
            {
                value.notify();
            }

            let progress = **value;
            md!(ui, "Value: `{progress:.3}`");

            ui.add(
                widgets::ProgressBar::new(progress)
                    .text("OUTPUT")
                    .scale_percent(),
            );

            let green = GORBIE::themes::ral(6024);
            let yellow = GORBIE::themes::ral(1023);
            let red = GORBIE::themes::ral(3020);

            md!(
                ui,
                "### Multi‑color meter\n\nThis uses normalized color zones (green/yellow/red) and a custom segment count."
            );

            ui.add(
                widgets::ProgressBar::new(progress)
                    .text("SIGNAL")
                    .segments(60)
                    .scale_labels([(0.0, "0"), (0.7, "70"), (0.9, "90"), (1.0, "100")])
                    .zone(0.0..=0.7, green)
                    .zone(0.7..=0.9, yellow)
                    .zone(0.9..=1.0, red),
            );
        }
    );
}

fn main() {
    notebook!(playbook);
}
