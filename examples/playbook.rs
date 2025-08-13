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

            ui.collapsing("🎨 Palette (current visuals)", |ui| {
                let vis = ui.style().visuals.clone();
                ui.label("Live visuals (style currently applied to this UI):");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Ink (dark text/base)"); ui.colored_label(vis.window_fill, to_hex(vis.window_fill));
                    ui.label("Parchment (panel background)"); ui.colored_label(vis.panel_fill, to_hex(vis.panel_fill));
                });

                ui.label("Semantic brand tokens (from visuals):");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Brand primary (selection)"); ui.colored_label(vis.selection.bg_fill, to_hex(vis.selection.bg_fill));
                    ui.label("Selection stroke (icon color)"); ui.colored_label(vis.selection.stroke.color, to_hex(vis.selection.stroke.color));
                });

                ui.label("Widget fills (inactive / hovered / active):");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Inactive fill"); ui.colored_label(vis.widgets.inactive.bg_fill, to_hex(vis.widgets.inactive.bg_fill));
                    ui.label("Hovered fill"); ui.colored_label(vis.widgets.hovered.bg_fill, to_hex(vis.widgets.hovered.bg_fill));
                    ui.label("Active fill"); ui.colored_label(vis.widgets.active.bg_fill, to_hex(vis.widgets.active.bg_fill));
                });

                ui.separator();
                ui.label("Theme defaults (source-of-truth in code):");
                let light_vis = GORBIE::themes::cosmic_gel_light().visuals;
                ui.label("Light theme (brand_primary = purple, contrast_accent = teal)");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Brand primary (selection)"); ui.colored_label(light_vis.selection.bg_fill, to_hex(light_vis.selection.bg_fill));
                    ui.label("Selection stroke"); ui.colored_label(light_vis.selection.stroke.color, to_hex(light_vis.selection.stroke.color));
                });

                let dark_vis = GORBIE::themes::cosmic_gel_dark().visuals;
                ui.label("Dark theme (brand_primary = teal, contrast_accent = purple)");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Brand primary (selection)"); ui.colored_label(dark_vis.selection.bg_fill, to_hex(dark_vis.selection.bg_fill));
                    ui.label("Selection stroke"); ui.colored_label(dark_vis.selection.stroke.color, to_hex(dark_vis.selection.stroke.color));
                });

                ui.label("Notes: 'Brand primary' is the main accent; 'Contrast accent' is used for secondary highlights. Hover/active fills are derived from those tokens and bounded between Ink and Parchment.");
            });

            ui.horizontal_wrapped(|ui| {
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
            ui.horizontal_wrapped(|ui| {
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
            // selection bg, selection stroke color, selection stroke width,
            Option::<Color32>::None,
            Option::<Color32>::None,
            Option::<f32>::None,
            // widget fg: inactive, hovered, active
            Option::<Color32>::None,
            Option::<Color32>::None,
            Option::<Color32>::None,
            // widget bg: inactive, hovered, active
            Option::<Color32>::None,
            Option::<Color32>::None,
            Option::<Color32>::None,
        ),
        |ui, value| {
            let (
                selection_bg,
                selection_icon_color,
                selection_width,
                inactive_fg,
                hovered_fg,
                active_fg,
                inactive_bg,
                hovered_bg,
                active_bg,
            ) = &mut *value;

            // read current visuals once per frame
            let current = ui.visuals().clone();

            // local fallbacks: prefer stored override, else current theme
            let mut sel_bg_local = selection_bg.unwrap_or(current.selection.bg_fill);
            let mut sel_icon_local = selection_icon_color.unwrap_or(current.selection.stroke.color);
            let mut sel_width_local = selection_width.unwrap_or(current.selection.stroke.width);

            let mut inactive_fg_local = inactive_fg.unwrap_or(current.widgets.inactive.fg_stroke.color);
            let mut hovered_fg_local = hovered_fg.unwrap_or(current.widgets.hovered.fg_stroke.color);
            let mut active_fg_local = active_fg.unwrap_or(current.widgets.active.fg_stroke.color);

            let mut inactive_bg_local = inactive_bg.unwrap_or(current.widgets.inactive.bg_fill);
            let mut hovered_bg_local = hovered_bg.unwrap_or(current.widgets.hovered.bg_fill);
            let mut active_bg_local = active_bg.unwrap_or(current.widgets.active.bg_fill);

            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("indicator (selection.bg_fill)");
                    let resp = ui.color_edit_button_srgba(&mut sel_bg_local);
                    if resp.changed() {
                        *selection_bg = Some(sel_bg_local);
                    }
                    ui.colored_label(sel_bg_local, to_hex(sel_bg_local));
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label("icon (selection.stroke color)");
                    let resp = ui.color_edit_button_srgba(&mut sel_icon_local);
                    if resp.changed() {
                        *selection_icon_color = Some(sel_icon_local);
                    }
                    ui.colored_label(sel_icon_local, to_hex(sel_icon_local));

                    ui.label("stroke width");
                    let resp = ui.add(egui::Slider::new(&mut sel_width_local, 0.0..=6.0).text("width"));
                    if resp.changed() {
                        *selection_width = Some(sel_width_local);
                    }
                });

                ui.separator();

                ui.label("Theme selector widget colors");

                ui.horizontal_wrapped(|ui| {
                    ui.label("FG: inactive");
                    let resp = ui.color_edit_button_srgba(&mut inactive_fg_local);
                    if resp.changed() {
                        *inactive_fg = Some(inactive_fg_local);
                    }
                    ui.colored_label(inactive_fg_local, to_hex(inactive_fg_local));

                    ui.label("hovered");
                    let resp = ui.color_edit_button_srgba(&mut hovered_fg_local);
                    if resp.changed() {
                        *hovered_fg = Some(hovered_fg_local);
                    }
                    ui.colored_label(hovered_fg_local, to_hex(hovered_fg_local));

                    ui.label("active");
                    let resp = ui.color_edit_button_srgba(&mut active_fg_local);
                    if resp.changed() {
                        *active_fg = Some(active_fg_local);
                    }
                    ui.colored_label(active_fg_local, to_hex(active_fg_local));
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label("BG: inactive");
                    let resp = ui.color_edit_button_srgba(&mut inactive_bg_local);
                    if resp.changed() {
                        *inactive_bg = Some(inactive_bg_local);
                    }
                    ui.colored_label(inactive_bg_local, to_hex(inactive_bg_local));

                    ui.label("hovered");
                    let resp = ui.color_edit_button_srgba(&mut hovered_bg_local);
                    if resp.changed() {
                        *hovered_bg = Some(hovered_bg_local);
                    }
                    ui.colored_label(hovered_bg_local, to_hex(hovered_bg_local));

                    ui.label("active");
                    let resp = ui.color_edit_button_srgba(&mut active_bg_local);
                    if resp.changed() {
                        *active_bg = Some(active_bg_local);
                    }
                    ui.colored_label(active_bg_local, to_hex(active_bg_local));
                });
            });

            let original = ui.visuals().clone();

            ui.visuals_mut().selection.bg_fill = sel_bg_local;
            ui.visuals_mut().selection.stroke = egui::Stroke::new(sel_width_local, sel_icon_local);

            ui.visuals_mut().widgets.inactive.fg_stroke.color = inactive_fg_local;
            ui.visuals_mut().widgets.inactive.bg_fill = inactive_bg_local;

            ui.visuals_mut().widgets.hovered.fg_stroke.color = hovered_fg_local;
            ui.visuals_mut().widgets.hovered.bg_fill = hovered_bg_local;

            ui.visuals_mut().widgets.active.fg_stroke.color = active_fg_local;
            ui.visuals_mut().widgets.active.bg_fill = active_bg_local;

            ui.separator();
            ui.label("Preview: theme switch rendered with current selection & icon colors");
            global_theme_switch(ui);
            ui.separator();

            // Inspector: show current visuals vs effective visuals we will apply
            ui.collapsing("Inspector: current vs effective visuals", |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Current visuals");
                        ui.label("selection.bg_fill");
                        ui.colored_label(current.selection.bg_fill, to_hex(current.selection.bg_fill));
                        ui.label("selection.stroke.color");
                        ui.colored_label(current.selection.stroke.color, to_hex(current.selection.stroke.color));
                        ui.label("selection.stroke.width");
                        ui.label(format!("{:.1}", current.selection.stroke.width));

                        ui.label("inactive.bg_fill");
                        ui.colored_label(current.widgets.inactive.bg_fill, to_hex(current.widgets.inactive.bg_fill));
                        ui.label("inactive.fg_stroke");
                        ui.colored_label(current.widgets.inactive.fg_stroke.color, to_hex(current.widgets.inactive.fg_stroke.color));

                        ui.label("hovered.bg_fill");
                        ui.colored_label(current.widgets.hovered.bg_fill, to_hex(current.widgets.hovered.bg_fill));
                        ui.label("hovered.fg_stroke");
                        ui.colored_label(current.widgets.hovered.fg_stroke.color, to_hex(current.widgets.hovered.fg_stroke.color));

                        ui.label("active.bg_fill");
                        ui.colored_label(current.widgets.active.bg_fill, to_hex(current.widgets.active.bg_fill));
                        ui.label("active.fg_stroke");
                        ui.colored_label(current.widgets.active.fg_stroke.color, to_hex(current.widgets.active.fg_stroke.color));
                    });

                    ui.vertical(|ui| {
                        ui.label("Effective visuals (what preview will use)");
                        ui.label("selection.bg_fill");
                        ui.colored_label(sel_bg_local, to_hex(sel_bg_local));
                        ui.label("selection.stroke.color");
                        ui.colored_label(sel_icon_local, to_hex(sel_icon_local));
                        ui.label("selection.stroke.width");
                        ui.label(format!("{:.1}", sel_width_local));

                        ui.label("inactive.bg_fill");
                        ui.colored_label(inactive_bg_local, to_hex(inactive_bg_local));
                        ui.label("inactive.fg_stroke");
                        ui.colored_label(inactive_fg_local, to_hex(inactive_fg_local));

                        ui.label("hovered.bg_fill");
                        ui.colored_label(hovered_bg_local, to_hex(hovered_bg_local));
                        ui.label("hovered.fg_stroke");
                        ui.colored_label(hovered_fg_local, to_hex(hovered_fg_local));

                        ui.label("active.bg_fill");
                        ui.colored_label(active_bg_local, to_hex(active_bg_local));
                        ui.label("active.fg_stroke");
                        ui.colored_label(active_fg_local, to_hex(active_fg_local));
                    });
                });
            });

            *ui.visuals_mut() = original;
        }
    );

    state!(
        nb,
        (),
        (
            // make these optional so we can lazily override
            Option::<Color32>::None,
            Option::<Color32>::None,
            Option::<Color32>::None,
            Option::<Color32>::None,
            Option::<Color32>::None,
            Option::<Color32>::None,
        ),
        |ui, value| {
            let (
                inactive_fg,
                hovered_fg,
                active_fg,
                inactive_bg,
                hovered_bg,
                active_bg,
            ) = &mut *value;

            // read current visuals once per frame
            let current = ui.visuals().clone();

            // local fallbacks: prefer stored override, else current theme
            let mut inactive_fg_local = inactive_fg.unwrap_or(current.widgets.inactive.fg_stroke.color);
            let mut hovered_fg_local = hovered_fg.unwrap_or(current.widgets.hovered.fg_stroke.color);
            let mut active_fg_local = active_fg.unwrap_or(current.widgets.active.fg_stroke.color);

            let mut inactive_bg_local = inactive_bg.unwrap_or(current.widgets.inactive.bg_fill);
            let mut hovered_bg_local = hovered_bg.unwrap_or(current.widgets.hovered.bg_fill);
            let mut active_bg_local = active_bg.unwrap_or(current.widgets.active.bg_fill);

            ui.label("Theme selector widget colors");

            ui.horizontal_wrapped(|ui| {
                ui.label("FG: inactive");
                let resp = ui.color_edit_button_srgba(&mut inactive_fg_local);
                if resp.changed() {
                    *inactive_fg = Some(inactive_fg_local);
                }
                ui.colored_label(inactive_fg_local, to_hex(inactive_fg_local));

                ui.label("hovered");
                let resp = ui.color_edit_button_srgba(&mut hovered_fg_local);
                if resp.changed() {
                    *hovered_fg = Some(hovered_fg_local);
                }
                ui.colored_label(hovered_fg_local, to_hex(hovered_fg_local));

                ui.label("active");
                let resp = ui.color_edit_button_srgba(&mut active_fg_local);
                if resp.changed() {
                    *active_fg = Some(active_fg_local);
                }
                ui.colored_label(active_fg_local, to_hex(active_fg_local));
            });

            ui.horizontal_wrapped(|ui| {
                ui.label("BG: inactive");
                let resp = ui.color_edit_button_srgba(&mut inactive_bg_local);
                if resp.changed() {
                    *inactive_bg = Some(inactive_bg_local);
                }
                ui.colored_label(inactive_bg_local, to_hex(inactive_bg_local));

                ui.label("hovered");
                let resp = ui.color_edit_button_srgba(&mut hovered_bg_local);
                if resp.changed() {
                    *hovered_bg = Some(hovered_bg_local);
                }
                ui.colored_label(hovered_bg_local, to_hex(hovered_bg_local));

                ui.label("active");
                let resp = ui.color_edit_button_srgba(&mut active_bg_local);
                if resp.changed() {
                    *active_bg = Some(active_bg_local);
                }
                ui.colored_label(active_bg_local, to_hex(active_bg_local));
            });

            let original = ui.visuals().clone();
            ui.visuals_mut().widgets.inactive.fg_stroke.color = inactive_fg_local;
            ui.visuals_mut().widgets.inactive.bg_fill = inactive_bg_local;

            ui.visuals_mut().widgets.hovered.fg_stroke.color = hovered_fg_local;
            ui.visuals_mut().widgets.hovered.bg_fill = hovered_bg_local;

            ui.visuals_mut().widgets.active.fg_stroke.color = active_fg_local;
            ui.visuals_mut().widgets.active.bg_fill = active_bg_local;

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
