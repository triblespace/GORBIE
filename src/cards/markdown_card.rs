use crate::{Card, Notebook};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

pub struct MarkdownCard {
    markdown: String,
    cache: CommonMarkCache,
}

impl Card for MarkdownCard {
    fn draw(&mut self, ui: &mut egui::Ui) {
        CommonMarkViewer::new().show(ui, &mut self.cache, &self.markdown);
    }
}

pub fn md(nb: &mut Notebook, markdown: &str) {
    nb.push(Box::new(MarkdownCard {
        markdown: markdown.to_owned(),
        cache: CommonMarkCache::default(),
    }));
}
