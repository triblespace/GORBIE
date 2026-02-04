use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use eframe::egui;
use rand_core06::OsRng;
use triblespace::core::repo::Repository;
use triblespace::core::repo::pile::Pile;
use triblespace::core::value::schemas::hash::Blake3;

/// Stateful wrapper that keeps a `.pile` file open as a TribleSpace repository.
///
/// This is the recommended pattern for live notebooks: keep the `Pile` open in
/// state and repeatedly `pull` + `checkout(prev_head..)` as the underlying pile
/// grows.
pub struct PileRepoState {
    pile_path: String,
    open_path: Option<PathBuf>,
    repo: Option<Repository<Pile<Blake3>>>,
    signing_key: SigningKey,
    last_error: Option<String>,
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
        if let Some(repo) = self.repo.take() {
            let _ = repo.close();
        }
    }

    pub fn open_path(&self) -> Option<&Path> {
        self.open_path.as_deref()
    }

    /// Opens (or reopens) the pile path currently stored in `pile_path`.
    pub fn open(&mut self) -> Result<(), String> {
        let open_path = PathBuf::from(self.pile_path.trim());
        let path_changed = self
            .open_path
            .as_ref()
            .map_or(true, |existing| existing != &open_path);

        if path_changed || self.repo.is_none() {
            self.close();
            let mut pile =
                Pile::<Blake3>::open(&open_path).map_err(|err| format!("open pile: {err:?}"))?;
            pile.restore()
                .map_err(|err| format!("restore pile: {err:?}"))?;
            self.repo = Some(Repository::new(pile, self.signing_key.clone()));
            self.open_path = Some(open_path);
        }

        self.last_error = None;
        Ok(())
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
}

impl<'a> PileRepoWidget<'a> {
    pub fn new(state: &'a mut PileRepoState) -> Self {
        Self { state }
    }

    pub fn show(self, ui: &mut egui::Ui) -> PileRepoResponse<'a> {
        let mut opened = false;
        let mut closed = false;

        ui.horizontal(|ui| {
            // Match heights across our LCD-style text field and the shadowed buttons by
            // temporarily bumping `interact_size.y` for this row.
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

                // Place the fixed-width buttons first (right-to-left), then give the path field
                // the remaining space.
                // Align to the top so the LCD field visually lines up with the button "body"
                // (buttons reserve extra space at the bottom/right for their drop shadow).
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui
                        .add_enabled(self.state.is_open(), super::super::Button::new("Close"))
                        .clicked()
                    {
                        self.state.close();
                        closed = true;
                    }

                    if ui.add(super::super::Button::new("Open")).clicked() {
                        if let Err(err) = self.state.open() {
                            self.state.last_error = Some(err);
                        } else {
                            opened = true;
                        }
                    }

                    // Reserve space for the label so the LCD field doesn't eat it in a
                    // right-to-left layout.
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

                    // Center the label vertically so it aligns with our taller LCD field.
                    let (label_rect, _) = ui.allocate_exact_size(
                        egui::vec2(label_w, target_h),
                        egui::Sense::hover(),
                    );
                    if ui.is_rect_visible(label_rect) {
                        // Align the label to the center of the LCD field, so we look visually
                        // centered even when different fonts have different baseline metrics.
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
