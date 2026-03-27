use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use eframe::egui;
use rand_core06::OsRng;
use triblespace::core::repo::Repository;
use triblespace::core::repo::pile::Pile;
use triblespace::core::trible::TribleSet;
use triblespace::core::value::schemas::hash::Blake3;

use crate::dataflow::ComputedState;
use crate::themes::GorbieToggleButtonStyle;
use crate::widgets::Button;

/// Result of a background pile open operation.
struct OpenResult {
    repo: Repository<Pile<Blake3>>,
    path: PathBuf,
}

/// Stateful wrapper that keeps a `.pile` file open as a TribleSpace repository.
///
/// Opening is asynchronous — `open()` spawns a background thread and the
/// widget shows a spinner until it completes. Use `is_open()` / `repo()`
/// to check availability.
pub struct PileRepoState {
    pile_path: String,
    open_path: Option<PathBuf>,
    repo: Option<Repository<Pile<Blake3>>>,
    signing_key: SigningKey,
    last_error: Option<String>,
    opener: ComputedState<Option<Result<OpenResult, String>>>,
}

impl Default for PileRepoState {
    fn default() -> Self {
        Self::new("./repo.pile")
    }
}

impl Drop for PileRepoState {
    fn drop(&mut self) {
        self.close();
    }
}

impl PileRepoState {
    pub fn new(pile_path: impl Into<String>) -> Self {
        Self {
            pile_path: pile_path.into(),
            open_path: None,
            repo: None,
            signing_key: SigningKey::generate(&mut OsRng),
            last_error: None,
            opener: ComputedState::new(None),
        }
    }

    /// Replace the signing key used for newly opened repositories.
    pub fn with_signing_key(mut self, signing_key: SigningKey) -> Self {
        self.signing_key = signing_key;
        self
    }

    pub fn pile_path(&self) -> &str {
        &self.pile_path
    }

    pub fn pile_path_mut(&mut self) -> &mut String {
        &mut self.pile_path
    }

    pub fn is_open(&self) -> bool {
        self.repo.is_some()
    }

    pub fn is_opening(&self) -> bool {
        self.opener.is_running()
    }

    pub fn repo(&self) -> Option<&Repository<Pile<Blake3>>> {
        self.repo.as_ref()
    }

    pub fn repo_mut(&mut self) -> Option<&mut Repository<Pile<Blake3>>> {
        self.repo.as_mut()
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn close(&mut self) {
        self.open_path = None;
        self.last_error = None;
        self.opener = ComputedState::new(None);
        if let Some(repo) = self.repo.take() {
            let _ = repo.close();
        }
    }

    pub fn open_path(&self) -> Option<&Path> {
        self.open_path.as_deref()
    }

    /// Poll the background opener. Call this every frame.
    pub fn poll(&mut self) {
        if self.opener.poll() {
            if let Some(result) = self.opener.value_mut().take() {
                match result {
                    Ok(open_result) => {
                        self.close();
                        self.repo = Some(open_result.repo);
                        self.open_path = Some(open_result.path);
                        self.last_error = None;
                    }
                    Err(err) => {
                        self.last_error = Some(err);
                    }
                }
            }
        }
    }

    /// Start opening the pile on a background thread.
    pub fn open(&mut self) {
        if self.opener.is_running() {
            return;
        }
        let path = PathBuf::from(self.pile_path.trim());
        let signing_key = self.signing_key.clone();
        self.opener.spawn(move || {
            let result = (|| -> Result<OpenResult, String> {
                let mut pile =
                    Pile::<Blake3>::open(&path).map_err(|err| format!("open pile: {err:?}"))?;
                if let Err(err) = pile.restore() {
                    let _ = pile.close();
                    return Err(format!("restore pile: {err:?}"));
                }
                let repo = Repository::new(pile, signing_key, TribleSet::new())
                    .map_err(|err| format!("create repository: {err:?}"))?;
                Ok(OpenResult {
                    repo,
                    path: path.clone(),
                })
            })();
            Some(result)
        });
    }
}

pub struct PileRepoResponse<'a> {
    pub opened: bool,
    pub closed: bool,
    pub repo: Option<&'a mut Repository<Pile<Blake3>>>,
}

/// UI helper that edits `PileRepoState` and offers Open/Close controls.
#[must_use = "Use `PileRepoWidget::show(ui)` to render this widget."]
pub struct PileRepoWidget<'a> {
    state: &'a mut PileRepoState,
    auto_open: bool,
}

