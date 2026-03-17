#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = "..", features = ["triblespace", "markdown"] }
//! egui = "0.33"
//! eframe = "0.33"
//! triblespace = { path = "../../triblespace-rs" }
//! ed25519-dalek = "2"
//! parking_lot = "0.12"
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{self};
use triblespace::core::id::Id;
use triblespace::core::metadata;
use triblespace::core::repo::pile::Pile;
use triblespace::core::repo::{BlobStore, BlobStoreGet, BranchStore, Repository};
use triblespace::core::trible::TribleSet;
use triblespace::core::value::schemas::hash::{Blake3, Handle};
use triblespace::core::value::Value;
use triblespace::macros::{find, pattern};
use triblespace::prelude::blobschemas::LongString;
use triblespace::prelude::View;

use GORBIE::notebook;
use GORBIE::prelude::*;
use GORBIE::widgets;

// ── wiki schema (mirrors playground/faculties/wiki.rs) ────────────────
const WIKI_BRANCH_NAME: &str = "wiki";
const KIND_VERSION_ID: Id = triblespace::macros::id_hex!("1AA0310347EDFED7874E8BFECC6438CF");
const TAG_ARCHIVED_ID: Id = triblespace::macros::id_hex!("480CB6A663C709478A26A8B49F366C3F");

mod wiki {
    use triblespace::prelude::*;
    attributes! {
        "EBFC56D50B748E38A14F5FC768F1B9C1" as fragment: valueschemas::GenId;
        "6DBBE746B7DD7A4793CA098AB882F553" as content: valueschemas::Handle<valueschemas::Blake3, blobschemas::LongString>;
        "476F6E26FCA65A0B49E38CC44CF31467" as created_at: valueschemas::NsTAIInterval;
        "78BABEF1792531A2E51A372D96FE5F3E" as title: valueschemas::Handle<valueschemas::Blake3, blobschemas::LongString>;
        "DEAFB7E307DF72389AD95A850F24BAA5" as links_to: valueschemas::GenId;
    }
}

type TextHandle = Value<Handle<Blake3, LongString>>;

// ── fragment index ────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct FragmentEntry {
    fragment_id: Id,
    title: String,
    tags: Vec<String>,
    archived: bool,
}

fn fmt_id(id: Id) -> String {
    let full = format!("{id:x}");
    full[..8.min(full.len())].to_string()
}

// ── pile operations (run on background threads) ───────────────────────

fn open_repo(path: &std::path::Path) -> Result<Repository<Pile<Blake3>>, String> {
    let mut pile = Pile::<Blake3>::open(path).map_err(|e| format!("open pile: {e:?}"))?;
    if let Err(err) = pile.restore() {
        let _ = pile.close();
        return Err(format!("restore pile: {err:?}"));
    }
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rand_core06::OsRng);
    Repository::new(pile, signing_key, TribleSet::new())
        .map_err(|e| format!("create repository: {e:?}"))
}

fn find_wiki_branch(repo: &mut Repository<Pile<Blake3>>) -> Result<Id, String> {
    repo.storage_mut()
        .refresh()
        .map_err(|e| format!("refresh: {e:?}"))?;
    let reader = repo
        .storage_mut()
        .reader()
        .map_err(|e| format!("reader: {e:?}"))?;

    for item in repo
        .storage_mut()
        .branches()
        .map_err(|e| format!("branches: {e:?}"))?
    {
        let branch_id = item.map_err(|e| format!("branch id: {e:?}"))?;
        let Some(head) = repo
            .storage_mut()
            .head(branch_id)
            .map_err(|e| format!("head: {e:?}"))?
        else {
            continue;
        };
        let meta: TribleSet = reader.get(head).map_err(|e| format!("meta blob: {e:?}"))?;
        let name = find!(
            (h: TextHandle),
            pattern!(&meta, [{ metadata::name: ?h }])
        )
        .into_iter()
        .next()
        .and_then(|(h,)| reader.get::<View<str>, LongString>(h).ok())
        .map(|v| v.to_string());
        if name.as_deref() == Some(WIKI_BRANCH_NAME) {
            return Ok(branch_id);
        }
    }
    Err("no 'wiki' branch found".to_string())
}

