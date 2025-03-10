#![allow(non_snake_case)]

pub mod widgets;

use crate::egui::{FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use ctrlc;
use eframe::egui::{self, CollapsingHeader};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use parking_lot::RwLock;
use std::{collections::BTreeMap, sync::Arc};
use tribles::prelude::*;

pub struct CardCtx<'a> {
    pub ui: &'a mut egui::Ui,
    id: Id,
}

impl CardCtx<'_> {
    pub fn id(&self) -> Id {
        self.id
    }
}

pub trait Card {
    fn update(&mut self, ctx: &mut CardCtx) -> ();
}

pub struct MarkdownCard {
    markdown: String,
    cache: CommonMarkCache,
}

impl Card for MarkdownCard {
    fn update(&mut self, ctx: &mut CardCtx) -> () {
        CommonMarkViewer::new().show(ctx.ui, &mut self.cache, &self.markdown);
    }
}

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut CardCtx) -> ()>,
    code: Option<String>,
}

impl Card for StatelessCard {
    fn update(&mut self, ctx: &mut CardCtx) -> () {
        (self.function)(ctx);

        if let Some(code) = &mut self.code {
            CollapsingHeader::new("Code")
                .id_salt(format!("{:x}/code", ctx.id))
                .show(ctx.ui, |ui| {
                    let language = "rs";
                    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(
                        ui.ctx(),
                        ui.style(),
                    );
                    egui_extras::syntax_highlighting::code_view_ui(ui, &theme, code, language);
                });
        }
    }
}

pub fn stateless_card(
    nb: &mut Notebook,
    function: impl FnMut(&mut CardCtx) -> () + 'static,
    code: Option<&str>,
) {
    nb.push_card(Box::new(StatelessCard {
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
    }));
}

#[macro_export]
macro_rules! view {
    ($nb:expr, $code:expr) => {
        $crate::stateless_card($nb, $code, Some(stringify!($code)))
    };
}

pub struct StatefulCard<T: std::default::Default> {
    current: Arc<RwLock<T>>,
    function: Box<dyn FnMut(&mut CardCtx, &mut T)>,
    code: Option<String>,
}

impl<T: std::fmt::Debug + std::default::Default> Card for StatefulCard<T> {
    fn update(&mut self, ctx: &mut CardCtx) -> () {
        let id = ctx.id;

        let mut current = self.current.write();
        (self.function)(ctx, &mut current);

        CollapsingHeader::new("Current")
            .id_salt(format!("{:x}/current", id))
            .show(ctx.ui, |ui| {
                ui.monospace(format!("{:?}", current));
            });

        if let Some(code) = &mut self.code {
            CollapsingHeader::new("Code")
                .id_salt(format!("{:x}/code", id))
                .show(ctx.ui, |ui| {
                    let language = "rs";
                    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(
                        ui.ctx(),
                        ui.style(),
                    );
                    egui_extras::syntax_highlighting::code_view_ui(ui, &theme, code, language);
                });
        }
    }
}

type CardState<T> = Arc<RwLock<T>>;

pub fn stateful_card<T: std::fmt::Debug + std::default::Default + 'static>(
    nb: &mut Notebook,
    init: T,
    function: impl FnMut(&mut CardCtx, &mut T) + 'static,
    code: Option<&str>,
) -> CardState<T> {
    let current = Arc::new(RwLock::new(init));
    nb.push_card(Box::new(StatefulCard {
        current: current.clone(),
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
    }));

    current
}

#[macro_export]
macro_rules! state {
    ($nb:expr, $code:expr) => {
        $crate::stateful_card($nb, Default::default(), $code, Some(stringify!($code)))
    };
    ($nb:expr, $init:expr, $code:expr) => {
        $crate::stateful_card($nb, $init, $code, Some(stringify!($code)))
    };
}

pub struct Notebook {
    pub cards: Vec<(Id, Box<dyn Card>)>,
}

pub fn md(nb: &mut Notebook, markdown: &str) {
    nb.push_card(Box::new(MarkdownCard {
        markdown: markdown.to_owned(),
        cache: CommonMarkCache::default(),
    }));
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
                    style.visuals.override_text_color = Some(egui::Color32::from_hex("#12051d").unwrap());
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
                            let mut ctx = CardCtx { ui, id: *id };
                            card.update(&mut ctx);
                            ui.separator();
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
