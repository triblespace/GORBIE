//! ## Working with mutable/non-cloneable things.
//! Sometimes when working with existing code, libraries or even std things like
//! files, can introduce an impedance mismatch with the reactive data-flow model.
//! Often it is enough to wrap the object in question into another layer of `Arc`s
//! and `RWLock`s in addition to what Gorby already does with it's `CardState`.
//!
//! - This is also why we compare explicit generations instead of return values,
//! to broaden the range of types that can be used with `derive!`. -
//!
//! But sometimes that isn't enough, e.g. when you want to display some application
//! global state. This is why `state!` and `view!` are carefully designed to not
//! rely on the dataflow mechanisms introduced by `derive`. Instead they can be
//! used, like any other mutable rust type, modulo the `CardState` wrapper.
//!

#![allow(non_snake_case)]

pub mod cards;
pub mod dataflow;
pub mod widgets;

use crate::egui::{FontData, FontDefinitions, FontFamily, FontId, TextStyle};
pub use cards::*;
use ctrlc;
pub use dataflow::*;
use eframe::egui::{self};
use egui::{style::{Selection, WidgetVisuals, Widgets}, Color32, Stroke, Style, Visuals};
use egui_theme_switch::global_theme_switch;

use tribles::prelude::*;

/// A notebook is a collection of cards.
/// Each card is a piece of content that can be displayed in the notebook.
/// Cards can be stateless, stateful, or reactively derived from other cards.
pub struct Notebook {
    pub cards: Vec<(Id, Box<dyn Card>)>,
}

pub fn cosmic_gel_light() -> Style {
    let mut style = Style::default();

    style.text_styles = cosmic_gel_text_styles()
        .into_iter()
        .collect();

    let visuals = Visuals {
        dark_mode: false,
        window_fill: Color32::from_hex("#F7F3F2").unwrap(), // Warm light grey background
        panel_fill: Color32::from_hex("#EDE9E8").unwrap(),  // Slightly darker light grey for panels
        override_text_color: Some(Color32::from_hex("#2E2A2B").unwrap()), // Dark grey for text
        faint_bg_color: Color32::from_hex("#E0DBDA").unwrap(), // Subtle contrast for faint backgrounds
        extreme_bg_color: Color32::from_hex("#CFC9C8").unwrap(), // Slightly darker grey for extreme contrast
        selection: Selection {
            bg_fill: Color32::from_hex("#6A5ACD").unwrap(), // Muted blue for selection
            stroke: Stroke::new(1.0, Color32::from_hex("#6A5ACD").unwrap()),
        },
        hyperlink_color: Color32::from_hex("#6A5ACD").unwrap(), // Muted blue for links
        widgets: Widgets {
            noninteractive: WidgetVisuals {
                bg_fill: Color32::from_hex("#EDE9E8").unwrap(),
                weak_bg_fill: Color32::from_hex("#E0DBDA").unwrap(),
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, Color32::from_hex("#6A5ACD").unwrap()), // Muted blue for noninteractive text
                corner_radius: 6.0.into(),
                expansion: 0.0,
            },
            inactive: WidgetVisuals {
                bg_fill: Color32::from_hex("#E0DBDA").unwrap(),
                weak_bg_fill: Color32::from_hex("#D6D1D0").unwrap(),
                bg_stroke: Stroke::new(1.0, Color32::from_hex("#6A5ACD").unwrap()),
                fg_stroke: Stroke::new(1.0, Color32::from_hex("#2E2A2B").unwrap()),
                corner_radius: 6.0.into(),
                expansion: 2.0,
            },
            hovered: WidgetVisuals {
                bg_fill: Color32::from_hex("#D6D1D0").unwrap(),
                weak_bg_fill: Color32::from_hex("#6A5ACD").unwrap(),
                bg_stroke: Stroke::new(1.5, Color32::from_hex("#6A5ACD").unwrap()),
                fg_stroke: Stroke::new(1.2, Color32::BLACK),
                corner_radius: 6.0.into(),
                expansion: 3.0,
            },
            active: WidgetVisuals {
                bg_fill: Color32::from_hex("#6A5ACD").unwrap(),
                weak_bg_fill: Color32::from_hex("#D6D1D0").unwrap(),
                bg_stroke: Stroke::new(1.5, Color32::BLACK),
                fg_stroke: Stroke::new(1.5, Color32::from_hex("#2E2A2B").unwrap()),
                corner_radius: 6.0.into(),
                expansion: 2.0,
            },
            open: WidgetVisuals {
                bg_fill: Color32::from_hex("#EDE9E8").unwrap(),
                weak_bg_fill: Color32::from_hex("#E0DBDA").unwrap(),
                bg_stroke: Stroke::new(1.0, Color32::from_hex("#6A5ACD").unwrap()),
                fg_stroke: Stroke::new(1.0, Color32::from_hex("#2E2A2B").unwrap()),
                corner_radius: 6.0.into(),
                expansion: 2.0,
            },
        },
        window_shadow: egui::epaint::Shadow {
            offset: [0, 4],
            blur: 8,
            spread: 0,
            color: Color32::from_rgba_premultiplied(0, 0, 0, 64),
        },
        ..Visuals::light()
    };

    style.visuals = visuals;
    style
}