fn scan_fragments(
    repo: &mut Repository<Pile<Blake3>>,
    branch_id: Id,
) -> Result<Vec<FragmentEntry>, String> {
    repo.storage_mut()
        .refresh()
        .map_err(|e| format!("refresh: {e:?}"))?;
    let mut ws = repo.pull(branch_id).map_err(|e| format!("pull: {e:?}"))?;
    let space = ws.checkout(..).map_err(|e| format!("checkout: {e:?}"))?;

    let mut tag_names: BTreeMap<Id, String> = BTreeMap::new();
    for (tag_id, handle) in find!(
        (tag_id: Id, handle: TextHandle),
        pattern!(&space, [{ ?tag_id @ metadata::name: ?handle }])
    ) {
        if let Ok(view) = ws.get::<View<str>, LongString>(handle) {
            tag_names.insert(tag_id, view.as_ref().to_string());
        }
    }

    let mut latest: BTreeMap<Id, (Id, [u8; 32])> = BTreeMap::new();
    for (vid, frag, ts) in find!(
        (vid: Id, frag: Id, ts: Value<triblespace::prelude::valueschemas::NsTAIInterval>),
        pattern!(&space, [{
            ?vid @
            metadata::tag: &KIND_VERSION_ID,
            wiki::fragment: ?frag,
            wiki::created_at: ?ts,
        }])
    ) {
        let replace = match latest.get(&frag) {
            None => true,
            Some((_, prev_ts)) => ts.raw > *prev_ts,
        };
        if replace {
            latest.insert(frag, (vid, ts.raw));
        }
    }

    let mut entries = Vec::new();
    for (frag, (vid, _)) in &latest {
        let title = find!(
            (h: TextHandle),
            pattern!(&space, [{ *vid @ wiki::title: ?h }])
        )
        .next()
        .and_then(|(h,)| ws.get::<View<str>, LongString>(h).ok())
        .map(|v| v.as_ref().to_string())
        .unwrap_or_default();

        let vtags: Vec<Id> = find!(
            (tag: Id),
            pattern!(&space, [{ *vid @ metadata::tag: ?tag }])
        )
        .filter(|(t,)| *t != KIND_VERSION_ID)
        .map(|(t,)| t)
        .collect();

        entries.push(FragmentEntry {
            fragment_id: *frag,
            title,
            tags: vtags
                .iter()
                .filter(|t| **t != TAG_ARCHIVED_ID)
                .map(|t| tag_names.get(t).cloned().unwrap_or_else(|| fmt_id(*t)))
                .collect(),
            archived: vtags.contains(&TAG_ARCHIVED_ID),
        });
    }

    entries.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    Ok(entries)
}

fn load_fragment_content(
    path: &std::path::Path,
    branch_id: Id,
    fragment_id: Id,
) -> Result<String, String> {
    let mut repo = open_repo(path)?;
    let mut ws = repo.pull(branch_id).map_err(|e| format!("pull: {e:?}"))?;
    let space = ws.checkout(..).map_err(|e| format!("checkout: {e:?}"))?;

    let latest_vid = find!(
        (vid: Id, ts: Value<triblespace::prelude::valueschemas::NsTAIInterval>),
        pattern!(&space, [{
            ?vid @
            metadata::tag: &KIND_VERSION_ID,
            wiki::fragment: &fragment_id,
            wiki::created_at: ?ts,
        }])
    )
    .max_by_key(|(_, ts)| ts.raw)
    .map(|(vid, _)| vid)
    .ok_or_else(|| "no version found".to_string())?;

    let content_handle: TextHandle = find!(
        (h: TextHandle),
        pattern!(&space, [{ latest_vid @ wiki::content: ?h }])
    )
    .next()
    .map(|(h,)| h)
    .ok_or_else(|| "no content handle".to_string())?;

    let view: View<str> = ws
        .get(content_handle)
        .map_err(|e| format!("read content: {e:?}"))?;
    let content = view.as_ref().to_string();
    let _ = repo.close();
    Ok(content)
}

/// Background task: open pile, find wiki branch, scan all fragments.
fn load_wiki_index(path: PathBuf) -> WikiIndex {
    let mut repo = match open_repo(&path) {
        Ok(r) => r,
        Err(e) => return WikiIndex { error: Some(e), ..Default::default() },
    };
    let branch_id = match find_wiki_branch(&mut repo) {
        Ok(id) => id,
        Err(e) => {
            let _ = repo.close();
            return WikiIndex { error: Some(e), ..Default::default() };
        }
    };
    let fragments = match scan_fragments(&mut repo, branch_id) {
        Ok(f) => f,
        Err(e) => {
            let _ = repo.close();
            return WikiIndex {
                branch_id: Some(branch_id),
                error: Some(e),
                ..Default::default()
            };
        }
    };
    let _ = repo.close();
    WikiIndex {
        branch_id: Some(branch_id),
        fragments,
        error: None,
    }
}

// ── shared state types ────────────────────────────────────────────────

