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
use triblespace::core::value::{RawValue, Value};
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

fn fmt_id(id: Id) -> String {
    let full = format!("{id:x}");
    full[..8.min(full.len())].to_string()
}

// ── wiki data (TribleSet + resolved blobs) ────────────────────────────

#[derive(Clone, Default)]
struct WikiData {
    space: TribleSet,
    /// All text blobs resolved to strings, keyed by handle raw bytes.
    blobs: BTreeMap<RawValue, String>,
    error: Option<String>,
}

impl WikiData {
    fn blob(&self, handle: TextHandle) -> &str {
        self.blobs.get(&handle.raw).map(|s| s.as_str()).unwrap_or("")
    }

    /// Find the latest version ID for a fragment (by timestamp).
    fn latest_version(&self, fragment_id: Id) -> Option<Id> {
        find!(
            (vid: Id, ts: Value<triblespace::prelude::valueschemas::NsTAIInterval>),
            pattern!(&self.space, [{
                ?vid @
                metadata::tag: &KIND_VERSION_ID,
                wiki::fragment: &fragment_id,
                wiki::created_at: ?ts,
            }])
        )
        .max_by_key(|(_, ts)| ts.raw)
        .map(|(vid, _)| vid)
    }

    fn title(&self, vid: Id) -> &str {
        find!(
            (h: TextHandle),
            pattern!(&self.space, [{ vid @ wiki::title: ?h }])
        )
        .next()
        .map(|(h,)| self.blob(h))
        .unwrap_or("")
    }

    fn content(&self, vid: Id) -> &str {
        find!(
            (h: TextHandle),
            pattern!(&self.space, [{ vid @ wiki::content: ?h }])
        )
        .next()
        .map(|(h,)| self.blob(h))
        .unwrap_or("")
    }

    fn tags(&self, vid: Id) -> Vec<Id> {
        find!(
            (tag: Id),
            pattern!(&self.space, [{ vid @ metadata::tag: ?tag }])
        )
        .filter(|(t,)| *t != KIND_VERSION_ID)
        .map(|(t,)| t)
        .collect()
    }

    fn tag_name(&self, tag_id: Id) -> &str {
        find!(
            (h: TextHandle),
            pattern!(&self.space, [{ tag_id @ metadata::name: ?h }])
        )
        .next()
        .map(|(h,)| self.blob(h))
        .unwrap_or("")
    }

    fn is_archived(&self, vid: Id) -> bool {
        self.tags(vid).contains(&TAG_ARCHIVED_ID)
    }

    fn is_markdown(&self, vid: Id) -> bool {
        self.tags(vid)
            .iter()
            .any(|t| self.tag_name(*t) == "markdown")
    }