impl<'a> PileRepoWidget<'a> {
    pub fn new(state: &'a mut PileRepoState) -> Self {
        Self {
            state,
            auto_open: false,
        }
    }

    /// Auto-open the pile on first render if not already open.
    pub fn auto_open(mut self) -> Self {
        self.auto_open = true;
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> PileRepoResponse<'a> {
        let mut opened = false;
        let mut closed = false;

        // Poll background opener.
        let was_opening = self.state.is_opening();
        self.state.poll();
        if was_opening && !self.state.is_opening() {
            opened = self.state.is_open();
        }

        // Auto-open on first render.
        if self.auto_open && !self.state.is_open() && !self.state.is_opening() && self.state.last_error.is_none() {
            self.state.open();
        }

        if self.state.is_opening() {
            ui.ctx().request_repaint();
        }

        ui.horizontal(|ui| {
            let base_interact_h = ui.spacing().interact_size.y;
            let button_padding_y = ui.spacing().button_padding.y;

            let button_font_id = egui::TextStyle::Button.resolve(ui.style());
            let button_row_h = ui.fonts_mut(|fonts| fonts.row_height(&button_font_id));
            let button_desired_h = button_row_h + button_padding_y * 2.0;

            let lcd_style = egui::TextStyle::Name("LCD".into());
            let lcd_font_id = ui
                .style()
                .text_styles
                .get(&lcd_style)
                .cloned()
                .unwrap_or_else(|| egui::TextStyle::Monospace.resolve(ui.style()));
            let lcd_row_h = ui.fonts_mut(|fonts| fonts.row_height(&lcd_font_id));

            let target_h = base_interact_h.max(button_desired_h).max(lcd_row_h);

            ui.scope(|ui| {
                ui.spacing_mut().interact_size.y = target_h;

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui
                        .add_enabled(self.state.is_open(), super::super::Button::new("Close"))
                        .clicked()
                    {
                        self.state.close();
                        closed = true;
                    }

                    let opening = self.state.is_opening();
                    let open_enabled = !self.state.is_open() && !opening;
                    let style = GorbieToggleButtonStyle::from(ui.style().as_ref());
                    let light_on = crate::themes::button_light_on();
                    let off = style.rail_bg;
                    let light = if opening {
                        let t = ui.input(|input| input.time) as f32;
                        let wave = (t * std::f32::consts::TAU * 0.8).sin() * 0.5 + 0.5;
                        crate::themes::blend(off, light_on, wave)
                    } else {
                        off
                    };
                    let mut active = opening;
                    let button = Button::new("Open").on(&mut active).light(light);
                    if ui.add(button).clicked() && open_enabled {
                        self.state.open();
                        ui.ctx().request_repaint();
                    }

                    let label_text = "Pile:";
                    let label_color = ui.visuals().text_color();
                    let label_galley = ui.fonts_mut(|fonts| {
                        fonts.layout_no_wrap(label_text.to_owned(), lcd_font_id.clone(), label_color)
                    });
                    let label_w = label_galley.size().x;
                    let field_w =
                        (ui.available_width() - label_w - ui.spacing().item_spacing.x).max(0.0);
                    let field_response = ui.add_sized(
                        [field_w, 0.0],
                        super::super::TextField::singleline(&mut self.state.pile_path),
                    );

                    let (label_rect, _) = ui.allocate_exact_size(
                        egui::vec2(label_w, target_h),
                        egui::Sense::hover(),
                    );
                    if ui.is_rect_visible(label_rect) {
                        let anchor_pos =
                            egui::pos2(label_rect.left(), field_response.rect.center().y);
                        let rect =
                            egui::Align2::LEFT_CENTER.anchor_size(anchor_pos, label_galley.size());
                        let galley_pos = rect.min - label_galley.rect.min.to_vec2();
                        ui.painter()
                            .with_clip_rect(label_rect)
                            .galley(galley_pos, label_galley, label_color);
                    }
                });
            });
        });

        if let Some(err) = self.state.last_error.as_deref() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(err)
                    .color(ui.visuals().error_fg_color)
                    .monospace(),
            );
        }

        PileRepoResponse {
            opened,
            closed,
            repo: self.state.repo.as_mut(),
        }
    }
}
