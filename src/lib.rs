#![allow(non_snake_case)]

pub mod cards;
pub mod widgets;
pub mod dataflow;

pub use cards::*;
pub use dataflow::*;
use crate::egui::{FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use ctrlc;
use eframe::egui::{self};
use std::collections::BTreeMap;
use tribles::prelude::*;

pub struct Notebook {
    pub cards: Vec<(Id, Box<dyn Card>)>,
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

                let mut fonts = FontDefinitions::default();
                fonts.font_data.insert(
                    "lora".to_owned(),
                    std::sync::Arc::new(FontData::from_static(include_bytes!(
                        "../assets/fonts/Lora/Lora-VariableFont_wght.ttf"
                    ))),
                );
                fonts.font_data.insert("atkinson".to_owned(),
                    std::sync::Arc::new(
                        FontData::from_static(include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/AtkinsonHyperlegibleNext-VariableFont_wght.ttf"))
                    )
                    );
                fonts.font_data.insert(
                    "roboto_mono".to_owned(),
                    std::sync::Arc::new(FontData::from_static(include_bytes!(
                        "../assets/fonts/Roboto_Mono/RobotoMono-VariableFont_wght.ttf"
                    ))),
                );
                fonts
                    .families
                    .get_mut(&FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "atkinson".to_owned());
                fonts
                    .families
                    .get_mut(&FontFamily::Monospace)
                    .unwrap()
                    .insert(0, "roboto_mono".to_owned());

                fonts
                    .families
                    .insert(FontFamily::Name("lora".into()), vec!["lora".into()]);
                fonts
                    .families
                    .insert(FontFamily::Name("atkinson".into()), vec!["atkinson".into()]);
                fonts.families.insert(
                    FontFamily::Name("roboto_mono".into()),
                    vec!["roboto_mono".into()],
                );

                cc.egui_ctx.set_fonts(fonts);

                let text_styles: BTreeMap<_, _> = [
                    (
                        TextStyle::Heading,
                        FontId::new(32.0, FontFamily::Name("lora".into())),
                    ),
                    (TextStyle::Body, FontId::new(16.0, FontFamily::Proportional)),
                    (
                        TextStyle::Monospace,
                        FontId::new(16.0, FontFamily::Monospace),
                    ),
                    (
                        TextStyle::Button,
                        FontId::new(16.0, FontFamily::Proportional),
                    ),
                    (
                        TextStyle::Small,
                        FontId::new(12.0, FontFamily::Proportional),
                    ),
                ]
                .into();

                cc.egui_ctx.all_styles_mut(move |style| {
                    style.text_styles = text_styles.clone();

                    // Base color: #130496
                    // Fade to white:
                    // #130496 #5032a8 #7858ba #9b80cc #bda9dd #ded3ee #ffffff
                    // Fade to black:
                    // #130496 #1a087b #1c0b62 #1a0c4a #170b32 #12051d #000000
                    style.visuals.window_fill = egui::Color32::from_hex("#ffffff").unwrap();
                    style.visuals.panel_fill = egui::Color32::from_hex("#ffffff").unwrap();
                    style.visuals.faint_bg_color = egui::Color32::from_hex("#ded3ee").unwrap();
                    style.visuals.extreme_bg_color = egui::Color32::from_hex("#9b80cc").unwrap();
                    style.visuals.code_bg_color = egui::Color32::from_hex("#ded3ee").unwrap();
                    style.visuals.selection.bg_fill = egui::Color32::from_hex("#bda9dd").unwrap();
                    style.visuals.hyperlink_color = egui::Color32::from_hex("#130496").unwrap();
                    style.visuals.warn_fg_color = egui::Color32::from_hex("#1a087b").unwrap();
                    style.visuals.error_fg_color = egui::Color32::from_hex("#1c0b62").unwrap();
                    style.visuals.widgets.active.fg_stroke.color =
                        egui::Color32::from_hex("#170b32").unwrap();
                    style.visuals.override_text_color =
                        Some(egui::Color32::from_hex("#12051d").unwrap());
                });

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