pub fn cosmic_gel_dark() -> Style {
    let mut style = Style::default();

    style.text_styles = cosmic_gel_text_styles()
        .into_iter()
        .collect();

    let visuals = Visuals {
        dark_mode: true,
        window_fill: Color32::from_hex("#2E2A2B").unwrap(), // Warm grey background
        panel_fill: Color32::from_hex("#3A3637").unwrap(),  // Slightly darker grey for panels
        override_text_color: Some(Color32::from_hex("#EDE9E8").unwrap()), // Soft off-white for text
        faint_bg_color: Color32::from_hex("#4A4546").unwrap(), // Subtle contrast for faint backgrounds
        extreme_bg_color: Color32::from_hex("#1F1C1D").unwrap(), // Darker grey for extreme contrast
        selection: Selection {
            bg_fill: Color32::from_hex("#6A5ACD").unwrap(), // Muted blue for selection
            stroke: Stroke::new(1.0, Color32::from_hex("#6A5ACD").unwrap()),
        },
        hyperlink_color: Color32::from_hex("#6A5ACD").unwrap(), // Muted blue for links
        widgets: Widgets {
            noninteractive: WidgetVisuals {
                bg_fill: Color32::from_hex("#3A3637").unwrap(),
                weak_bg_fill: Color32::from_hex("#4A4546").unwrap(),
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, Color32::from_hex("#B0ACA9").unwrap()), // Soft grey for noninteractive text
                corner_radius: 6.0.into(),
                expansion: 0.0,
            },
            inactive: WidgetVisuals {
                bg_fill: Color32::from_hex("#4A4546").unwrap(),
                weak_bg_fill: Color32::from_hex("#5A5556").unwrap(),
                bg_stroke: Stroke::new(1.0, Color32::from_hex("#6A5ACD").unwrap()),
                fg_stroke: Stroke::new(1.0, Color32::from_hex("#EDE9E8").unwrap()),
                corner_radius: 6.0.into(),
                expansion: 2.0,
            },
            hovered: WidgetVisuals {
                bg_fill: Color32::from_hex("#5A5556").unwrap(),
                weak_bg_fill: Color32::from_hex("#6A5ACD").unwrap(),
                bg_stroke: Stroke::new(1.5, Color32::from_hex("#6A5ACD").unwrap()),
                fg_stroke: Stroke::new(1.2, Color32::WHITE),
                corner_radius: 6.0.into(),
                expansion: 3.0,
            },
            active: WidgetVisuals {
                bg_fill: Color32::from_hex("#6A5ACD").unwrap(),
                weak_bg_fill: Color32::from_hex("#5A5556").unwrap(),
                bg_stroke: Stroke::new(1.5, Color32::WHITE),
                fg_stroke: Stroke::new(1.5, Color32::from_hex("#EDE9E8").unwrap()),
                corner_radius: 6.0.into(),
                expansion: 2.0,
            },
            open: WidgetVisuals {
                bg_fill: Color32::from_hex("#3A3637").unwrap(),
                weak_bg_fill: Color32::from_hex("#4A4546").unwrap(),
                bg_stroke: Stroke::new(1.0, Color32::from_hex("#B0ACA9").unwrap()),
                fg_stroke: Stroke::new(1.0, Color32::from_hex("#EDE9E8").unwrap()),
                corner_radius: 6.0.into(),
                expansion: 2.0,
            },
        },
        window_shadow: egui::epaint::Shadow {
            offset: [0, 4],
            blur: 8,
            spread: 0,
            color: Color32::from_rgba_premultiplied(0, 0, 0, 128),
        },
        ..Visuals::dark()
    };

    style.visuals = visuals;
    style
}

