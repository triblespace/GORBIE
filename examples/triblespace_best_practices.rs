#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = "..", features = ["triblespace"] }
//! egui = "0.33"
//! eframe = "0.33"
//! triblespace = { path = "../../triblespace-rs" }
//! ed25519-dalek = "2"
//! ```

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use egui::{self};
use triblespace::core::blob::schemas::simplearchive::SimpleArchive;
use triblespace::core::id::Id;
use triblespace::core::metadata;
use triblespace::core::repo::pile::Pile;
use triblespace::core::repo::{BlobStore, BlobStoreGet, BlobStoreMeta, BranchStore, Repository};
use triblespace::core::trible::TribleSet;
use triblespace::core::value::schemas::hash::{Blake3, Handle};
use triblespace::core::value::Value;
use triblespace::macros::{find, pattern};

use GORBIE::cards::with_padding;
use GORBIE::notebook;
use GORBIE::widgets;
use GORBIE::widgets::triblespace::{PileRepoState, PileRepoWidget};
use GORBIE::NotebookCtx;

type CommitHandle = Value<Handle<Blake3, SimpleArchive>>;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn fmt_commit_prefix(handle: CommitHandle) -> String {
    let mut out = String::with_capacity(16);
    for byte in handle.raw.iter().take(8) {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[derive(Clone)]
struct BranchInfo {
    id: Id,
    name: String,
}

fn scan_branches(
    repo: &mut Repository<Pile<Blake3>>,
    prefix: &str,
) -> Result<Vec<BranchInfo>, String> {
    repo.storage_mut()
        .refresh()
        .map_err(|err| format!("refresh pile: {err:?}"))?;
    let reader = repo
        .storage_mut()
        .reader()
        .map_err(|err| format!("open pile reader: {err:?}"))?;
    let iter = repo
        .storage_mut()
        .branches()
        .map_err(|err| format!("list branches: {err:?}"))?;

    let mut branches = Vec::new();
    for item in iter {
        let branch_id = item.map_err(|err| format!("branch id: {err:?}"))?;
        let Some(head) = repo
            .storage_mut()
            .head(branch_id)
            .map_err(|err| format!("branch head: {err:?}"))?
        else {
            continue;
        };

        let meta: TribleSet = reader
            .get(head)
            .map_err(|err| format!("branch metadata blob: {err:?}"))?;
        let name = find!(
            (shortname: String),
            pattern!(&meta, [{ metadata::shortname: ?shortname }])
        )
        .into_iter()
        .next()
        .map(|(n,)| n)
        .unwrap_or_default();

        if !prefix.is_empty() && !name.starts_with(prefix) {
            continue;
        }

        branches.push(BranchInfo {
            id: branch_id,
            name,
        });
    }

    branches.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(branches)
}

struct BranchCardState {
    branch_prefix: String,
    branches: Vec<BranchInfo>,
    selected: Option<usize>,
    live: bool,
    prev_head: Option<CommitHandle>,
    last_head: Option<CommitHandle>,
    last_head_ts_ms: Option<u64>,
    last_delta_tribles: usize,
    last_error: Option<String>,
    last_repo_path: Option<PathBuf>,
    last_repo_open: bool,
}

impl Default for BranchCardState {
    fn default() -> Self {
        Self {
            branch_prefix: String::new(),
            branches: Vec::new(),
            selected: None,
            live: true,
            prev_head: None,
            last_head: None,
            last_head_ts_ms: None,
            last_delta_tribles: 0,
            last_error: None,
            last_repo_path: None,
            last_repo_open: false,
        }
    }
}

impl BranchCardState {
    fn reset_session_state(&mut self) {
        self.branches.clear();
        self.selected = None;
        self.prev_head = None;
        self.last_head = None;
        self.last_head_ts_ms = None;
        self.last_delta_tribles = 0;
        self.last_error = None;
    }
}

fn refresh_selected_branch(
    repo: &mut Repository<Pile<Blake3>>,
    branches: &[BranchInfo],
    selected: Option<usize>,
    prev_head: &mut Option<CommitHandle>,
    last_head: &mut Option<CommitHandle>,
    last_head_ts_ms: &mut Option<u64>,
    last_delta_tribles: &mut usize,
) -> Result<(), String> {
    let Some(branch_id) = selected.and_then(|idx| branches.get(idx)).map(|b| b.id) else {
        return Ok(());
    };

    let mut ws = repo
        .pull(branch_id)
        .map_err(|err| format!("pull branch {branch_id:x}: {err:?}"))?;

    let head = ws.head();
    let delta = match (*prev_head, head) {
        (Some(prev), Some(new)) if prev == new => TribleSet::new(),
        (Some(prev), Some(_)) => ws
            .checkout(prev..)
            .map_err(|err| format!("checkout delta: {err}"))?,
        (None, Some(_)) => ws
            .checkout(..)
            .map_err(|err| format!("checkout branch: {err}"))?,
        (_, None) => TribleSet::new(),
    };

    *last_delta_tribles = delta.iter().count();
    *prev_head = head;
    *last_head = head;

    let head_ts_ms = head.and_then(|head| {
        let reader = repo.storage_mut().reader().ok()?;
        reader.metadata(head).ok().flatten().map(|m| m.timestamp)
    });
    *last_head_ts_ms = head_ts_ms;

    Ok(())
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;
    let pile_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./repo.pile".to_owned());

    nb.view(|ui| {
        widgets::markdown(
            ui,
            "# TribleSpace + GORBIE best practices\n\nThis notebook demonstrates a simple pattern for *live* TribleSpace views inside a GORBIE notebook.\n\nKey ideas:\n- Keep the `.pile` open in notebook state.\n- Call `repo.pull(...)` repeatedly (even every frame). The underlying `Pile` refreshes branch heads internally.\n- Use `ws.checkout(prev_head..)` to load only deltas and incrementally update your own derived indices.\n- If you need heavier work, do it in a background thread (see `ComputedState`).\n\nThis keeps your notebook responsive even when the underlying history grows.\n",
        );
    });

    let repo_state = nb.state("repo", PileRepoState::new(pile_path), move |ui, repo| {
        with_padding(ui, padding, |ui| {
            ui.heading("Pile");
            PileRepoWidget::new(repo).show(ui);
        });
    });

    nb.state("state", BranchCardState::default(), move |ui, state| {
        let mut repo_state_guard = repo_state.read_mut(ui);
        with_padding(ui, padding, |ui| {
            {
                let open_path = repo_state_guard.open_path().map(|p| p.to_path_buf());
                let is_open = repo_state_guard.is_open();
                if open_path != state.last_repo_path || is_open != state.last_repo_open {
                    state.last_repo_path = open_path;
                    state.last_repo_open = is_open;
                    state.reset_session_state();
                }
            }

            ui.horizontal(|ui| {
                ui.label("Branch prefix:");
                ui.add_sized(
                    [ui.available_width(), 0.0],
                    widgets::TextField::singleline(&mut state.branch_prefix),
                );
            });

            ui.add_space(10.0);

            if !state.last_repo_open {
                ui.label(egui::RichText::new("Open a pile to start.").italics().small());
                return;
            }

            if state.branches.is_empty() {
                if let Some(repo) = repo_state_guard.repo_mut() {
                    if let Ok(branches) = scan_branches(repo, state.branch_prefix.trim()) {
                        state.branches = branches;
                        if state.selected.is_none() && !state.branches.is_empty() {
                            state.selected = Some(0);
                        }
                    }
                }
            }

            ui.horizontal_wrapped(|ui| {
                ui.label("Mode:");
                ui.add(widgets::ChoiceToggle::binary(&mut state.live, "PAUSED", "LIVE"));

                if ui.add(widgets::Button::new("Scan branches")).clicked() {
                    if let Some(repo) = repo_state_guard.repo_mut() {
                        match scan_branches(repo, state.branch_prefix.trim()) {
                            Ok(branches) => {
                                state.branches = branches;
                                if state.selected.is_none() && !state.branches.is_empty() {
                                    state.selected = Some(0);
                                }
                            }
                            Err(err) => state.last_error = Some(err),
                        }
                    }
                }

                ui.add_space(12.0);
                ui.label("Session:");
                egui::ComboBox::from_id_salt("branch_selector")
                    .selected_text(
                        state
                            .selected
                            .and_then(|idx| state.branches.get(idx))
                            .map(|b| b.name.as_str())
                            .unwrap_or("<none>"),
                    )
                    .show_ui(ui, |ui| {
                        for (idx, branch) in state.branches.iter().enumerate() {
                            let selected = state.selected == Some(idx);
                            if ui.selectable_label(selected, &branch.name).clicked() {
                                state.selected = Some(idx);
                                state.prev_head = None;
                                state.last_head = None;
                                state.last_head_ts_ms = None;
                                state.last_delta_tribles = 0;
                            }
                        }
                    });
            });

            if let Some(err) = state.last_error.as_deref() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(err)
                        .color(ui.visuals().error_fg_color)
                        .monospace(),
                );
            }

            ui.add_space(10.0);

            if state.live {
                if let Some(repo) = repo_state_guard.repo_mut() {
                    state.last_error = refresh_selected_branch(
                        repo,
                        &state.branches,
                        state.selected,
                        &mut state.prev_head,
                        &mut state.last_head,
                        &mut state.last_head_ts_ms,
                        &mut state.last_delta_tribles,
                    )
                    .err();
                }
                ui.ctx().request_repaint();
            }

            ui.heading("Live snapshot");
            ui.add_space(4.0);

            let head_label = state
                .last_head
                .map(fmt_commit_prefix)
                .unwrap_or_else(|| "<none>".to_owned());
            ui.label(format!("Head: {head_label}"));
            ui.label(format!("Delta tribles: {}", state.last_delta_tribles));
            if let Some(ts) = state.last_head_ts_ms {
                ui.label(format!("Last commit age: {} ms", now_ms().saturating_sub(ts)));
            }
        });
    });
}
