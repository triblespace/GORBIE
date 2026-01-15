//! ## Working with mutable/non-cloneable things.
//! Sometimes when working with existing code, libraries or even std things like
//! files, can introduce an impedance mismatch with a dataflow-style model.
//! Often it is enough to wrap the object in question into another layer of `Arc`s
//! and `RWLock`s in addition to what Gorby already does with its shared state
//! store.
//!
//! For heavier work, `ComputedState` can run background tasks and hold the latest
//! value. Use `Option<T>` when a value may be absent while a computation runs.
//!
//! But sometimes that isn't enough, e.g. when you want to display some application
//! global state. This is why `Notebook::state` and `Notebook::view` are carefully
//! designed to stay independent from any dataflow runtime. Instead they can be used,
//! like any other mutable rust type, via the typed `StateId` handle.
//!

#![allow(non_snake_case)]

pub mod cards;
pub mod dataflow;
pub mod prelude;
pub mod state;
pub mod themes;
pub mod widgets;

pub use gorbie_macros::notebook;

use crate::themes::industrial_dark;
use crate::themes::industrial_fonts;
use crate::themes::industrial_light;
use eframe::egui::{self};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FloatingAnchor {
    Content,
    Viewport,
}

enum FloatingElement {
    DetachedCard(DetachedCardDraw),
}

struct DetachedCardDraw {
    index: usize,
    area_id: egui::Id,
    width: f32,
}

#[derive(Clone, Debug)]
struct SourceLocation {
    file: String,
    line: u32,
    column: u32,
}

impl SourceLocation {
    fn from_location(location: &'static std::panic::Location<'static>) -> Self {
        Self {
            file: location.file().to_string(),
            line: location.line(),
            column: location.column(),
        }
    }

    fn format_arg(&self, template: &str) -> String {
        let file = &self.file;
        let line = self.line;
        let column = self.column;
        template
            .replace("{file}", file)
            .replace("{line}", &line.to_string())
            .replace("{column}", &column.to_string())
    }

    fn file_line_column(&self) -> String {
        let file = &self.file;
        let line = self.line;
        let column = self.column;
        format!("{file}:{line}:{column}")
    }
}

#[derive(Clone, Debug)]
pub struct EditorCommand {
    program: String,
    args: Vec<String>,
}

impl EditorCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    fn open(&self, source: &SourceLocation) -> std::io::Result<()> {
        let mut cmd = Command::new(&self.program);
        if self.args.is_empty() {
            cmd.arg(source.file_line_column());
        } else {
            for arg in &self.args {
                cmd.arg(source.format_arg(arg));
            }
        }
        let _child = cmd.spawn()?;
        Ok(())
    }
}

struct CardEntry {
    card: Box<dyn cards::Card + 'static>,
    source: Option<SourceLocation>,
}

#[derive(Clone, Default)]
struct NotebookState {
    card_detached: Vec<bool>,
    card_detached_positions: Vec<egui::Pos2>,
    card_detached_anchors: Vec<FloatingAnchor>,
    card_placeholder_sizes: Vec<egui::Vec2>,
}

impl NotebookState {
    fn sync_len(&mut self, len: usize) {
        self.card_detached.resize(len, false);
        self.card_detached_positions.resize(len, egui::Pos2::ZERO);
        self.card_detached_anchors
            .resize(len, FloatingAnchor::Content);
        self.card_placeholder_sizes
            .resize(len, egui::Vec2::ZERO);
    }
}

/// Configuration for a notebook application.
pub struct NotebookConfig {
    title: String,
    editor: Option<EditorCommand>,
}

struct NotebookApp {
    config: NotebookConfig,
    body: Box<dyn FnMut(&mut Notebook)>,
}

/// Frame-scoped notebook builder used to collect cards in immediate mode.
pub struct Notebook {
    state_id: egui::Id,
    cards: Vec<CardEntry>,
}

const NOTEBOOK_COLUMN_WIDTH: f32 = 768.0;
const NOTEBOOK_MIN_HEIGHT: f32 = 360.0;

impl Default for NotebookConfig {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl NotebookConfig {
    pub fn new(name: impl Into<String>) -> Self {
        let title = name.into();
        Self {
            title,
            editor: editor_from_env(),
        }
    }

    pub fn with_editor(mut self, editor: EditorCommand) -> Self {
        self.editor = Some(editor);
        self
    }

