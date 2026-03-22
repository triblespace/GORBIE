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

use cubecl::prelude::*;
use cubecl::wgpu::{WgpuDevice, WgpuRuntime};
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
            (vid: Id, ts: (i128, i128)),
            pattern!(&self.space, [{
                ?vid @
                metadata::tag: &KIND_VERSION_ID,
                wiki::fragment: &fragment_id,
                wiki::created_at: ?ts,
            }])
        )
        .max_by_key(|(_, ts)| ts.0)
        .map(|(vid, _)| vid)
    }

    fn title(&self, vid: Id) -> &str {
        find!(
            h: TextHandle,
            pattern!(&self.space, [{ vid @ wiki::title: ?h }])
        )
        .next()
        .map(|h| self.blob(h))
        .unwrap_or("")
    }

    fn content(&self, vid: Id) -> &str {
        find!(
            h: TextHandle,
            pattern!(&self.space, [{ vid @ wiki::content: ?h }])
        )
        .next()
        .map(|h| self.blob(h))
        .unwrap_or("")
    }

    fn tags(&self, vid: Id) -> Vec<Id> {
        find!(
            tag: Id,
            pattern!(&self.space, [{ vid @ metadata::tag: ?tag }])
        )
        .filter(|t| *t != KIND_VERSION_ID)
        .collect()
    }

    fn tag_name(&self, tag_id: Id) -> &str {
        find!(
            h: TextHandle,
            pattern!(&self.space, [{ tag_id @ metadata::name: ?h }])
        )
        .next()
        .map(|h| self.blob(h))
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

    /// Outgoing wiki links from a version entity.
    fn links(&self, vid: Id) -> Vec<Id> {
        find!(
            target: Id,
            pattern!(&self.space, [{ vid @ wiki::links_to: ?target }])
        )
        .collect()
    }

    /// All unique fragment IDs with their latest version, sorted by title.
    fn fragments_sorted(&self) -> Vec<(Id, Id)> {
        let mut latest: BTreeMap<Id, (Id, i128)> = BTreeMap::new();
        for (vid, frag, ts) in find!(
            (vid: Id, frag: Id, ts: (i128, i128)),
            pattern!(&self.space, [{
                ?vid @
                metadata::tag: &KIND_VERSION_ID,
                wiki::fragment: ?frag,
                wiki::created_at: ?ts,
            }])
        ) {
            let replace = match latest.get(&frag) {
                None => true,
                Some((_, prev_key)) => ts.0 > *prev_key,
            };
            if replace {
                latest.insert(frag, (vid, ts.0));
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
        for h in find!(
            h: TextHandle,
            pattern!(&space, [{ _?vid @ wiki::title: ?h }])
        ) {
            if !blobs.contains_key(&h.raw) {
                if let Ok(view) = ws.get::<View<str>, LongString>(h) {
                    blobs.insert(h.raw, view.as_ref().to_string());
                }
            }
        }
        // Resolve all content handles.
        for h in find!(
            h: TextHandle,
            pattern!(&space, [{ _?vid @ wiki::content: ?h }])
        ) {
            if !blobs.contains_key(&h.raw) {
                if let Ok(view) = ws.get::<View<str>, LongString>(h) {
                    blobs.insert(h.raw, view.as_ref().to_string());
                }
            }
        }
        // Resolve all tag/metadata name handles.
        for h in find!(
            h: TextHandle,
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

// ── GPU force-directed layout kernel ──────────────────────────────────

/// GPU kernel: compute forces for each node in parallel.
/// Positions are interleaved as [x0, y0, x1, y1, ...].
/// Velocities are the same layout.
/// Edges are [from0, to0, from1, to1, ...].
#[cube(launch)]
fn force_step_kernel(
    pos: &Array<f32>,
    vel: &mut Array<f32>,
    edges: &Array<u32>,
    node_count: u32,
    edge_count: u32,
    pos_out: &mut Array<f32>,
) {
    let i = ABSOLUTE_POS as u32;
    if i < node_count {
        let repulsion = 8000.0f32;
        let attraction = 0.005f32;
        let damping = 0.85f32;
        let max_force = 50.0f32;
        let gravity = 0.001f32;

        let ix = (i * 2) as usize;
        let iy = ix + 1;
        let px = pos[ix];
        let py = pos[iy];

        let mut fx = 0.0f32;
        let mut fy = 0.0f32;

        // Repulsion from all other nodes.
        for j in 0..node_count {
            if j != i {
                let jx = (j * 2) as usize;
                let dx = px - pos[jx];
                let dy = py - pos[jx + 1];
                let dist_sq = (dx * dx + dy * dy).max(1.0f32);
                let dist = dist_sq.sqrt().max(0.001f32);
                let f = repulsion / dist_sq;
                fx += (dx / dist) * f;
                fy += (dy / dist) * f;
            }
        }

        // Attraction along edges.
        for e in 0..edge_count {
            let ea = edges[(e * 2) as usize];
            let eb = edges[(e * 2 + 1) as usize];
            if ea == i {
                let bx = (eb * 2) as usize;
                fx += (pos[bx] - px) * attraction;
                fy += (pos[bx + 1] - py) * attraction;
            }
            if eb == i {
                let ax = (ea * 2) as usize;
                fx += (pos[ax] - px) * attraction;
                fy += (pos[ax + 1] - py) * attraction;
            }
        }

        // Center gravity.
        fx -= px * gravity;
        fy -= py * gravity;

        // Clamp force.
        let fmag = (fx * fx + fy * fy).sqrt();
        if fmag > max_force {
            let scale = max_force / fmag;
            fx *= scale;
            fy *= scale;
        }

        // Update velocity and position.
        let vx = (vel[ix] + fx) * damping;
        let vy = (vel[iy] + fy) * damping;
        vel[ix] = vx;
        vel[iy] = vy;
        pos_out[ix] = px + vx;
        pos_out[iy] = py + vy;
    }
}

// ── force-directed graph ──────────────────────────────────────────────

struct WikiGraph {
    nodes: Vec<GraphNode>,
    edges: Vec<(usize, usize)>,
    // GPU state.
    gpu: Option<GpuForceState>,
}

struct GpuForceState {
    client: ComputeClient<WgpuRuntime>,
    pos_handle: cubecl::server::Handle,
    vel_handle: cubecl::server::Handle,
    edges_handle: cubecl::server::Handle,
    pos_out_handle: cubecl::server::Handle,
    node_count: u32,
    edge_count: u32,
}

struct GraphNode {
    frag_id: Id,
    label: String,
    pos: egui::Vec2,
}

impl WikiGraph {
    fn from_wiki(data: &WikiData) -> Self {
        let fragments = data.fragments_sorted();
        let mut frag_to_idx = BTreeMap::new();
        let mut nodes = Vec::new();

        let n = fragments.len().max(1) as f32;
        for (i, &(frag_id, vid)) in fragments.iter().enumerate() {
            let angle = (i as f32 / n) * std::f32::consts::TAU;
            let radius = 200.0 + n * 5.0;
            let title = data.title(vid);
            frag_to_idx.insert(frag_id, i);
            nodes.push(GraphNode {
                frag_id,
                label: if title.is_empty() {
                    fmt_id(frag_id)
                } else {
                    title.to_string()
                },
                pos: egui::vec2(angle.cos() * radius, angle.sin() * radius),
            });
        }

        let mut seen = std::collections::HashSet::new();
        let mut edges = Vec::new();
        for &(frag_id, vid) in &fragments {
            let from = frag_to_idx[&frag_id];
            for target in data.links(vid) {
                if let Some(&to) = frag_to_idx.get(&target) {
                    if from != to && seen.insert((from, to)) {
                        edges.push((from, to));
                    }
                }
            }
        }

        // Initialize GPU.
        let gpu = Self::init_gpu(&nodes, &edges);

        WikiGraph { nodes, edges, gpu }
    }

    fn init_gpu(nodes: &[GraphNode], edges: &[(usize, usize)]) -> Option<GpuForceState> {
        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);
        let n = nodes.len();

        let mut pos_flat: Vec<f32> = Vec::with_capacity(n * 2);
        let mut vel_flat: Vec<f32> = vec![0.0; n * 2];
        for node in nodes {
            pos_flat.push(node.pos.x);
            pos_flat.push(node.pos.y);
        }

        let edges_flat: Vec<u32> = edges
            .iter()
            .flat_map(|&(a, b)| [a as u32, b as u32])
            .collect();

        let pos_handle = client.create_from_slice(f32::as_bytes(&pos_flat));
        let vel_handle = client.create_from_slice(f32::as_bytes(&vel_flat));
        let edges_handle = if edges_flat.is_empty() {
            client.create_from_slice(u32::as_bytes(&[0u32; 2]))
        } else {
            client.create_from_slice(u32::as_bytes(&edges_flat))
        };
        let pos_out_handle = client.empty(n * 2 * std::mem::size_of::<f32>());

        Some(GpuForceState {
            client,
            pos_handle,
            vel_handle,
            edges_handle,
            pos_out_handle,
            node_count: n as u32,
            edge_count: edges.len() as u32,
        })
    }

    /// Run one iteration of force-directed layout on the GPU.
    fn step(&mut self) {
        let Some(gpu) = &mut self.gpu else {
            return;
        };
        let n = gpu.node_count as usize;
        if n == 0 {
            return;
        }

        // Launch kernel — one thread per node.
        // SAFETY: handles were created with matching sizes above.
        unsafe {
            force_step_kernel::launch::<WgpuRuntime>(
                &gpu.client,
                CubeCount::new_1d(n as u32),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts::<f32>(&gpu.pos_handle, n * 2, 1),
                ArrayArg::from_raw_parts::<f32>(&gpu.vel_handle, n * 2, 1),
                ArrayArg::from_raw_parts::<u32>(&gpu.edges_handle, gpu.edge_count.max(1) as usize * 2, 1),
                ScalarArg::new(gpu.node_count),
                ScalarArg::new(gpu.edge_count),
                ArrayArg::from_raw_parts::<f32>(&gpu.pos_out_handle, n * 2, 1),
            );
        }

        // Swap buffers.
        std::mem::swap(&mut gpu.pos_handle, &mut gpu.pos_out_handle);

        // Read positions back to CPU for rendering.
        let bytes = gpu.client.read_one(gpu.pos_handle.clone());
        let positions: &[f32] = f32::from_bytes(&bytes);
        for (i, node) in self.nodes.iter_mut().enumerate() {
            node.pos = egui::vec2(positions[i * 2], positions[i * 2 + 1]);
        }
    }

    /// Render the graph and return the clicked node's fragment ID (if any).
    fn show(&self, ui: &mut egui::Ui) -> Option<Id> {
        let available = ui.available_size();
        let (response, painter) = ui.allocate_painter(
            egui::vec2(available.x, available.y.max(400.0)),
            egui::Sense::click_and_drag(),
        );
        let rect = response.rect;
        let center = rect.center();

        // Pan + zoom stored in egui memory.
        let view_id = ui.id().with("wiki_graph_view");
        let pan_id = view_id.with("pan");
        let zoom_id = view_id.with("zoom");

        let mut pan: egui::Vec2 = ui.ctx().memory_mut(|m| {
            *m.data.get_temp_mut_or_insert_with(pan_id, || egui::Vec2::ZERO)
        });
        let mut zoom: f32 = ui.ctx().memory_mut(|m| {
            *m.data.get_temp_mut_or_insert_with(zoom_id, || 1.0f32)
        });

        // Pinch-to-zoom (trackpad) or ctrl+scroll (mouse).
        if response.hovered() {
            let pinch = ui.input(|i| i.zoom_delta());
            if pinch != 1.0 {
                let old_zoom = zoom;
                zoom = (zoom * pinch).clamp(0.05, 10.0);
                if let Some(hp) = response.hover_pos() {
                    let cursor_offset = hp - center - pan;
                    pan -= cursor_offset * (zoom / old_zoom - 1.0);
                }
                ui.ctx().memory_mut(|m| {
                    m.data.insert_temp(zoom_id, zoom);
                    m.data.insert_temp(pan_id, pan);
                });
            }
        }

        // Drag to pan (any mouse button).
        if response.dragged() {
            pan += response.drag_delta();
            ui.ctx().memory_mut(|m| m.data.insert_temp(pan_id, pan));
        }

        // Transform: world pos → screen pos.
        let to_screen =
            |world: egui::Vec2| center + pan + egui::vec2(world.x * zoom, world.y * zoom);

        let node_radius = 6.0 * zoom.max(0.3);
        let edge_color = ui.visuals().weak_text_color();
        let node_fill = GORBIE::themes::ral(5005);
        let node_stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        let label_color = ui.visuals().text_color();
        let font_id = egui::TextStyle::Small.resolve(ui.style());

        // Draw edges.
        for &(a, b) in &self.edges {
            let p1 = to_screen(self.nodes[a].pos);
            let p2 = to_screen(self.nodes[b].pos);
            if rect.expand(50.0).contains(p1) || rect.expand(50.0).contains(p2) {
                painter.line_segment([p1, p2], egui::Stroke::new(0.5, edge_color));
            }
        }

        // Draw nodes and detect clicks.
        let mut clicked = None;
        let hover_pos = response.hover_pos();
        let show_labels = zoom > 0.3;
        for node in &self.nodes {
            let pos = to_screen(node.pos);

            if !rect.expand(20.0).contains(pos) {
                continue;
            }

            painter.circle(pos, node_radius, node_fill, node_stroke);
            if show_labels {
                painter.text(
                    pos + egui::vec2(node_radius + 4.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &node.label,
                    font_id.clone(),
                    label_color,
                );
            }

            // Hit test.
            if let Some(hp) = hover_pos {
                if (hp - pos).length() < node_radius + 8.0 {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    if response.clicked() {
                        clicked = Some(node.frag_id);
                    }
                }
            }
        }

        clicked
    }
}

struct BrowserState {
    pile_path: String,
    data: ComputedState<WikiData>,
    graph: Option<WikiGraph>,
    open_pages: Vec<Id>,
}

impl BrowserState {
    fn new(pile_path: String) -> Self {
        Self {
            pile_path,
            data: ComputedState::default(),
            graph: None,
            open_pages: Vec::new(),
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
        .or_else(|| std::env::var("PILE").ok())
        .unwrap_or_else(|| "./self.pile".to_owned());

    nb.view(|ctx| {
        ctx.grid(|g| {
            g.full(|ctx| {
                ctx.markdown("# Wiki Viewer\nBrowse wiki fragments stored in a TribleSpace pile.");
            });
        });
    });

    nb.state("browser", BrowserState::new(pile_path), move |ctx, state| {
        // Auto-load on first frame.
        let pile_path_clone = state.pile_path.trim().to_owned();
        widgets::load_auto(
            ctx,
            &mut state.data,
            |data| data.space.is_empty() && data.error.is_none(),
            move || load_wiki_data(PathBuf::from(pile_path_clone)),
        );

        ctx.grid(|g| {
            g.place(10, |ctx| {
                ctx.text_field(&mut state.pile_path);
            });
            g.place(2, |ctx| {
                let path = state.pile_path.trim().to_owned();
                widgets::load_button(
                    ctx,
                    &mut state.data,
                    "Open",
                    move || load_wiki_data(PathBuf::from(path)),
                );
            });
            let data = state.data.value();
            if let Some(err) = &data.error {
                g.full(|ctx| {
                    let color = ctx.visuals().error_fg_color;
                    ctx.label(
                        egui::RichText::new(err.as_str()).color(color).monospace(),
                    );
                });
            }
        });

        // Graph (outside the grid so it can use full card width).
        let data = state.data.value();
        if !data.space.is_empty() {
            if state.graph.is_none() {
                state.graph = Some(WikiGraph::from_wiki(data));
            }

            if let Some(graph) = &mut state.graph {
                graph.step();
                if let Some(frag_id) = graph.show(ctx) {
                    if !state.open_pages.contains(&frag_id) {
                        state.open_pages.push(frag_id);
                    }
                }
                ctx.ctx().request_repaint();
            }
        }

        // ── floating wiki page cards ─────────────────────────────────
        let open_snapshot: Vec<Id> = state.open_pages.clone();
        let mut to_close = Vec::new();
        let mut to_open_from_link = Vec::new();

        for &frag_id in &open_snapshot {
            let frag_bytes: &[u8] = frag_id.as_ref();
            let mut frag_key = [0u8; 16];
            frag_key.copy_from_slice(frag_bytes);

            let data = state.data.value();
            // The ID might be a fragment or a version — resolve to latest either way
            let vid = data.latest_version(frag_id)
                .or_else(|| {
                    // frag_id might be a version ID — find its fragment, then latest
                    find!(
                        frag: Id,
                        pattern!(&data.space, [{ frag_id @ wiki::fragment: ?frag }])
                    )
                    .next()
                    .and_then(|f| data.latest_version(f))
                });
            let title = vid.map(|v| data.title(v)).unwrap_or("");
            let content = vid.map(|v| data.content(v)).unwrap_or("");
            let is_md = vid.map(|v| data.is_markdown(v)).unwrap_or(false); // default typst, not markdown

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