#[derive(Clone, Default)]
struct WikiIndex {
    branch_id: Option<Id>,
    fragments: Vec<FragmentEntry>,
    error: Option<String>,
}

#[derive(Clone)]
struct OpenPage {
    fragment_id: Id,
    title: String,
    content: String,
}

// ── notebook state ────────────────────────────────────────────────────

struct BrowserState {
    pile_path: String,
    index: ComputedState<WikiIndex>,
    open_pages: Vec<OpenPage>,
    search: String,
    show_archived: bool,
    content_loaders: Vec<ComputedState<Option<OpenPage>>>,
}

impl BrowserState {
    fn new(pile_path: String) -> Self {
        Self {
            pile_path,
            index: ComputedState::default(),
            open_pages: Vec::new(),
            search: String::new(),
            show_archived: false,
            content_loaders: Vec::new(),
        }
    }
}

// ── wiki-aware markdown rendering ─────────────────────────────────────

/// Render markdown and intercept `wiki:<hex>` link clicks.
/// Returns the resolved fragment Id if a wiki link was clicked.
fn render_wiki_markdown(ctx: &mut CardCtx<'_>, content: &str) -> Option<Id> {
    // Snapshot command count before rendering.
    let cmd_count_before = ctx.ctx().output(|o| o.commands.len());

    ctx.markdown(content);

    // Check for new commands — intercept wiki: URLs.
    let mut wiki_target = None;
    ctx.ctx().output_mut(|o| {
        let new_commands: Vec<egui::OutputCommand> = o.commands.drain(cmd_count_before..).collect();
        for cmd in new_commands {
            match &cmd {
                egui::OutputCommand::OpenUrl(open_url) => {
                    if let Some(hex) = open_url.url.strip_prefix("wiki:") {
                        if let Some(id) = Id::from_hex(hex) {
                            wiki_target = Some(id);
                        }
                    } else {
                        // Re-emit non-wiki URLs.
                        o.commands.push(cmd);
                    }
                }
                _ => o.commands.push(cmd),
            }
        }
    });

    wiki_target
}

/// Open a wiki page by fragment ID, spawning a background content loader.
/// Works directly on BrowserState (for use inside the browser card closure).
fn open_wiki_page_direct(state: &mut BrowserState, target_id: Id) {
    if state.open_pages.iter().any(|p| p.fragment_id == target_id) {
        return;
    }

    let title = state
        .index
        .value()
        .fragments
        .iter()
        .find(|f| f.fragment_id == target_id)
        .map(|f| f.title.clone())
        .unwrap_or_else(|| fmt_id(target_id));

    if let Some(branch_id) = state.index.value().branch_id {
        let path = PathBuf::from(state.pile_path.trim().to_owned());
        let frag_id = target_id;
        let frag_title = title;
        let mut loader = ComputedState::new(None);
        loader.spawn(move || {
            let content = load_fragment_content(&path, branch_id, frag_id)
                .unwrap_or_else(|e| format!("*Error: {e}*"));
            Some(OpenPage {
                fragment_id: frag_id,
                title: frag_title,
                content,
            })
        });
        state.content_loaders.push(loader);
    }
}

// ── entry point ───────────────────────────────────────────────────────