    /// All unique fragment IDs with their latest version, sorted by title.
    fn fragments_sorted(&self) -> Vec<(Id, Id)> {
        let mut latest: BTreeMap<Id, (Id, RawValue)> = BTreeMap::new();
        for (vid, frag, ts) in find!(
            (vid: Id, frag: Id, ts: Value<triblespace::prelude::valueschemas::NsTAIInterval>),
            pattern!(&self.space, [{
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
        let mut entries: Vec<(Id, Id)> = latest
            .into_iter()
            .map(|(frag, (vid, _))| (frag, vid))
            .collect();
        entries.sort_by(|a, b| {
            self.title(a.1)
                .to_lowercase()
                .cmp(&self.title(b.1).to_lowercase())
        });
        entries
    }
}

// ── background loading ───────────────────────────────────────────────

fn load_wiki_data(path: PathBuf) -> WikiData {
    let open = || -> Result<WikiData, String> {
        let mut pile =
            Pile::<Blake3>::open(&path).map_err(|e| format!("open pile: {e:?}"))?;
        if let Err(err) = pile.restore() {
            let _ = pile.close();
            return Err(format!("restore: {err:?}"));
        }
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand_core06::OsRng);
        let mut repo = Repository::new(pile, signing_key, TribleSet::new())
            .map_err(|e| format!("repo: {e:?}"))?;

        // Find wiki branch.
        repo.storage_mut()
            .refresh()
            .map_err(|e| format!("refresh: {e:?}"))?;
        let reader = repo
            .storage_mut()
            .reader()
            .map_err(|e| format!("reader: {e:?}"))?;
        let mut wiki_branch = None;
        for item in repo
            .storage_mut()
            .branches()
            .map_err(|e| format!("branches: {e:?}"))?
        {
            let bid = item.map_err(|e| format!("branch: {e:?}"))?;
            let Some(head) = repo
                .storage_mut()
                .head(bid)
                .map_err(|e| format!("head: {e:?}"))?
            else {
                continue;
            };
            let meta: TribleSet = reader.get(head).map_err(|e| format!("meta: {e:?}"))?;
            let name = find!(
                (h: TextHandle),
                pattern!(&meta, [{ metadata::name: ?h }])
            )
            .into_iter()
            .next()
            .and_then(|(h,)| reader.get::<View<str>, LongString>(h).ok())
            .map(|v| v.to_string());
            if name.as_deref() == Some(WIKI_BRANCH_NAME) {
                wiki_branch = Some(bid);
                break;
            }
        }
        let branch_id =
            wiki_branch.ok_or_else(|| "no 'wiki' branch found".to_string())?;

        // Checkout and resolve all blobs.
        let mut ws = repo.pull(branch_id).map_err(|e| format!("pull: {e:?}"))?;
        let space = ws.checkout(..).map_err(|e| format!("checkout: {e:?}"))?;

        let mut blobs = BTreeMap::new();
        // Resolve all title handles.
        for (h,) in find!(
            (h: TextHandle),
            pattern!(&space, [{ _?vid @ wiki::title: ?h }])
        ) {
            if !blobs.contains_key(&h.raw) {
                if let Ok(view) = ws.get::<View<str>, LongString>(h) {
                    blobs.insert(h.raw, view.as_ref().to_string());
                }
            }
        }
        // Resolve all content handles.
        for (h,) in find!(
            (h: TextHandle),
            pattern!(&space, [{ _?vid @ wiki::content: ?h }])
        ) {
            if !blobs.contains_key(&h.raw) {
                if let Ok(view) = ws.get::<View<str>, LongString>(h) {
                    blobs.insert(h.raw, view.as_ref().to_string());
                }
            }
        }
        // Resolve all tag/metadata name handles.
        for (h,) in find!(
            (h: TextHandle),
            pattern!(&space, [{ _?id @ metadata::name: ?h }])
        ) {
            if !blobs.contains_key(&h.raw) {
                if let Ok(view) = ws.get::<View<str>, LongString>(h) {
                    blobs.insert(h.raw, view.as_ref().to_string());
                }
            }
        }

        let _ = repo.close();
        Ok(WikiData {
            space,
            blobs,
            error: None,
        })
    };

    open().unwrap_or_else(|e| WikiData {
        error: Some(e),
        ..Default::default()
    })
}

// ── notebook state ────────────────────────────────────────────────────

struct BrowserState {
    pile_path: String,
    data: ComputedState<WikiData>,
    open_pages: Vec<Id>,
    search: String,
    show_archived: bool,
}

impl BrowserState {
    fn new(pile_path: String) -> Self {
        Self {
            pile_path,
            data: ComputedState::default(),
            open_pages: Vec::new(),
            search: String::new(),
            show_archived: false,
        }
    }
}

// ── wiki-aware markdown rendering ─────────────────────────────────────

/// Render wiki content (typst by default, markdown if tagged) and intercept
/// `wiki:<hex>` link clicks.
fn render_wiki_content(ctx: &mut CardCtx<'_>, content: &str, markdown: bool) -> Option<Id> {
    let cmd_count_before = ctx.ctx().output(|o| o.commands.len());

    if markdown {
        ctx.markdown(content);
    } else {
        ctx.typst(content);
    }

    let mut wiki_target = None;
    ctx.ctx().output_mut(|o| {
        let new_commands: Vec<egui::OutputCommand> =
            o.commands.drain(cmd_count_before..).collect();
        for cmd in new_commands {
            match &cmd {
                egui::OutputCommand::OpenUrl(open_url) => {
                    if let Some(hex) = open_url.url.strip_prefix("wiki:") {
                        if let Some(id) = Id::from_hex(hex) {
                            wiki_target = Some(id);
                        }
                    } else {
                        o.commands.push(cmd);
                    }
                }
                _ => o.commands.push(cmd),
            }
        }
    });
    wiki_target
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
        if state.data.is_running() {
            ctx.ctx().request_repaint();
        }
        state.data.poll();

        ctx.with_padding(padding, |ctx| {
            ctx.heading("Pile");

            ctx.horizontal(|ctx| {
                ctx.label("Path:");
                let field_w = ctx.available_width() - 80.0;
                ctx.add_sized(
                    [field_w, 0.0],
                    widgets::TextField::singleline(&mut state.pile_path),
                );
                if !state.data.is_running() {
                    if ctx.small_button("Open").clicked() {
                        let path = PathBuf::from(state.pile_path.trim().to_owned());
                        state.data.spawn(move || load_wiki_data(path));
                        ctx.ctx().request_repaint();
                    }
                } else {
                    ctx.label(egui::RichText::new("Loading...").weak().italics());
                }
            });

            let data = state.data.value();
            if let Some(err) = &data.error {
                let color = ctx.visuals().error_fg_color;
                ctx.add_space(4.0);
                ctx.label(
                    egui::RichText::new(err.as_str()).color(color).monospace(),
                );
            }

            if data.space.is_empty() && data.error.is_none() && !state.data.is_running() {
                return;
            }
            if data.space.is_empty() {
                return;
            }

            ctx.add_space(8.0);
            ctx.heading("Fragments");

            ctx.horizontal(|ctx| {
                ctx.label("Search:");
                let w = ctx.available_width();
                ctx.add_sized(
                    [w, 0.0],
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

            // Query the TribleSet directly for the fragment list.
            let needle = state.search.to_lowercase();
            let sel_color = ctx.visuals().selection.stroke.color;
            let mut to_open = None;

            for (frag_id, vid) in data.fragments_sorted() {
                let archived = data.is_archived(vid);
                if archived && !state.show_archived {
                    continue;
                }

                let title = data.title(vid);
                let tags = data.tags(vid);
                let tag_names: Vec<&str> = tags
                    .iter()
                    .filter(|t| **t != TAG_ARCHIVED_ID)
                    .map(|t| data.tag_name(*t))
                    .collect();

                if !needle.is_empty()
                    && !title.to_lowercase().contains(&needle)
                    && !tag_names
                        .iter()
                        .any(|t| t.to_lowercase().contains(&needle))
                {
                    continue;
                }

                let already_open = state.open_pages.contains(&frag_id);
                ctx.horizontal(|ctx| {
                    let display_title = if title.is_empty() {
                        fmt_id(frag_id)
                    } else {
                        title.to_string()
                    };
                    let label = if archived {
                        format!("{display_title} [archived]")
                    } else if tag_names.is_empty() {
                        display_title
                    } else {
                        format!("{display_title}  [{}]", tag_names.join(", "))
                    };

                    if already_open {
                        ctx.label(
                            egui::RichText::new(label).strong().color(sel_color),
                        );
                    } else if ctx.link(label).clicked() {
                        to_open = Some(frag_id);
                    }
                });
            }

            if let Some(id) = to_open {
                if !state.open_pages.contains(&id) {
                    state.open_pages.push(id);
                }
            }
        });

        // ── floating wiki page cards ─────────────────────────────────
        let open_snapshot: Vec<Id> = state.open_pages.clone();
        let mut to_close = Vec::new();
        let mut to_open_from_link = Vec::new();

        for &frag_id in &open_snapshot {
            let frag_bytes: &[u8] = frag_id.as_ref();
            let mut frag_key = [0u8; 16];
            frag_key.copy_from_slice(frag_bytes);

            let data = state.data.value();
            let vid = data.latest_version(frag_id);
            let title = vid.map(|v| data.title(v)).unwrap_or("");
            let content = vid.map(|v| data.content(v)).unwrap_or("");
            let is_md = vid.map(|v| data.is_markdown(v)).unwrap_or(true);

            ctx.push_id(frag_key, |ctx| {
                let resp = ctx.float(|ctx| {
                    ctx.with_padding(padding, |ctx| {
                        ctx.add(
                            egui::Label::new(egui::RichText::new(title).heading())
                                .wrap(),
                        );
                        ctx.label(
                            egui::RichText::new(fmt_id(frag_id)).monospace().weak(),
                        );
                        ctx.separator();

                        if let Some(target_id) = render_wiki_content(ctx, content, is_md) {
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
            state.open_pages.retain(|&p| p != id);
        }
        for id in to_open_from_link {
            if !state.open_pages.contains(&id) {
                state.open_pages.push(id);
            }
        }
    });
}