pub fn cosmic_gel_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "Lora".to_owned(),
        std::sync::Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Lora/Lora-VariableFont_wght.ttf"
        ))),
    );

    fonts.font_data.insert(
        "Caprasimo".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Caprasimo/Caprasimo-Regular.ttf")).into(),
    );

    fonts.font_data.insert(
        "JetBrainsMono".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/JetBrains_Mono/static/JetBrainsMono-Regular.ttf")).into(),
    );

    // Set up font families
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "Lora".to_owned());
    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .unwrap()
        .insert(0, "JetBrainsMono".to_owned());
    
    fonts.families.insert(
        FontFamily::Name("Lora".into()),
        vec!["Lora".to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name("Caprasimo".into()),
        vec!["Caprasimo".to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name("JetBrainsMono".into()),
        vec!["JetBrainsMono".to_owned()],
    );

    fonts
}

pub fn cosmic_gel_text_styles() -> Vec<(TextStyle, FontId)> {
    vec![
        (
            TextStyle::Heading,
            FontId::new(30.0, FontFamily::Name("Caprasimo".into())),
        ),
        (
            TextStyle::Body,
            FontId::new(16.0, FontFamily::Name("Lora".into())),
        ),
        (
            TextStyle::Monospace,
            FontId::new(14.0, FontFamily::Name("JetBrainsMono".into())),
        ),
        (
            TextStyle::Button,
            FontId::new(16.0, FontFamily::Name("Lora".into())),
        ),
        (
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Name("Lora".into())),
        ),
    ]
}

impl Notebook {
    pub fn new() -> Self {
        Self { cards: Vec::new() }
    }

    pub fn push_card(&mut self, card: Box<dyn Card>) {
        self.cards.push((*fucid(), card));
    }

    pub fn run(self, name: &str) -> eframe::Result {
        let mut native_options = eframe::NativeOptions::default();
        native_options.persist_window = true;

        eframe::run_native(
            name,
            native_options,
            Box::new(|cc| {
                let ctx = cc.egui_ctx.clone();
                ctrlc::set_handler(move || ctx.send_viewport_cmd(egui::ViewportCommand::Close))
                    .expect("failed to set exit signal handler");

                cc.egui_ctx.set_fonts(cosmic_gel_fonts());
                
                cc.egui_ctx
                    .set_style_of(egui::Theme::Light, cosmic_gel_light());
                cc.egui_ctx
                    .set_style_of(egui::Theme::Dark, cosmic_gel_dark());

                Ok(Box::new(self))
            }),
        )
    }
}

impl eframe::App for Notebook {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    let mut frame = egui::Frame::default().outer_margin(16.0).begin(ui);
                    {
                        frame.content_ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            global_theme_switch(ui);
                        });
                    }
                    frame.end(ui);

                    let frame = egui::Frame::default().begin(ui);
                    {
                        ui.vertical_centered(|ui| {
                            ui.set_max_width(740.0);
                            for (id, card) in &mut self.cards {
                                ui.push_id(&id, |ui| {
                                    let mut ctx = CardCtx::new(ui, *id);
                                    card.update(&mut ctx);
                                    ui.separator();
                                });
                            }
                        });
                    }
                    frame.end(ui);
                });
        });
    }
}

#[macro_export]
macro_rules! notebook {
    ($setup:ident) => {
        let mut notebook = Notebook::new();
        $setup(&mut notebook);

        let this_file = file!();
        let filename = std::path::Path::new(this_file)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap();

        notebook.run(filename).unwrap();
    };
}
