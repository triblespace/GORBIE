use crate::egui::{FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use ctrlc;
use eframe::egui::{self, CollapsingHeader};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::collections::BTreeMap;
use tribles::prelude::*;

pub trait Cell {
    fn view(&mut self, ui: &mut egui::Ui) -> ();
    fn id(&self) -> Id;
}

pub struct MarkdownCell {
    id: Id,
    markdown: String,
    cache: CommonMarkCache,
}

impl Cell for MarkdownCell {
    fn view(&mut self, ui: &mut egui::Ui) -> () {
        CommonMarkViewer::new().show(ui, &mut self.cache, &self.markdown);
    }

    fn id(&self) -> Id {
        self.id
    }
}

pub struct CodeCell {
    id: Id,
    function: Box<dyn FnMut(&mut egui::Ui) -> ()>,
    code: Option<String>,
}

impl Cell for CodeCell {
    fn view(&mut self, ui: &mut egui::Ui) -> () {
        let id = self.id();

        (self.function)(ui);

        if let Some(code) = &mut self.code {
            CollapsingHeader::new("Code")
                .id_salt(format!("{:x}/code", id))
                .show(ui, |ui| {
                    let language = "rs";
                    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(
                        ui.ctx(),
                        ui.style(),
                    );
                    egui_extras::syntax_highlighting::code_view_ui(ui, &theme, code, language);
                });
        }
    }

    fn id(&self) -> Id {
        self.id
    }
}

pub fn code_cell(
    function: impl FnMut(&mut egui::Ui) -> () + 'static,
    code: Option<&str>,
) -> Box<dyn Cell> {
    Box::new(CodeCell {
        id: *fucid(),
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
    })
}

#[macro_export]
macro_rules! code {
    ($code:expr) => {
        $crate::code_cell($code, Some(stringify!($code)))
    };
}

pub struct ReactiveCell<T> {
    id: Id,
    current: Option<T>,
    function: Box<dyn FnMut(&mut egui::Ui, Option<T>) -> T>,
    code: Option<String>,
}

impl<T: std::fmt::Debug> Cell for ReactiveCell<T> {
    fn view(&mut self, ui: &mut egui::Ui) -> () {
        let id = self.id();

        let new = (self.function)(ui, self.current.take());
        self.current = Some(new);

        if let Some(code) = &mut self.code {
            CollapsingHeader::new("Code")
                .id_salt(format!("{:x}/code", id))
                .show(ui, |ui| {
                    let language = "rs";
                    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(
                        ui.ctx(),
                        ui.style(),
                    );
                    egui_extras::syntax_highlighting::code_view_ui(ui, &theme, code, language);
                });
        }

        if let Some(current) = &self.current {
            CollapsingHeader::new("Current")
                .id_salt(format!("{:x}/current", id))
                .show(ui, |ui| {
                    ui.monospace(format!("{:?}", current));
            });
        }
    }

    fn id(&self) -> Id {
        self.id
    }
}

pub fn reactive_cell<T: std::fmt::Debug + 'static>(
    function: impl FnMut(&mut egui::Ui, Option<T>) -> T + 'static,
    code: Option<&str>,
) -> Box<dyn Cell> {
    Box::new(ReactiveCell {
        id: *fucid(),
        current: None,
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
    })
}

#[macro_export]
macro_rules! reactive {
    ($code:expr) => {
        $crate::reactive_cell($code, Some(stringify!($code)))
    };
}

pub struct Notebook {
    pub cells: Vec<Box<dyn Cell>>,
}

pub fn md(markdown: &str) -> Box<dyn Cell> {
    Box::new(MarkdownCell {
        id: *fucid(),
        markdown: markdown.to_owned(),
        cache: CommonMarkCache::default(),
    })
}

impl Notebook {
    pub fn new() -> Self {
        Self { cells: Vec::new() }
    }

    pub fn cell(&mut self, cell: Box<dyn Cell>) {
        self.cells.push(cell);
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
                    style.visuals.panel_fill = egui::Color32::from_hex("#FFFFFF").unwrap();
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
                        for cell in &mut self.cells {
                            cell.view(ui);
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
        pub fn main() {
            let mut notebook = Notebook::new();
            $setup(&mut notebook);

            let this_file = file!();
            let filename = std::path::Path::new(this_file)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap();

            notebook.run(filename).unwrap();
        }
    };
}
