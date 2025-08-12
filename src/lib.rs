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
pub mod themes;

pub use cards::*;
use ctrlc;
pub use dataflow::*;
use eframe::egui::{self};
use egui_theme_switch::global_theme_switch;

use crate::themes::{cosmic_gel_dark, cosmic_gel_fonts, cosmic_gel_light};
use parking_lot::Mutex;
use std::sync::Arc;
use egui_commonmark::CommonMarkCache;

/// A notebook is a collection of cards.
/// Each card is a piece of content that can be displayed in the notebook.
/// Cards can be stateless, stateful, or reactively derived from other cards.
pub struct Notebook {
    pub cards: Vec<Box<dyn Card + 'static>>,
    // Shared markdown cache for inline rendering (Arc so clones are cheap)
    pub commonmark_cache: Arc<Mutex<CommonMarkCache>>,
}

impl Notebook {
    pub fn new() -> Self {
        Self { cards: Vec::new(), commonmark_cache: Arc::new(Mutex::new(CommonMarkCache::default())) }
    }

    pub fn push(&mut self, card: Box<dyn Card>) {
        self.cards.push(card);
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
                        frame.content_ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Min),
                            |ui| {
                                global_theme_switch(ui);
                            },
                        );
                    }
                    frame.end(ui);

                    let frame = egui::Frame::default().begin(ui);
                    {
                        ui.vertical_centered(|ui| {
                            ui.set_max_width(740.0);
                            for (i, card) in self.cards.iter_mut().enumerate() {
                                ui.push_id(i, |ui| {
                                    let card: &mut (dyn Card) = card.as_mut();
                                    ui.add(card);
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
