#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! egui-theme-switch = "0.4"
//! ```

use egui::{self, Color32};
use GORBIE::{md, notebook, state, Notebook};

fn to_hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
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
        md!(ui, "# Palette Playbook\n\nA short story of how our theme palette is constructed from four base tokens.");

        // Get the canonical base tokens directly from the themes module
        let ink = GORBIE::themes::base_ink();
        let parchment = GORBIE::themes::base_parchment();
        let brand_primary = GORBIE::themes::base_purple();
        let contrast_accent = GORBIE::themes::base_teal();

        // Derived samples (same rules used in themes.rs)
        let hover_light = blend(parchment, brand_primary, 0.30);
        let panel_alt = blend(parchment, brand_primary, 0.15);

        let hover_dark = blend(ink, contrast_accent, 0.30);
        let panel_alt_dark = blend(Color32::from_hex("#281E2F").unwrap(), contrast_accent, 0.10);

        ui.vertical(|ui| {
            ui.group(|ui| {
                md!(ui, "## Base tokens");
                ui.horizontal(|ui| {
                    swatch(ui, ink, "Ink — dark base");
                    swatch(ui, parchment, "Parchment — light base");
                    swatch(ui, brand_primary, "Brand primary — purple");
                    swatch(ui, contrast_accent, "Contrast accent — teal");
                });
            });

            ui.group(|ui| {
                md!(ui, "## Derived samples (light)");
                ui.horizontal(|ui| {
                    swatch(ui, panel_alt, "Panel (inactive)");
                    swatch(ui, hover_light, "Hover (light)");
                    swatch(ui, blend(panel_alt, parchment, 0.02), "Panel weak");
                });
            });

            ui.group(|ui| {
                md!(ui, "## Derived samples (dark)");
                ui.horizontal(|ui| {
                    swatch(ui, panel_alt_dark, "Panel (inactive, dark)");
                    swatch(ui, hover_dark, "Hover (dark)");
                    swatch(ui, blend(panel_alt_dark, ink, 0.08), "Panel weak (dark)");
                });
            });
        });

        md!(ui, "---\nThese colors are the primitives we use to build the two themes. The light theme uses the purple as Brand Primary while the dark theme swaps to teal as Brand Primary for better contrast on dark backgrounds.");
    });
}

fn main() {
    notebook!(playbook);
}
