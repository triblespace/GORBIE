use eframe::egui::{self, Response, Widget};
use crate::egui::{FontFamily, FontData, FontDefinitions, FontId, TextStyle};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use ctrlc;
use std::{collections::BTreeMap, sync::Arc};

pub trait Cell {
    fn view(&mut self, ui: &mut egui::Ui) -> ();
}

pub struct MarkdownCell {
    markdown: String,
    cache: CommonMarkCache,
}

impl Cell for MarkdownCell {
    fn view(&mut self, ui: &mut egui::Ui) -> () {
        CommonMarkViewer::new().show(ui, &mut self.cache, &self.markdown);
    }
}

pub struct CodeCell {
    function: Box<dyn FnMut(&mut egui::Ui) -> ()>,
    code: Option<String>,
}

impl Cell for CodeCell {
    fn view(&mut self, ui: &mut egui::Ui) -> () {
        (self.function)(ui);

        if let Some(code) = &mut self.code {
            ui.collapsing("Code", |ui| {
                let language = "rs";
                let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx(), ui.style());
                egui_extras::syntax_highlighting::code_view_ui(ui, &theme, code, language);
            });
        }
    }
}

pub fn code(function: impl FnMut(&mut egui::Ui) -> () + 'static, code: Option<&str>) -> Box<dyn Cell> {
    Box::new(CodeCell {
        function: Box::new(function),
        code: code.map(|s| s.to_owned())
    })
}

pub struct Notebook {
    cells: Vec<Box<dyn Cell>>,
}

pub fn md(markdown: &str) -> Box<dyn Cell> {
    Box::new(MarkdownCell {
        markdown: markdown.to_owned(),
        cache: CommonMarkCache::default(),
    })
}

impl Notebook {
    fn new(cc: &eframe::CreationContext<'_>, cells: Vec<Box<dyn Cell>>) -> Self {
        let ctx = cc.egui_ctx.clone();
        ctrlc::set_handler(move || ctx.send_viewport_cmd(egui::ViewportCommand::Close))
        .expect("failed to set exit signal handler");

        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert("lora".to_owned(),
           std::sync::Arc::new(
               FontData::from_static(include_bytes!("../assets/fonts/Lora/Lora-VariableFont_wght.ttf"))
           )
        );
        fonts.font_data.insert("atkinson".to_owned(),
        std::sync::Arc::new(
            FontData::from_static(include_bytes!("../assets/fonts/Atkinson_Hyperlegible_Next/AtkinsonHyperlegibleNext-VariableFont_wght.ttf"))
        )
        );
        fonts.font_data.insert("roboto_mono".to_owned(),
        std::sync::Arc::new(
            FontData::from_static(include_bytes!("../assets/fonts/Roboto_Mono/RobotoMono-VariableFont_wght.ttf"))
        )
        );
        fonts.families.get_mut(&FontFamily::Proportional).unwrap()
            .insert(0, "atkinson".to_owned());
        fonts.families.get_mut(&FontFamily::Monospace).unwrap()
            .insert(0, "roboto_mono".to_owned());

        fonts.families.insert(FontFamily::Name("lora".into()), vec!["lora".into()]);
        fonts.families.insert(FontFamily::Name("atkinson".into()), vec!["atkinson".into()]);
        fonts.families.insert(FontFamily::Name("roboto_mono".into()), vec!["roboto_mono".into()]);
        
        cc.egui_ctx.set_fonts(fonts);

        let text_styles: BTreeMap<_, _> = [
            (TextStyle::Heading, FontId::new(32.0, FontFamily::Name("lora".into()))),
            (TextStyle::Body, FontId::new(16.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(16.0, FontFamily::Monospace)),
            (TextStyle::Button, FontId::new(16.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        ].into();

        cc.egui_ctx.all_styles_mut(move |style| {
            style.text_styles = text_styles.clone();
            style.visuals.panel_fill = egui::Color32::from_hex("#FFFFFF").unwrap();
        });


        Self {
            cells,
        }
    }
}

impl eframe::App for Notebook {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
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

pub fn run_notebook(cells: Vec<Box<dyn Cell>>) {
    let mut native_options = eframe::NativeOptions::default();
    native_options.persist_window = true;
    let _ = eframe::run_native(
        "Notebook",
        native_options,
        Box::new(|cc| Ok(Box::new(Notebook::new(cc, cells)))),
    );
}

#[macro_export]
macro_rules! code {
    ($code:expr) => {
        $crate::code($code, Some(stringify!($code)))
    };
}

#[macro_export]
macro_rules! notebook {
    ($($cell:expr),*) => {
        pub fn main() {
            $crate::run_notebook(vec![$($cell),*]);
        }
    };
}