#[notebook]
fn main(nb: &mut NotebookCtx) {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;
    let pile_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./self.pile".to_owned());

    nb.view(|ctx| {
        widgets::markdown(
            ctx,
            "# Wiki Viewer\nBrowse wiki fragments stored in a TribleSpace pile.",
        );
    });

    nb.state("browser", BrowserState::new(pile_path), move |ctx, state| {
        // Keep repainting while background work is in flight.
        if state.index.is_running() || !state.content_loaders.is_empty() {
            ctx.ctx().request_repaint();
        }

        // Poll background tasks.
        state.index.poll();

        // Drain finished content loaders into open_pages.
        for loader in &mut state.content_loaders {
            if loader.poll() {
                if let Some(page) = loader.value().clone() {
                    state.open_pages.push(page);
                }
            }
        }
        state.content_loaders.retain(|l| l.is_running());

        ctx.with_padding(padding, |ctx| {
            ctx.heading("Pile");

            ctx.horizontal(|ctx| {
                ctx.label("Path:");
                let field_w = ctx.available_width() - 80.0;
                ctx.add_sized(
                    [field_w, 0.0],
                    widgets::TextField::singleline(&mut state.pile_path),
                );
                let is_loading = state.index.is_running();
                if !is_loading {
                    if ctx.small_button("Open").clicked() {
                        let path = PathBuf::from(state.pile_path.trim().to_owned());
                        state.index.spawn(move || load_wiki_index(path));
                        ctx.ctx().request_repaint();
                    }
                } else {
                    ctx.label(egui::RichText::new("Loading...").weak().italics());
                    ctx.ctx().request_repaint();
                }
            });

            let index = state.index.value();
            if let Some(err) = &index.error {
                let error_color = ctx.visuals().error_fg_color;
                ctx.add_space(4.0);
                ctx.label(
                    egui::RichText::new(err.as_str())
                        .color(error_color)
                        .monospace(),
                );
            }

            if index.fragments.is_empty() && index.error.is_none() && !state.index.is_running() {
                return;
            }

            ctx.add_space(8.0);
            ctx.heading("Fragments");

            ctx.horizontal(|ctx| {
                ctx.label("Search:");
                let field_w = ctx.available_width();
                ctx.add_sized(
                    [field_w, 0.0],
                    widgets::TextField::singleline(&mut state.search),
                );
            });

            ctx.horizontal(|ctx| {
                ctx.add(widgets::ToggleButton::new(
                    &mut state.show_archived,
                    "Show archived",
                ));
            });

            ctx.add_space(8.0);

            let needle = state.search.to_lowercase();
            let open_ids: Vec<Id> = state.open_pages.iter().map(|p| p.fragment_id).collect();
            let sel_color = ctx.visuals().selection.stroke.color;
            let mut to_open = None;

            for frag in &state.index.value().fragments {
                if frag.archived && !state.show_archived {
                    continue;
                }
                if !needle.is_empty()
                    && !frag.title.to_lowercase().contains(&needle)
                    && !frag.tags.iter().any(|t| t.to_lowercase().contains(&needle))
                {
                    continue;
                }

                let already_open = open_ids.contains(&frag.fragment_id);
                ctx.horizontal(|ctx| {
                    let title = if frag.title.is_empty() {
                        fmt_id(frag.fragment_id)
                    } else {
                        frag.title.clone()
                    };
                    let label = if frag.archived {
                        format!("{title} [archived]")
                    } else if frag.tags.is_empty() {
                        title
                    } else {
                        format!("{title}  [{}]", frag.tags.join(", "))
                    };

                    if already_open {
                        ctx.label(
                            egui::RichText::new(label).strong().color(sel_color),
                        );
                    } else if ctx.link(label).clicked() {
                        to_open = Some(frag.fragment_id);
                    }
                });
            }

            if let Some(id) = to_open {
                let title = state
                    .index
                    .value()
                    .fragments
                    .iter()
                    .find(|f| f.fragment_id == id)
                    .map(|f| f.title.clone())
                    .unwrap_or_else(|| fmt_id(id));

                if let Some(branch_id) = state.index.value().branch_id {
                    let path = PathBuf::from(state.pile_path.trim().to_owned());
                    let frag_id = id;
                    let frag_title = title;
                    let mut loader = ComputedState::new(None);
                    loader.spawn(move || {
                        let content = load_fragment_content(&path, branch_id, frag_id)
                            .unwrap_or_else(|e| format!("*Error: {e}*"));
                        Some(OpenPage {
                            fragment_id: frag_id,
                            title: frag_title,
                            content,
                        })
                    });
                    state.content_loaders.push(loader);
                    ctx.ctx().request_repaint();
                }
            }
        });

        // ── floating wiki page cards ─────────────────────────────────
        let page_snapshot: Vec<OpenPage> = state.open_pages.clone();
        let mut to_close = Vec::new();
        let mut to_open_from_link = Vec::new();
        for page in &page_snapshot {
            let frag_id = page.fragment_id;
            let frag_bytes: &[u8] = frag_id.as_ref();
            let mut frag_key = [0u8; 16];
            frag_key.copy_from_slice(frag_bytes);
            ctx.push_id(frag_key, |ctx| {
                let resp = ctx.float(|ctx| {
                    ctx.with_padding(padding, |ctx| {
                        ctx.add(egui::Label::new(
                            egui::RichText::new(&page.title).heading()
                        ).wrap());
                        ctx.label(
                            egui::RichText::new(fmt_id(frag_id)).monospace().weak(),
                        );
                        ctx.separator();

                        if let Some(target_id) = render_wiki_markdown(ctx, &page.content) {
                            to_open_from_link.push(target_id);
                        }
                    });
                });
                if resp.closed {
                    to_close.push(frag_id);
                }
            });
        }
        for id in to_close {
            state.open_pages.retain(|p| p.fragment_id != id);
        }
        for id in to_open_from_link {
            open_wiki_page_direct(state, id);
        }
    });
}