    fn state_id(&self) -> egui::Id {
        egui::Id::new(("gorbie_notebook_state", self.title.as_str()))
    }

    pub fn run(self, body: impl FnMut(&mut Notebook) + 'static) -> eframe::Result {
        let config = self;
        let window_title = if config.title.is_empty() {
            "GORBIE".to_owned()
        } else {
            config.title.clone()
        };

        let mut native_options = eframe::NativeOptions::default();
        native_options.persist_window = true;
        native_options.viewport = native_options
            .viewport
            .with_inner_size(egui::vec2(1200.0, 800.0))
            .with_min_inner_size(egui::vec2(NOTEBOOK_COLUMN_WIDTH, NOTEBOOK_MIN_HEIGHT));

        let body = Box::new(body);
        eframe::run_native(
            &window_title,
            native_options,
            Box::new(|cc| {
                let ctx = cc.egui_ctx.clone();
                ctrlc::set_handler(move || ctx.send_viewport_cmd(egui::ViewportCommand::Close))
                    .expect("failed to set exit signal handler");

                cc.egui_ctx.set_fonts(industrial_fonts());

                cc.egui_ctx
                    .set_style_of(egui::Theme::Light, industrial_light());
                cc.egui_ctx
                    .set_style_of(egui::Theme::Dark, industrial_dark());

                Ok(Box::new(NotebookApp { config, body }))
            }),
        )
    }
}

fn editor_from_env() -> Option<EditorCommand> {
    let editor = std::env::var("GORBIE_EDITOR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("VISUAL")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })?;

    let mut parts = editor.split_whitespace();
    let program = parts.next()?.to_string();
    let args = parts.map(str::to_string);
    let mut command = EditorCommand::new(program);
    for arg in args {
        command = command.arg(arg);
    }
    Some(command)
}

impl Notebook {
    fn new(config: &NotebookConfig) -> Self {
        Self {
            state_id: config.state_id(),
            cards: Vec::new(),
        }
    }

    #[track_caller]
    pub fn view(&mut self, function: impl FnMut(&mut egui::Ui) + 'static) {
        let source = SourceLocation::from_location(std::panic::Location::caller());
        let card = cards::StatelessCard::new(function);
        self.push_with_source(Box::new(card), Some(source));
    }

    #[track_caller]
    pub fn state<K, T>(
        &mut self,
        key: &K,
        init: T,
        function: impl FnMut(&mut egui::Ui, &mut T) + 'static,
    ) -> state::StateId<T>
    where
        K: std::hash::Hash + ?Sized,
        T: std::fmt::Debug + std::default::Default + Send + Sync + 'static,
    {
        let source = SourceLocation::from_location(std::panic::Location::caller());
        let state = state::StateId::new(self.state_id_for(key));
        let handle = state;
        let card = cards::StatefulCard::new(state, init, function);
        self.push_with_source(Box::new(card), Some(source));
        handle
    }

    pub fn push(&mut self, card: Box<dyn cards::Card>) {
        self.push_with_source(card, None);
    }

    pub(crate) fn state_id_for<K: std::hash::Hash + ?Sized>(&self, key: &K) -> egui::Id {
        self.state_id.with(("state", key))
    }

    fn push_with_source(
        &mut self,
        card: Box<dyn cards::Card>,
        source: Option<SourceLocation>,
    ) {
        self.cards.push(CardEntry { card, source });
    }
}

impl eframe::App for NotebookApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let config = &self.config;
        let mut notebook = Notebook::new(config);
        (self.body)(&mut notebook);

        let state_id = config.state_id();
        let mut runtime = ctx.data_mut(|data| {
            let slot = data.get_temp_mut_or_insert_with(state_id, NotebookState::default);
            std::mem::take(slot)
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show_viewport(ui, |ui, viewport| {
                    let rect = ui.max_rect();
                    let clip_rect = ui.clip_rect();
                        let scroll_y = viewport.min.y;

                    let column_width = NOTEBOOK_COLUMN_WIDTH;
                    let left_margin_width = 0.0;
                    let card_width = column_width;

                    let left_margin_paint = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, clip_rect.min.y),
                        egui::pos2(rect.min.x + left_margin_width, clip_rect.max.y),
                    );
                    let left_margin = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(rect.min.x + left_margin_width, rect.max.y),
                    );
                    let column_rect = egui::Rect::from_min_max(
                        egui::pos2(left_margin.max.x, rect.min.y),
                        egui::pos2(left_margin.max.x + column_width, rect.max.y),
                    );
                    let right_margin_paint = egui::Rect::from_min_max(
                        egui::pos2(column_rect.max.x, clip_rect.min.y),
                        egui::pos2(rect.max.x, clip_rect.max.y),
                    );
                    let right_margin = egui::Rect::from_min_max(
                        egui::pos2(column_rect.max.x, rect.min.y),
                        rect.max,
                    );

                    paint_dot_grid(ui, left_margin_paint, scroll_y);
                    paint_dot_grid(ui, right_margin_paint, scroll_y);

                    ui.scope_builder(egui::UiBuilder::new().max_rect(column_rect), |ui| {
                        ui.set_min_size(column_rect.size());
                        ui.set_max_width(column_rect.width());

                        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                        let fill = ui.visuals().window_fill;

                        let column_inner_margin = egui::Margin::symmetric(0, 12);

                        egui::Frame::new()
                            .fill(fill)
                            .stroke(stroke)
                            .corner_radius(0.0)
                            .inner_margin(column_inner_margin)
                            .show(ui, |ui| {
                                // Theme switch is part of the page header (above the first card).
                                ui.horizontal(|ui| {
                                    ui.add_space(16.0);
                                    if !config.title.is_empty() {
                                        let header_title =
                                            egui::RichText::new(config.title.to_uppercase())
                                                .monospace()
                                                .strong();
                                        ui.add(egui::Label::new(header_title).truncate());
                                    }

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.add_space(16.0);
                                            let mut preference =
                                                ui.ctx().options(|opt| opt.theme_preference);
                                            if ui
                                                .add(
                                                    widgets::ChoiceToggle::new(&mut preference)
                                                        .choice(egui::ThemePreference::System, "◐")
                                                        .choice(egui::ThemePreference::Dark, "●")
                                                        .choice(egui::ThemePreference::Light, "○"),
                                                )
                                                .changed()
                                            {
                                                ui.ctx().set_theme(preference);
                                            }
                                        },
                                    );
                                });

                                ui.add_space(12.0);

                                let divider_x_range = ui.max_rect().x_range();
                                runtime.sync_len(notebook.cards.len());

                                ui.style_mut().spacing.item_spacing.y = 0.0;
                                let mut floating_elements: Vec<FloatingElement> = Vec::new();
                                let mut dragged_layer_ids: Vec<egui::LayerId> = Vec::new();
                                for (i, entry) in notebook.cards.iter_mut().enumerate() {
                                    let card_detached = runtime.card_detached
                                        .get_mut(i)
                                        .expect("card_detached synced to cards");
                                    let card_detached_position = runtime.card_detached_positions
                                        .get_mut(i)
                                        .expect("card_detached_positions synced to cards");
                                    let card_detached_anchor = runtime.card_detached_anchors
                                        .get_mut(i)
                                        .expect("card_detached_anchors synced to cards");
                                    let card_placeholder_size = runtime.card_placeholder_sizes
                                        .get_mut(i)
                                        .expect("card_placeholder_sizes synced to cards");
                                    ui.push_id(i, |ui| {
                                        let card: &mut dyn cards::Card = entry.card.as_mut();
                                        let card_rect = if *card_detached {
                                            let placeholder_height =
                                                if card_placeholder_size.y > 0.0 {
                                                    card_placeholder_size.y
                                                } else {
                                                    120.0
                                                };
                                            let placeholder_width = card_width;
                                            let (rect, resp) = ui.allocate_exact_size(
                                                egui::vec2(placeholder_width, placeholder_height),
                                                egui::Sense::click(),
                                            );
                                            let fill = ui.visuals().window_fill;
                                            let outline =
                                                ui.visuals().widgets.noninteractive.bg_stroke.color;
                                            ui.painter().rect_filled(rect, 0.0, fill);
                                            paint_hatching(
                                                &ui.painter().with_clip_rect(rect),
                                                rect,
                                                outline,
                                            );
                                            show_postit_tooltip(ui, &resp, "Dock card");
                                            if resp.clicked() {
                                                *card_detached = false;
                                            }

                                            if *card_detached {
                                                let detached_card_width =
                                                    card_width.max(240.0);
                                                if *card_detached_position == egui::Pos2::ZERO {
                                                    let initial_screen_pos = egui::pos2(
                                                        right_margin.min.x + 12.0,
                                                        rect.top(),
                                                    );
                                                    *card_detached_anchor =
                                                        FloatingAnchor::Content;
                                                    *card_detached_position = screen_to_content_pos(
                                                        initial_screen_pos,
                                                        scroll_y,
                                                        clip_rect.min.y,
                                                    );
                                                }

                                                let detached_id = ui.id().with("detached_card");
                                                floating_elements.push(FloatingElement::DetachedCard(
                                                    DetachedCardDraw {
                                                        index: i,
                                                        area_id: detached_id,
                                                        width: detached_card_width,
                                                    },
                                                ));
                                            }
                                            rect
                                        } else {
                                            let inner = egui::Frame::group(ui.style())
                                                .stroke(egui::Stroke::NONE)
                                                .corner_radius(0.0)
                                                .inner_margin(egui::Margin::ZERO)
                                                .show(ui, |ui| {
                                                    ui.reset_style();
                                                    ui.set_width(card_width);
                                                    card.draw(ui);
                                                });
                                            *card_placeholder_size = egui::vec2(
                                                card_width,
                                                inner.response.rect.height(),
                                            );
                                            inner.response.rect
                                        };
                                        ui.painter().hline(
                                            divider_x_range,
                                            card_rect.top(),
                                            ui.visuals().widgets.noninteractive.bg_stroke,
                                        );

                                        let button_size = egui::vec2(18.0, 18.0);
                                        let button_gap = 6.0;
                                        let button_x = (card_rect.right() + 8.0).round();
                                        let show_detach_button = !*card_detached;
                                        let show_open_button = show_detach_button
                                            && entry.source.is_some()
                                            && config.editor.is_some();
                                        let button_count =
                                            usize::from(show_open_button) + 1;
                                        let total_height = button_size.y
                                            * button_count as f32
                                            + button_gap * (button_count.saturating_sub(1) as f32);
                                        let top_y = (card_rect.center().y
                                            - total_height / 2.0)
                                            .round();
                                        let open_pos = show_open_button.then(|| {
                                            egui::pos2(button_x, top_y)
                                        });
                                        let detach_pos = Some(egui::pos2(
                                            button_x,
                                            (top_y
                                                + if show_open_button {
                                                    button_size.y + button_gap
                                                } else {
                                                    0.0
                                                })
                                            .round(),
                                        ));

                                        if let Some(open_pos) = open_pos {
                                            let open_id = ui.id().with("open_button");
                                            let open_area = egui::Area::new(open_id)
                                                .order(egui::Order::Middle)
                                                .fixed_pos(open_pos)
                                                .movable(false)
                                                .constrain_to(egui::Rect::EVERYTHING);
                                            let open_resp = open_area.show(ui.ctx(), |ui| {
                                                let (rect, resp) = ui.allocate_exact_size(
                                                    button_size,
                                                    egui::Sense::click(),
                                                );
                                                let fill = ui.visuals().window_fill;
                                                let outline = ui
                                                    .visuals()
                                                    .widgets
                                                    .noninteractive
                                                    .bg_stroke
                                                    .color;
                                                let accent =
                                                    ui.visuals().selection.stroke.color;
                                                let stroke_color =
                                                    if resp.hovered() || resp.has_focus() {
                                                        accent
                                                    } else {
                                                        outline
                                                    };
                                                let stroke =
                                                    egui::Stroke::new(1.0, stroke_color);

                                                ui.painter().rect_filled(rect, 0.0, fill);
                                                ui.painter().rect_stroke(
                                                    rect,
                                                    0.0,
                                                    stroke,
                                                    egui::StrokeKind::Inside,
                                                );
                                                ui.painter().text(
                                                    rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    "<>",
                                                    egui::FontId::monospace(10.0),
                                                    ui.visuals().text_color(),
                                                );

                                                if let Some(source) = entry.source.as_ref() {
                                                    let file = &source.file;
                                                    let line = source.line;
                                                    let tooltip =
                                                        format!("Open in editor\n{file}:{line}");
                                                    show_postit_tooltip(ui, &resp, &tooltip);
                                                } else {
                                                    show_postit_tooltip(
                                                        ui,
                                                        &resp,
                                                        "Open in editor",
                                                    );
                                                }
                                                resp
                                            });

                                            if open_resp.inner.clicked() {
                                                if let Some(source) = entry.source.as_ref() {
                                                    if let Some(editor) =
                                                        config.editor.as_ref()
                                                    {
                                                        if let Err(err) = editor.open(source) {
                                                            log::warn!(
                                                                "failed to open editor: {err}"
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        if let Some(detach_pos) = detach_pos {
                                            let detach_id = ui.id().with("detach_button");
                                            let detach_area = egui::Area::new(detach_id)
                                                .order(egui::Order::Middle)
                                                .fixed_pos(detach_pos)
                                                .movable(false)
                                                .constrain_to(egui::Rect::EVERYTHING);
                                            if show_detach_button {
                                                let detach_resp = detach_area.show(ui.ctx(), |ui| {
                                                    let (rect, resp) = ui.allocate_exact_size(
                                                        button_size,
                                                        egui::Sense::click(),
                                                    );
                                                    let fill = ui.visuals().window_fill;
                                                    let outline = ui
                                                        .visuals()
                                                        .widgets
                                                        .noninteractive
                                                        .bg_stroke
                                                        .color;
                                                    let accent =
                                                        ui.visuals().selection.stroke.color;
                                                    let stroke_color =
                                                        if resp.hovered() || resp.has_focus() {
                                                            accent
                                                        } else {
                                                            outline
                                                        };
                                                    let stroke =
                                                        egui::Stroke::new(1.0, stroke_color);

                                                    ui.painter().rect_filled(rect, 0.0, fill);
                                                    ui.painter().rect_stroke(
                                                        rect,
                                                        0.0,
                                                        stroke,
                                                        egui::StrokeKind::Inside,
                                                    );
                                                    ui.painter().text(
                                                        rect.center(),
                                                        egui::Align2::CENTER_CENTER,
                                                        "[]",
                                                        egui::FontId::monospace(10.0),
                                                        ui.visuals().text_color(),
                                                    );

                                                    let tooltip = if *card_detached {
                                                        "Dock card"
                                                    } else {
                                                        "Detach card"
                                                    };
                                                    show_postit_tooltip(ui, &resp, tooltip);
                                                    resp
                                                });

                                                if detach_resp.inner.clicked() {
                                                    if *card_detached {
                                                        *card_detached = false;
                                                    } else {
                                                        *card_detached = true;
                                                        *card_detached_anchor =
                                                            FloatingAnchor::Content;
                                                        let initial_screen_pos = egui::pos2(
                                                            right_margin.min.x + 12.0,
                                                            card_rect.top(),
                                                        );
                                                        *card_detached_position =
                                                            screen_to_content_pos(
                                                                initial_screen_pos,
                                                                scroll_y,
                                                                clip_rect.min.y,
                                                            );
                                                    }
                                                }
                                            } else {
                                                detach_area.show(ui.ctx(), |ui| {
                                                    ui.allocate_exact_size(
                                                        button_size,
                                                        egui::Sense::hover(),
                                                    );
                                                });
                                            }
                                        }

                                    });
                                }

                                for pass_anchor in
                                    [FloatingAnchor::Content, FloatingAnchor::Viewport]
                                {
                                    for element in floating_elements.iter() {
                                        match element {
                                            FloatingElement::DetachedCard(draw) => {
                                                let current_anchor = *runtime.card_detached_anchors
                                                    .get(draw.index)
                                                    .expect(
                                                        "card_detached_anchors synced to cards",
                                                    );
                                                if current_anchor != pass_anchor {
                                                    continue;
                                                }

                                                let card_detached = runtime.card_detached
                                                    .get_mut(draw.index)
                                                    .expect("card_detached synced to cards");
                                                if !*card_detached {
                                                    continue;
                                                }

                                                let card_detached_position = runtime.card_detached_positions
                                                    .get_mut(draw.index)
                                                    .expect(
                                                        "card_detached_positions synced to cards",
                                                    );
                                                let card_detached_anchor = runtime.card_detached_anchors
                                                    .get_mut(draw.index)
                                                    .expect(
                                                        "card_detached_anchors synced to cards",
                                                    );

                                                let detached_screen_pos =
                                                    match *card_detached_anchor {
                                                        FloatingAnchor::Content => {
                                                            content_to_screen_pos(
                                                                *card_detached_position,
                                                                scroll_y,
                                                                clip_rect.min.y,
                                                            )
                                                        }
                                                        FloatingAnchor::Viewport => {
                                                            *card_detached_position
                                                        }
                                                    };

                                                let card_width = draw.width;
                                                let detached_id = draw.area_id;
                                                let card: &mut dyn cards::Card = notebook
                                                    .cards
                                                    .get_mut(draw.index)
                                                    .expect("cards synced to floating_elements")
                                                    .card
                                                    .as_mut();

                                                let area_order = match pass_anchor {
                                                    FloatingAnchor::Content => {
                                                        egui::Order::Foreground
                                                    }
                                                    FloatingAnchor::Viewport => egui::Order::Tooltip,
                                                };
                                                let detached_area =
                                                    egui::Area::new(detached_id)
                                                        .order(area_order)
                                                        .fixed_pos(detached_screen_pos)
                                                        .movable(false)
                                                        .constrain_to(egui::Rect::EVERYTHING);
                                                detached_area.show(ui.ctx(), |ui| {
                                                        let outline = ui
                                                            .visuals()
                                                            .widgets
                                                            .noninteractive
                                                            .bg_stroke
                                                            .color;
                                                        let shadow_color =
                                                            crate::themes::ral(9004);
                                                        let shadow = egui::epaint::Shadow {
                                                            offset: [6, 6],
                                                            blur: 0,
                                                            spread: 0,
                                                            color: shadow_color,
                                                        };

                                                        ui.set_width(card_width);
                                                        let frame = egui::Frame::new()
                                                            .fill(ui.visuals().window_fill)
                                                            .stroke(egui::Stroke::new(
                                                                1.0, outline,
                                                            ))
                                                            .shadow(shadow)
                                                            .corner_radius(0.0)
                                                            .inner_margin(egui::Margin::ZERO);

                                                        let inner = frame.show(ui, |ui| {
                                                            ui.reset_style();
                                                            ui.set_width(card_width);
                                                            card.draw(ui);
                                                        });

                                                        let handle_height = 18.0;
                                                        let handle_rect = egui::Rect::from_min_size(
                                                            inner.response.rect.min,
                                                            egui::vec2(
                                                                inner.response.rect.width(),
                                                                handle_height,
                                                            ),
                                                        );
                                                        let handle_id =
                                                            ui.id().with("detached_handle");
                                                        let handle_resp = ui.interact(
                                                            handle_rect,
                                                            handle_id,
                                                            egui::Sense::click_and_drag(),
                                                        );

                                                        if handle_resp.dragged() {
                                                            ui.ctx().set_cursor_icon(
                                                                egui::CursorIcon::Grabbing,
                                                            );
                                                        } else if handle_resp.hovered() {
                                                            ui.ctx().set_cursor_icon(
                                                                egui::CursorIcon::Grab,
                                                            );
                                                        }

                                                        if handle_resp.hovered()
                                                            || handle_resp.dragged()
                                                        {
                                                            let stripe_color =
                                                                crate::themes::ral(9004);
                                                            let stripe_stroke = egui::Stroke::new(
                                                                1.0,
                                                                stripe_color,
                                                            );
                                                            let stripe_x = handle_rect.x_range();
                                                            let stripe_padding = 3.0;
                                                            let stripe_spacing = 3.0;
                                                            let mut stripe_y = handle_rect.top()
                                                                + stripe_padding;
                                                            let painter = ui
                                                                .painter()
                                                                .with_clip_rect(handle_rect);
                                                            while stripe_y
                                                                <= handle_rect.bottom()
                                                                    - stripe_padding
                                                            {
                                                                painter.hline(
                                                                    stripe_x,
                                                                    stripe_y,
                                                                    stripe_stroke,
                                                                );
                                                                stripe_y += stripe_spacing;
                                                            }

                                                            show_postit_tooltip(
                                                                ui,
                                                                &handle_resp,
                                                                "Dock card",
                                                            );
                                                        }

                                                        {
                                                            if handle_resp.dragged() {
                                                                ui.ctx().move_to_top(
                                                                    handle_resp.layer_id,
                                                                );
                                                                dragged_layer_ids
                                                                    .push(handle_resp.layer_id);
                                                                let delta =
                                                                    handle_resp.drag_delta();
                                                                let moved_rect = inner
                                                                    .response
                                                                    .rect
                                                                    .translate(delta);
                                                                *card_detached_position += delta;

                                                                match *card_detached_anchor {
                                                                    FloatingAnchor::Content => {
                                                                        if right_outside_ratio(
                                                                            moved_rect,
                                                                            clip_rect,
                                                                        )
                                                                            >= STICK_RIGHT_OUTSIDE_RATIO
                                                                        {
                                                                            *card_detached_anchor =
                                                                                FloatingAnchor::Viewport;
                                                                            *card_detached_position =
                                                                                moved_rect.min;
                                                                        }
                                                                    }
                                                                    FloatingAnchor::Viewport => {
                                                                        if right_outside_ratio(
                                                                            moved_rect,
                                                                            clip_rect,
                                                                        )
                                                                            <= UNSTICK_RIGHT_OUTSIDE_RATIO
                                                                        {
                                                                            *card_detached_anchor =
                                                                                FloatingAnchor::Content;
                                                                            *card_detached_position =
                                                                                screen_to_content_pos(
                                                                                    moved_rect.min,
                                                                                    scroll_y,
                                                                                    clip_rect.min.y,
                                                                                );
                                                                        }
                                                                    }
                                                                }
                                                            }

                                                            if handle_resp.clicked() {
                                                                *card_detached = false;
                                                            }
                                                        }
                                                    });
                                            }
                                        }
                                    }
                                }

                                for layer_id in dragged_layer_ids {
                                    ui.ctx().move_to_top(layer_id);
                                }

                            });
                    });
                });
        });

        ctx.data_mut(|data| {
            data.insert_temp(state_id, runtime);
        });
    }
}

fn paint_dot_grid(ui: &egui::Ui, rect: egui::Rect, scroll_y: f32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    let painter = ui.painter_at(rect);

    let spacing = 18.0;
    let radius = 1.2;
    let background = ui.visuals().window_fill;
    let outline = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let color = crate::themes::blend(background, outline, 0.35);

    let start_x = (rect.left() / spacing).floor() * spacing + spacing / 2.0;
    let start_y = rect.top() - scroll_y.rem_euclid(spacing) + spacing / 2.0;

    let mut y = start_y;
    while y < rect.bottom() {
        let mut x = start_x;
        while x < rect.right() {
            painter.circle_filled(egui::pos2(x, y), radius, color);
            x += spacing;
        }
        y += spacing;
    }
}

fn paint_hatching(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let spacing = 12.0;
    let stroke = egui::Stroke::new(1.0, color);

    let h = rect.height();
    let mut x = rect.left() - h;
    while x < rect.right() + h {
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x + h, rect.bottom())],
            stroke,
        );
        x += spacing;
    }
}

