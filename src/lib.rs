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
use egui_theme_switch::global_theme_switch;

use tribles::prelude::*;

/// A notebook is a collection of cards.
/// Each card is a piece of content that can be displayed in the notebook.
/// Cards can be stateless, stateful, or reactively derived from other cards.
pub struct Notebook {
    pub cards: Vec<(Id, Box<dyn Card>)>,
}

pub fn gorbie_fonts() -> FontDefinitions {
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

    fonts
}

pub fn gorbie_theme_light() -> egui::Style {
    let mut style = egui::Style::default();
    style.text_styles = [
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
    style.visuals.override_text_color = Some(egui::Color32::from_hex("#12051d").unwrap());

    style.visuals.widgets.active.weak_bg_fill = egui::Color32::from_hex("#bda9dd").unwrap();
    style.visuals.widgets.active.bg_fill = egui::Color32::from_hex("#9b80cc").unwrap();
    style.visuals.widgets.active.bg_stroke.color = egui::Color32::from_hex("#7858ba").unwrap();
    style.visuals.widgets.active.fg_stroke.color = egui::Color32::from_hex("#5032a8").unwrap();

    style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_hex("#bda9dd").unwrap();
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_hex("#9b80cc").unwrap();
    style.visuals.widgets.inactive.bg_stroke.color = egui::Color32::from_hex("#7858ba").unwrap();
    style.visuals.widgets.inactive.fg_stroke.color = egui::Color32::from_hex("#5032a8").unwrap();

    style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_hex("#9b80cc").unwrap();
    style.visuals.widgets.noninteractive.weak_bg_fill = egui::Color32::from_hex("#bda9dd").unwrap();
    style.visuals.widgets.noninteractive.bg_stroke.color = egui::Color32::from_hex("#7858ba").unwrap();
    style.visuals.widgets.noninteractive.fg_stroke.color = egui::Color32::from_hex("#5032a8").unwrap();

    style.visuals.widgets.open.weak_bg_fill = egui::Color32::from_hex("#bda9dd").unwrap();
    style.visuals.widgets.open.bg_fill = egui::Color32::from_hex("#9b80cc").unwrap();
    style.visuals.widgets.open.bg_stroke.color = egui::Color32::from_hex("#7858ba").unwrap();
    style.visuals.widgets.open.fg_stroke.color = egui::Color32::from_hex("#5032a8").unwrap();
    
    style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_hex("#bda9dd").unwrap();
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_hex("#9b80cc").unwrap();
    style.visuals.widgets.hovered.bg_stroke.color = egui::Color32::from_hex("#7858ba").unwrap();
    style.visuals.widgets.hovered.fg_stroke.color = egui::Color32::from_hex("#5032a8").unwrap();
    
    style
}

pub fn gorbie_theme_dark() -> egui::Style {
    let mut style = egui::Style::default();
    style.text_styles = [
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

    // Base color: #130496
    // Fade to white:
    // #130496 #5032a8 #7858ba #9b80cc #bda9dd #ded3ee #ffffff
    // Fade to black:
    // #130496 #1a087b #1c0b62 #1a0c4a #170b32 #12051d #000000
    style.visuals.window_fill = egui::Color32::from_hex("#000000").unwrap();
    style.visuals.panel_fill = egui::Color32::from_hex("#000000").unwrap();
    style.visuals.faint_bg_color = egui::Color32::from_hex("#12051d").unwrap();
    style.visuals.extreme_bg_color = egui::Color32::from_hex("#1a0c4a").unwrap();
    style.visuals.code_bg_color = egui::Color32::from_hex("#12051d").unwrap();
    style.visuals.selection.bg_fill = egui::Color32::from_hex("#170b32").unwrap();
    style.visuals.hyperlink_color = egui::Color32::from_hex("#130496").unwrap();
    style.visuals.warn_fg_color = egui::Color32::from_hex("#5032a8").unwrap();
    style.visuals.error_fg_color = egui::Color32::from_hex("#7858ba").unwrap();
    style.visuals.override_text_color = Some(egui::Color32::from_hex("#ded3ee").unwrap());

    style.visuals.widgets.active.weak_bg_fill = egui::Color32::from_hex("#170b32").unwrap();
    style.visuals.widgets.active.bg_fill = egui::Color32::from_hex("#1a0c4a").unwrap();
    style.visuals.widgets.active.bg_stroke.color = egui::Color32::from_hex("#1c0b62").unwrap();
    style.visuals.widgets.active.fg_stroke.color = egui::Color32::from_hex("#1a087b").unwrap();
    
    style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_hex("#170b32").unwrap();
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_hex("#1a0c4a").unwrap();
    style.visuals.widgets.inactive.bg_stroke.color = egui::Color32::from_hex("#1c0b62").unwrap();
    style.visuals.widgets.inactive.fg_stroke.color = egui::Color32::from_hex("#1a087b").unwrap();

    style.visuals.widgets.noninteractive.weak_bg_fill = egui::Color32::from_hex("#170b32").unwrap();
    style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_hex("#1a0c4a").unwrap();
    style.visuals.widgets.noninteractive.bg_stroke.color = egui::Color32::from_hex("#1c0b62").unwrap();
    style.visuals.widgets.noninteractive.fg_stroke.color = egui::Color32::from_hex("#1a087b").unwrap();

    style.visuals.widgets.open.weak_bg_fill = egui::Color32::from_hex("#170b32").unwrap();
    style.visuals.widgets.open.bg_fill = egui::Color32::from_hex("#1a0c4a").unwrap();
    style.visuals.widgets.open.bg_stroke.color = egui::Color32::from_hex("#1c0b62").unwrap();
    style.visuals.widgets.open.fg_stroke.color = egui::Color32::from_hex("#1a087b").unwrap();

    style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_hex("#170b32").unwrap();
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_hex("#1a0c4a").unwrap();
    style.visuals.widgets.hovered.bg_stroke.color = egui::Color32::from_hex("#1c0b62").unwrap();
    style.visuals.widgets.hovered.fg_stroke.color = egui::Color32::from_hex("#1a087b").unwrap();

    style
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

                cc.egui_ctx.set_fonts(gorbie_fonts());
                cc.egui_ctx
                    .set_style_of(egui::Theme::Light, gorbie_theme_light());
                cc.egui_ctx
                    .set_style_of(egui::Theme::Dark, gorbie_theme_dark());

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