const STICK_RIGHT_OUTSIDE_RATIO: f32 = 0.5;
const UNSTICK_RIGHT_OUTSIDE_RATIO: f32 = 0.4;

fn screen_to_content_pos(pos: egui::Pos2, scroll_y: f32, viewport_top: f32) -> egui::Pos2 {
    egui::pos2(pos.x, pos.y - viewport_top + scroll_y)
}

fn content_to_screen_pos(pos: egui::Pos2, scroll_y: f32, viewport_top: f32) -> egui::Pos2 {
    egui::pos2(pos.x, pos.y - scroll_y + viewport_top)
}

fn right_outside_ratio(rect: egui::Rect, viewport: egui::Rect) -> f32 {
    let width = rect.width().max(0.0);
    if width <= 0.0 {
        return 1.0;
    }

    let outside_width = (rect.right() - viewport.right()).max(0.0);
    let ratio = outside_width / width;
    ratio.clamp(0.0, 1.0)
}

fn show_postit_tooltip(ui: &egui::Ui, response: &egui::Response, text: &str) {
    let outline = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let shadow_color = crate::themes::ral(9004);
    let shadow = egui::epaint::Shadow {
        offset: [4, 4],
        blur: 0,
        spread: 0,
        color: shadow_color,
    };

    let frame = egui::Frame::new()
        .fill(crate::themes::ral(1003))
        .stroke(egui::Stroke::new(1.0, outline))
        .shadow(shadow)
        .corner_radius(0.0)
        .inner_margin(egui::Margin::same(10));

    let mut tooltip = egui::containers::Tooltip::for_enabled(response);
    tooltip.popup = tooltip.popup.frame(frame);
    tooltip.show(|ui| {
        ui.set_max_width(ui.spacing().tooltip_width);
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .monospace()
                    .color(crate::themes::ral(9011)),
            )
            .wrap_mode(egui::TextWrapMode::Extend),
        );
    });
}

// notebook initialization is handled by the #[notebook] attribute macro.
