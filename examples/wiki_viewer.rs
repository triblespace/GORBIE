#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = "..", features = ["triblespace", "markdown"] }
//! egui = "0.33"
//! eframe = "0.33"
//! triblespace = "0.34.1"
//! ed25519-dalek = "2"
//! parking_lot = "0.12"
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use cubecl::prelude::*;
use cubecl::wgpu::{WgpuDevice, WgpuRuntime};
use egui::{self};
use triblespace::core::blob::Blob;
use triblespace::core::id::Id;
use triblespace::core::metadata;
use triblespace::core::repo::pile::Pile;
use triblespace::core::repo::{BlobStore, BlobStoreGet, BranchStore, Repository, Workspace};
use triblespace::core::trible::TribleSet;
use triblespace::core::value::schemas::hash::{Blake3, Handle};
use triblespace::core::value::{TryToValue, Value};
use triblespace::macros::{find, pattern};
use triblespace::prelude::blobschemas::{FileBytes, LongString};
use triblespace::prelude::View;

use GORBIE::notebook;
use GORBIE::prelude::*;

// ── wiki schema (mirrors playground/faculties/wiki.rs) ────────────────
const WIKI_BRANCH_NAME: &str = "wiki";
const FILES_BRANCH_NAME: &str = "files";
const KIND_VERSION_ID: Id = triblespace::macros::id_hex!("1AA0310347EDFED7874E8BFECC6438CF");
const KIND_FILE: Id = triblespace::macros::id_hex!("1F9C9DCA69504452F318BA11E81D47D1");
const TAG_ARCHIVED_ID: Id = triblespace::macros::id_hex!("480CB6A663C709478A26A8B49F366C3F");

mod wiki {
    use triblespace::prelude::*;
    attributes! {
        "EBFC56D50B748E38A14F5FC768F1B9C1" as fragment: valueschemas::GenId;
        "6DBBE746B7DD7A4793CA098AB882F553" as content: valueschemas::Handle<valueschemas::Blake3, blobschemas::LongString>;
        "78BABEF1792531A2E51A372D96FE5F3E" as title: valueschemas::Handle<valueschemas::Blake3, blobschemas::LongString>;
        "DEAFB7E307DF72389AD95A850F24BAA5" as links_to: valueschemas::GenId;
    }
}

mod file {
    use triblespace::prelude::*;
    attributes! {
        "C1E3A12230595280F22ABEB8733D082C" as content: valueschemas::Handle<valueschemas::Blake3, blobschemas::FileBytes>;
        "AA6AB6F5E68F3A9D95681251C2B9DAFA" as name: valueschemas::Handle<valueschemas::Blake3, blobschemas::LongString>;
        "BFE2C88ECD13D56F80967C343FC072EE" as mime: valueschemas::ShortString;
    }
}

type TextHandle = Value<Handle<Blake3, LongString>>;
type FileHandle = Value<Handle<Blake3, FileBytes>>;

fn fmt_id(id: Id) -> String {
    format!("{id:x}")
}

// ── live wiki connection ─────────────────────────────────────────────

struct WikiLive {
    wiki_space: TribleSet,
    files_space: TribleSet,
    wiki_ws: Workspace<Pile<Blake3>>,
    files_ws: Option<Workspace<Pile<Blake3>>>,
}

impl WikiLive {
    fn open(path: &std::path::Path) -> Result<Self, String> {
        let mut pile = Pile::<Blake3>::open(path).map_err(|e| format!("open pile: {e:?}"))?;
        if let Err(err) = pile.restore() {
            let _ = pile.close();
            return Err(format!("restore: {err:?}"));
        }
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand_core06::OsRng);
        let mut repo = Repository::new(pile, signing_key, TribleSet::new())
            .map_err(|e| format!("repo: {e:?}"))?;
        repo.storage_mut()
            .refresh()
            .map_err(|e| format!("refresh: {e:?}"))?;

        let wiki_bid = find_branch(&mut repo, WIKI_BRANCH_NAME)
            .ok_or_else(|| "no 'wiki' branch found".to_string())?;
        let mut wiki_ws = repo
            .pull(wiki_bid)
            .map_err(|e| format!("pull wiki: {e:?}"))?;
        let wiki_space = wiki_ws
            .checkout(..)
            .map_err(|e| format!("checkout wiki: {e:?}"))?
            .into_facts();

        let (files_space, files_ws) =
            if let Some(files_bid) = find_branch(&mut repo, FILES_BRANCH_NAME) {
                let mut files_ws = repo
                    .pull(files_bid)
                    .map_err(|e| format!("pull files: {e:?}"))?;
                let fs = files_ws
                    .checkout(..)
                    .map_err(|e| format!("checkout files: {e:?}"))?
                    .into_facts();
                (fs, Some(files_ws))
            } else {
                eprintln!("[files] no 'files' branch found — file links will not resolve");
                (TribleSet::new(), None)
            };

        Ok(WikiLive {
            wiki_space,
            files_space,
            wiki_ws,
            files_ws,
        })
    }

    fn text(&mut self, h: TextHandle) -> String {
        self.wiki_ws
            .get::<View<str>, LongString>(h)
            .map(|v| {
                let s: &str = v.as_ref();
                s.to_string()
            })
            .unwrap_or_default()
    }

    fn file_text(&mut self, h: TextHandle) -> String {
        self.files_ws
            .as_mut()
            .and_then(|ws| ws.get::<View<str>, LongString>(h).ok())
            .map(|v| {
                let s: &str = v.as_ref();
                s.to_string()
            })
            .unwrap_or_default()
    }

    // ── queries (all on-demand via find!) ─────────────────────────────

    /// Resolve a hex prefix to a fragment ID. Matches both version and fragment IDs.
    /// Returns None if no match or ambiguous.
    fn resolve_prefix(&self, prefix: &str) -> Option<Id> {
        let needle = prefix.trim().to_lowercase();
        let mut matches = Vec::new();
        let mut seen_frags = std::collections::HashSet::new();
        for (vid, frag) in find!(
            (vid: Id, frag: Id),
            pattern!(&self.wiki_space, [{ ?vid @ metadata::tag: &KIND_VERSION_ID, wiki::fragment: ?frag }])
        ) {
            if format!("{vid:x}").starts_with(&needle) {
                matches.push(frag); // resolve version to its fragment
            }
            if seen_frags.insert(frag) && format!("{frag:x}").starts_with(&needle) {
                matches.push(frag);
            }
        }
        matches.sort();
        matches.dedup();
        if matches.len() == 1 { Some(matches[0]) } else { None }
    }

    fn to_fragment(&self, id: Id) -> Option<Id> {
        if self.latest_version(id).is_some() {
            return Some(id);
        }
        find!(frag: Id, pattern!(&self.wiki_space, [{ id @ wiki::fragment: ?frag }])).next()
    }

    /// All versions of a fragment, sorted newest-first.
    fn version_history(&self, fragment_id: Id) -> Vec<Id> {
        let mut versions: Vec<(Id, i128)> = find!(
            (vid: Id, ts: (i128, i128)),
            pattern!(&self.wiki_space, [{
                ?vid @
                metadata::tag: &KIND_VERSION_ID,
                wiki::fragment: &fragment_id,
                metadata::created_at: ?ts,
            }])
        )
        .map(|(vid, ts)| (vid, ts.0))
        .collect();
        versions.sort_by(|a, b| b.1.cmp(&a.1)); // newest first
        versions.into_iter().map(|(vid, _)| vid).collect()
    }

    fn latest_version(&self, fragment_id: Id) -> Option<Id> {
        find!(
            (vid: Id, ts: (i128, i128)),
            pattern!(&self.wiki_space, [{
                ?vid @
                metadata::tag: &KIND_VERSION_ID,
                wiki::fragment: &fragment_id,
                metadata::created_at: ?ts,
            }])
        )
        .max_by_key(|(_, ts)| ts.0)
        .map(|(vid, _)| vid)
    }

    fn title(&mut self, vid: Id) -> String {
        find!(h: TextHandle, pattern!(&self.wiki_space, [{ vid @ wiki::title: ?h }]))
            .next()
            .map(|h| self.text(h))
            .unwrap_or_default()
    }

    fn content(&mut self, vid: Id) -> String {
        find!(h: TextHandle, pattern!(&self.wiki_space, [{ vid @ wiki::content: ?h }]))
            .next()
            .map(|h| self.text(h))
            .unwrap_or_default()
    }

    fn tags(&self, vid: Id) -> Vec<Id> {
        find!(tag: Id, pattern!(&self.wiki_space, [{ vid @ metadata::tag: ?tag }]))
            .filter(|t| *t != KIND_VERSION_ID)
            .collect()
    }

    fn is_archived(&self, vid: Id) -> bool {
        self.tags(vid).contains(&TAG_ARCHIVED_ID)
    }

    fn links(&self, vid: Id) -> Vec<Id> {
        find!(target: Id, pattern!(&self.wiki_space, [{ vid @ wiki::links_to: ?target }])).collect()
    }

    fn fragments_sorted(&mut self) -> Vec<(Id, Id)> {
        let mut latest: BTreeMap<Id, (Id, i128)> = BTreeMap::new();
        for (vid, frag, ts) in find!(
            (vid: Id, frag: Id, ts: (i128, i128)),
            pattern!(&self.wiki_space, [{
                ?vid @
                metadata::tag: &KIND_VERSION_ID,
                wiki::fragment: ?frag,
                metadata::created_at: ?ts,
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
            .filter(|(_, vid)| !self.is_archived(*vid))
            .collect();
        entries.sort_by(|a, b| {
            self.title(a.1)
                .to_lowercase()
                .cmp(&self.title(b.1).to_lowercase())
        });
        entries
    }

    // ── file resolution ──────────────────────────────────────────────

    fn resolve_file(&mut self, hex: &str) -> Option<(FileHandle, String)> {
        let (entity_id, handle) = if hex.len() == 32 {
            let eid = Id::from_hex(hex)?;
            let h = find!(
                h: FileHandle,
                pattern!(&self.files_space, [{
                    eid @ metadata::tag: &KIND_FILE, file::content: ?h,
                }])
            )
            .next()?;
            (eid, h)
        } else if hex.len() == 64 {
            let hash_str = format!("blake3:{hex}");
            let hash_value: Value<triblespace::core::value::schemas::hash::Hash<Blake3>> =
                hash_str.as_str().try_to_value().ok()?;
            let content_handle: FileHandle = hash_value.into();
            let eid = find!(
                eid: Id,
                pattern!(&self.files_space, [{
                    ?eid @ metadata::tag: &KIND_FILE, file::content: &content_handle,
                }])
            )
            .next()?;
            (eid, content_handle)
        } else {
            return None;
        };

        let name = find!(
            h: TextHandle,
            pattern!(&self.files_space, [{ entity_id @ file::name: ?h }])
        )
        .next()
        .map(|h| self.file_text(h))
        .unwrap_or_else(|| "file".to_string());

        Some((handle, name))
    }

    fn open_file(&mut self, hex: &str) {
        let Some((handle, name)) = self.resolve_file(hex) else {
            eprintln!("[files] could not resolve files:{hex}");
            return;
        };

        let ws = match self.files_ws.as_mut() {
            Some(ws) => ws,
            None => {
                eprintln!("[files] no files workspace available");
                return;
            }
        };

        let result = (|| -> Result<PathBuf, String> {
            let blob: Blob<FileBytes> = ws.get(handle).map_err(|e| format!("get blob: {e:?}"))?;
            let tmp_dir = std::env::temp_dir().join("liora-files");
            std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("mkdir: {e}"))?;
            let path = tmp_dir.join(&name);
            std::fs::write(&path, &*blob.bytes).map_err(|e| format!("write: {e}"))?;
            Ok(path)
        })();

        match result {
            Ok(path) => {
                eprintln!("[files] opening: {}", path.display());
                let _ = std::process::Command::new("open").arg(&path).spawn();
            }
            Err(e) => eprintln!("[files] error: {e}"),
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────

fn find_branch(repo: &mut Repository<Pile<Blake3>>, name: &str) -> Option<Id> {
    let reader = repo.storage_mut().reader().ok()?;
    for item in repo.storage_mut().branches().ok()? {
        let bid = item.ok()?;
        let head = repo.storage_mut().head(bid).ok()??;
        let meta: TribleSet = reader.get(head).ok()?;
        let branch_name = find!(
            (h: TextHandle),
            pattern!(&meta, [{ metadata::name: ?h }])
        )
        .into_iter()
        .next()
        .and_then(|(h,)| reader.get::<View<str>, LongString>(h).ok())
        .map(|v| {
            let s: &str = v.as_ref();
            s.to_string()
        });
        if branch_name.as_deref() == Some(name) {
            return Some(bid);
        }
    }
    None
}

// ── GPU force-directed layout kernel ──────────────────────────────────

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
        let repulsion = 200000.0f32;
        let attraction = 0.3f32;
        let damping = 0.75f32;
        let max_force = 30.0f32;
        let gravity = 0.001f32;

        let ix = (i * 2) as usize;
        let iy = ix + 1;
        let px = pos[ix];
        let py = pos[iy];

        let mut fx = 0.0f32;
        let mut fy = 0.0f32;

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

        // Count degree to normalize attraction (high-degree nodes
        // don't collapse into a ball).
        let mut degree = 1.0f32;
        for e in 0..edge_count {
            let ea = edges[(e * 2) as usize];
            let eb = edges[(e * 2 + 1) as usize];
            if ea == i || eb == i { degree += 1.0f32; }
        }
        let norm_attraction = attraction / degree;

        for e in 0..edge_count {
            let ea = edges[(e * 2) as usize];
            let eb = edges[(e * 2 + 1) as usize];
            if ea == i {
                let bx = (eb * 2) as usize;
                fx += (pos[bx] - px) * norm_attraction;
                fy += (pos[bx + 1] - py) * norm_attraction;
            }
            if eb == i {
                let ax = (ea * 2) as usize;
                fx += (pos[ax] - px) * norm_attraction;
                fy += (pos[ax + 1] - py) * norm_attraction;
            }
        }

        fx -= px * gravity;
        fy -= py * gravity;

        let fmag = (fx * fx + fy * fy).sqrt();
        if fmag > max_force {
            let scale = max_force / fmag;
            fx *= scale;
            fy *= scale;
        }

        let vx = (vel[ix] + fx) * damping;
        let vy = (vel[iy] + fy) * damping;
        vel[ix] = vx;
        vel[iy] = vy;
        pos_out[ix] = px + vx;
        pos_out[iy] = py + vy;
    }
}

// ── FDEB (force-directed edge bundling) kernel ────────────────────────

#[cube(launch)]
fn fdeb_step_kernel(
    points: &Array<f32>,
    points_out: &mut Array<f32>,
    edge_count: u32,
    k: u32,
    step_size: f32,
    spring_k: f32,
) {
    let tid = ABSOLUTE_POS as u32;
    let total = edge_count * k;
    if tid < total {
        let e = tid / k;
        let p = tid % k;
        let ix = (tid * 2) as usize;
        let px = points[ix];
        let py = points[ix + 1];

        if p == 0u32 || p == k - 1u32 {
            points_out[ix] = px;
            points_out[ix + 1] = py;
        } else {
            let my0 = (e * k * 2) as usize;
            let my1 = ((e * k + k - 1u32) * 2) as usize;
            let my_p0x = points[my0];
            let my_p0y = points[my0 + 1];
            let my_p1x = points[my1];
            let my_p1y = points[my1 + 1];
            let my_dx = my_p1x - my_p0x;
            let my_dy = my_p1y - my_p0y;
            let my_len = (my_dx * my_dx + my_dy * my_dy).sqrt().max(1.0f32);
            let my_mx = (my_p0x + my_p1x) * 0.5f32;
            let my_my = (my_p0y + my_p1y) * 0.5f32;

            // Smoothing spring: penalizes curvature (local).
            let prev_ix = ((e * k + p - 1u32) * 2) as usize;
            let next_ix = ((e * k + p + 1u32) * 2) as usize;
            let fx_smooth = ((points[prev_ix] - px) + (points[next_ix] - px)) * spring_k;
            let fy_smooth = ((points[prev_ix + 1] - py) + (points[next_ix + 1] - py)) * spring_k;

            // Straight-line restoring: pulls back toward the unbent
            // position on the original edge (global shape anchor).
            let t = p as f32 / (k - 1u32) as f32;
            let sx = my_p0x + (my_p1x - my_p0x) * t;
            let sy = my_p0y + (my_p1y - my_p0y) * t;
            let straighten = 0.03f32;
            let fx_straight = (sx - px) * straighten;
            let fy_straight = (sy - py) * straighten;

            // Electrostatic: unit-vector pull toward corresponding
            // point on each compatible edge, averaged over compatible
            // count so total magnitude is bounded ≤ 1.
            let mut fx_elec = 0.0f32;
            let mut fy_elec = 0.0f32;

            for other in 0u32..edge_count {
                if other != e {
                    let o0 = (other * k * 2) as usize;
                    let o1 = ((other * k + k - 1u32) * 2) as usize;
                    let o_p0x = points[o0];
                    let o_p0y = points[o0 + 1];
                    let o_p1x = points[o1];
                    let o_p1y = points[o1 + 1];
                    let o_dx = o_p1x - o_p0x;
                    let o_dy = o_p1y - o_p0y;
                    let o_len = (o_dx * o_dx + o_dy * o_dy).sqrt().max(1.0f32);
                    let o_mx = (o_p0x + o_p1x) * 0.5f32;
                    let o_my = (o_p0y + o_p1y) * 0.5f32;

                    let dot = my_dx * o_dx + my_dy * o_dy;
                    let cos_a = dot / (my_len * o_len);
                    let c_angle = cos_a * cos_a;

                    let lavg = (my_len + o_len) * 0.5f32;
                    let lmin = my_len.min(o_len);
                    let lmax = my_len.max(o_len);
                    let c_scale = 2.0f32 / (lavg / lmin + lmax / lavg);

                    let mdx = my_mx - o_mx;
                    let mdy = my_my - o_my;
                    let mdist = (mdx * mdx + mdy * mdy).sqrt();
                    let c_pos = lavg / (lavg + mdist);

                    let compat = c_angle * c_scale * c_pos;

                    if compat > 0.2f32 {
                        let corr_p = if dot >= 0.0f32 { p } else { k - 1u32 - p };
                        let other_ix = ((other * k + corr_p) * 2) as usize;
                        let ox = points[other_ix];
                        let oy = points[other_ix + 1];
                        let ddx = ox - px;
                        let ddy = oy - py;
                        let d = (ddx * ddx + ddy * ddy).sqrt().max(0.1f32);
                        fx_elec += (ddx / d) * compat;
                        fy_elec += (ddy / d) * compat;
                    }
                }
            }

            // Cap electrostatic magnitude so it can't overwhelm
            // the straight-line restoring force.
            let elec_mag = (fx_elec * fx_elec + fy_elec * fy_elec).sqrt();
            let max_elec = 3.0f32;
            if elec_mag > max_elec {
                let s = max_elec / elec_mag;
                fx_elec *= s;
                fy_elec *= s;
            }

            let fx = fx_smooth + fx_straight + fx_elec;
            let fy = fy_smooth + fy_straight + fy_elec;
            points_out[ix] = px + fx * step_size;
            points_out[ix + 1] = py + fy * step_size;
        }
    }
}

// ── force-directed graph ──────────────────────────────────────────────

struct WikiGraph {
    nodes: Vec<GraphNode>,
    edges: Vec<(usize, usize)>,
    gpu: Option<GpuForceState>,
    /// Bundled polylines per edge (world coords). `None` = draw straight.
    polylines: Option<Vec<Vec<egui::Vec2>>>,
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
    fn from_wiki(live: &mut WikiLive) -> Self {
        let fragments = live.fragments_sorted();
        let mut frag_to_idx = BTreeMap::new();
        let mut nodes = Vec::new();

        let n = fragments.len().max(1) as f32;
        for (i, &(frag_id, vid)) in fragments.iter().enumerate() {
            let angle = (i as f32 / n) * std::f32::consts::TAU;
            let radius = 200.0 + n * 5.0;
            let title = live.title(vid);
            frag_to_idx.insert(frag_id, i);
            nodes.push(GraphNode {
                frag_id,
                label: if title.is_empty() {
                    fmt_id(frag_id)
                } else {
                    title
                },
                pos: egui::vec2(angle.cos() * radius, angle.sin() * radius),
            });
        }

        let mut seen = std::collections::HashSet::new();
        let mut edges = Vec::new();
        let mut unresolved = 0usize;
        for &(frag_id, vid) in &fragments {
            let from = frag_to_idx[&frag_id];
            for target in live.links(vid) {
                let frag_target = if frag_to_idx.contains_key(&target) {
                    Some(target)
                } else {
                    find!(
                        frag: Id,
                        pattern!(&live.wiki_space, [{ target @ wiki::fragment: ?frag }])
                    )
                    .next()
                };
                if let Some(frag) = frag_target {
                    if let Some(&to) = frag_to_idx.get(&frag) {
                        if from != to && seen.insert((from, to)) {
                            edges.push((from, to));
                        }
                    } else {
                        unresolved += 1;
                    }
                } else {
                    unresolved += 1;
                }
            }
        }
        if unresolved > 0 {
            eprintln!("[wiki] graph: {unresolved} link targets could not be resolved to fragments");
        }

        let gpu = Self::init_gpu(&nodes, &edges);
        WikiGraph { nodes, edges, gpu, polylines: None }
    }

    fn init_gpu(nodes: &[GraphNode], edges: &[(usize, usize)]) -> Option<GpuForceState> {
        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);
        let n = nodes.len();

        let mut pos_flat: Vec<f32> = Vec::with_capacity(n * 2);
        let vel_flat: Vec<f32> = vec![0.0; n * 2];
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

    fn step(&mut self) {
        let Some(gpu) = &mut self.gpu else { return };
        let n = gpu.node_count as usize;
        if n == 0 {
            return;
        }

        unsafe {
            let _ = force_step_kernel::launch::<WgpuRuntime>(
                &gpu.client,
                CubeCount::new_1d(((n as u32) + 255) / 256),
                CubeDim::new_1d(256),
                ArrayArg::from_raw_parts::<f32>(&gpu.pos_handle, n * 2, 1),
                ArrayArg::from_raw_parts::<f32>(&gpu.vel_handle, n * 2, 1),
                ArrayArg::from_raw_parts::<u32>(
                    &gpu.edges_handle,
                    gpu.edge_count.max(1) as usize * 2,
                    1,
                ),
                ScalarArg::new(gpu.node_count),
                ScalarArg::new(gpu.edge_count),
                ArrayArg::from_raw_parts::<f32>(&gpu.pos_out_handle, n * 2, 1),
            );
        }

        std::mem::swap(&mut gpu.pos_handle, &mut gpu.pos_out_handle);

        let bytes = gpu.client.read_one(gpu.pos_handle.clone());
        let positions: &[f32] = f32::from_bytes(&bytes);

        // Compute center of mass and average angular velocity,
        // then subtract to kill collective rotation.
        let mut cx = 0.0f32;
        let mut cy = 0.0f32;
        for i in 0..n {
            cx += positions[i * 2];
            cy += positions[i * 2 + 1];
        }
        cx /= n as f32;
        cy /= n as f32;

        // Compute average angular momentum around center of mass.
        let mut angular = 0.0f32;
        let mut inertia = 0.0f32;
        for (i, node) in self.nodes.iter().enumerate() {
            let px = positions[i * 2];
            let py = positions[i * 2 + 1];
            let dx = px - cx;
            let dy = py - cy;
            let vx = px - node.pos.x;
            let vy = py - node.pos.y;
            let r_sq = dx * dx + dy * dy;
            angular += dx * vy - dy * vx; // cross product = angular contribution
            inertia += r_sq;
        }
        let omega = if inertia > 1.0 { angular / inertia } else { 0.0 };

        for (i, node) in self.nodes.iter_mut().enumerate() {
            let px = positions[i * 2] - cx;
            let py = positions[i * 2 + 1] - cy;
            // Subtract rigid rotation: v_rot = omega × r = (-omega*y, omega*x)
            node.pos = egui::vec2(
                positions[i * 2] + omega * py,
                positions[i * 2 + 1] - omega * px,
            );
        }
    }

    fn is_bundled(&self) -> bool {
        self.polylines.is_some()
    }

    fn clear_bundling(&mut self) {
        self.polylines = None;
    }

    /// Force-Directed Edge Bundling (Holten & Van Wijk 2009) on GPU.
    /// Edges subdivide into K control points; each non-endpoint point
    /// is pulled by spring forces from its polyline neighbors and by
    /// electrostatic attraction from *compatible* edges (matching
    /// angle, scale, and midpoint proximity). Compatibility prevents
    /// edges from detouring through unrelated bundles.
    fn bundle_edges(&mut self) {
        const K: u32 = 17;
        const CYCLES: usize = 5;
        const ITERATIONS_START: usize = 50;
        const SPRING_K: f32 = 0.1;

        if self.edges.is_empty() {
            self.polylines = Some(Vec::new());
            return;
        }

        let e = self.edges.len() as u32;
        let total = e * K;
        let total_floats = (total * 2) as usize;

        let mut flat: Vec<f32> = Vec::with_capacity(total_floats);
        for &(a, b) in &self.edges {
            let p0 = self.nodes[a].pos;
            let p1 = self.nodes[b].pos;
            for i in 0..K {
                let t = i as f32 / (K - 1) as f32;
                let p = p0 + (p1 - p0) * t;
                flat.push(p.x);
                flat.push(p.y);
            }
        }

        // Average edge length — sets step scale so forces move control
        // points a sensible fraction of a typical edge per iteration.
        let mut len_sum = 0.0f32;
        for &(a, b) in &self.edges {
            len_sum += (self.nodes[a].pos - self.nodes[b].pos).length();
        }
        let avg_len = (len_sum / e as f32).max(1.0);
        // Step in world units. Electrostatic force is a unit vector
        // (bounded ≤ 1 after averaging), so step_size controls the
        // max displacement per iteration. Segment length ≈ avg_len/16;
        // move at most ~1/3 of a segment per step for stability.
        let segment_len = avg_len / (K - 1) as f32;
        let mut step_size = segment_len * 0.15;

        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);
        let mut pts_handle = client.create_from_slice(f32::as_bytes(&flat));
        let mut pts_out_handle = client.empty(total_floats * std::mem::size_of::<f32>());

        let mut iterations = ITERATIONS_START;
        for _cycle in 0..CYCLES {
            for _ in 0..iterations {
                unsafe {
                    let _ = fdeb_step_kernel::launch::<WgpuRuntime>(
                        &client,
                        CubeCount::new_1d((total + 255) / 256),
                        CubeDim::new_1d(256),
                        ArrayArg::from_raw_parts::<f32>(&pts_handle, total_floats, 1),
                        ArrayArg::from_raw_parts::<f32>(&pts_out_handle, total_floats, 1),
                        ScalarArg::new(e),
                        ScalarArg::new(K),
                        ScalarArg::new(step_size),
                        ScalarArg::new(SPRING_K),
                    );
                }
                std::mem::swap(&mut pts_handle, &mut pts_out_handle);
            }
            step_size *= 0.5;
            iterations = (iterations * 2 / 3).max(10);
        }

        let bytes = client.read_one(pts_handle);
        let result: &[f32] = f32::from_bytes(&bytes);

        if result.len() >= 4 {
            let has_nan = result.iter().take(K as usize * 2).any(|v| v.is_nan());
            let has_inf = result.iter().take(K as usize * 2).any(|v| v.is_infinite());
            eprintln!(
                "[fdeb] first edge: p0=({:.1},{:.1}) mid=({:.1},{:.1}) p_end=({:.1},{:.1}) | nan={} inf={} total_floats={}",
                result[0], result[1],
                result[K as usize], result[K as usize + 1],
                result[(K as usize - 1) * 2], result[(K as usize - 1) * 2 + 1],
                has_nan, has_inf, result.len(),
            );
        }

        let mut polylines = Vec::with_capacity(self.edges.len());
        for ei in 0..self.edges.len() {
            let mut poly = Vec::with_capacity(K as usize);
            for pi in 0..K as usize {
                let ix = (ei * K as usize + pi) * 2;
                poly.push(egui::vec2(result[ix], result[ix + 1]));
            }
            polylines.push(poly);
        }
        self.polylines = Some(polylines);
    }

    fn show(&self, ui: &mut egui::Ui) -> Option<Id> {
        let available = ui.available_size();
        let (response, painter) = ui.allocate_painter(
            egui::vec2(available.x, available.y.max(400.0)),
            egui::Sense::click_and_drag(),
        );
        let rect = response.rect;
        let center = rect.center();

        let view_id = ui.id().with("wiki_graph_view");
        let pan_id = view_id.with("pan");
        let zoom_id = view_id.with("zoom");

        let mut pan: egui::Vec2 = ui.ctx().memory_mut(|m| {
            *m.data
                .get_temp_mut_or_insert_with(pan_id, || egui::Vec2::ZERO)
        });
        let mut zoom: f32 = ui
            .ctx()
            .memory_mut(|m| *m.data.get_temp_mut_or_insert_with(zoom_id, || 1.0f32));

        if response.hovered() {
            // Pinch-to-zoom (trackpad) and scroll-to-zoom (mouse wheel).
            let pinch = ui.input(|i| i.zoom_delta());
            let scroll = ui.input(|i| i.smooth_scroll_delta.x);
            let zoom_factor = if pinch != 1.0 {
                pinch
            } else if scroll != 0.0 {
                (1.0 + scroll * 0.002).clamp(0.9, 1.1)
            } else {
                1.0
            };
            if zoom_factor != 1.0 {
                let old_zoom = zoom;
                zoom = (zoom * zoom_factor).clamp(0.05, 10.0);
                if let Some(hp) = response.hover_pos() {
                    let cursor_offset = hp - center - pan;
                    pan -= cursor_offset * (zoom / old_zoom - 1.0);
                }
                ui.ctx().memory_mut(|m| {
                    m.data.insert_temp(zoom_id, zoom);
                    m.data.insert_temp(pan_id, pan);
                });
                // Consume only horizontal scroll so vertical passes through to notebook.
                ui.ctx().input_mut(|i| i.smooth_scroll_delta.x = 0.0);
            }
        }

        if response.dragged() {
            pan += response.drag_delta();
            ui.ctx().memory_mut(|m| m.data.insert_temp(pan_id, pan));
        }

        let to_screen =
            |world: egui::Vec2| center + pan + egui::vec2(world.x * zoom, world.y * zoom);

        let node_radius = 6.0 * zoom.max(0.3);
        let edge_color = ui.visuals().weak_text_color();
        let node_fill = GORBIE::themes::ral(5005);
        let node_stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        let label_color = ui.visuals().text_color();
        let font_id = egui::TextStyle::Small.resolve(ui.style());

        let edge_stroke = egui::Stroke::new(0.5, edge_color);
        for (e_idx, &(a, b)) in self.edges.iter().enumerate() {
            let p1 = to_screen(self.nodes[a].pos);
            let p2 = to_screen(self.nodes[b].pos);
            if !(rect.expand(50.0).contains(p1) || rect.expand(50.0).contains(p2)) {
                continue;
            }
            match &self.polylines {
                Some(polys) => {
                    let pts: Vec<egui::Pos2> =
                        polys[e_idx].iter().map(|&p| to_screen(p)).collect();
                    painter.add(egui::Shape::line(pts, edge_stroke));
                }
                None => {
                    painter.line_segment([p1, p2], edge_stroke);
                }
            }
        }

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

// ── link interception ────────────────────────────────────────────────

enum LinkClick {
    Wiki(Id),
    File(String),
}

fn render_wiki_content(ctx: &mut CardCtx<'_>, content: &str) -> Option<LinkClick> {
    let cmd_count_before = ctx.ctx().output(|o| o.commands.len());
    ctx.typst(content);

    let mut clicked = None;
    ctx.ctx().output_mut(|o| {
        let new_commands: Vec<egui::OutputCommand> =
            o.commands.drain(cmd_count_before..).collect();
        for cmd in new_commands {
            match &cmd {
                egui::OutputCommand::OpenUrl(open_url) => {
                    if let Some(hex) = open_url.url.strip_prefix("wiki:") {
                        if let Some(id) = Id::from_hex(hex) {
                            eprintln!("[wiki] link click: wiki:{hex} → {id:x}");
                            clicked = Some(LinkClick::Wiki(id));
                        } else {
                            eprintln!("[wiki] link click: wiki:{hex} ({} chars) → failed to parse as Id (expected 32 hex chars)", hex.len());
                        }
                    } else if let Some(hex) = open_url.url.strip_prefix("files:") {
                        eprintln!("[files] link click: files:{hex}");
                        clicked = Some(LinkClick::File(hex.to_string()));
                    } else {
                        o.commands.push(cmd);
                    }
                }
                _ => o.commands.push(cmd),
            }
        }
    });
    clicked
}

// ── notebook state ───────────────────────────────────────────────────

/// An open wiki page — tracks which version is being viewed.
struct OpenPage {
    frag_id: Id,
    /// None = show latest version.
    pinned_version: Option<Id>,
}

struct BrowserState {
    pile_path: String,
    search_query: String,
    live: Option<parking_lot::Mutex<WikiLive>>,
    graph: Option<WikiGraph>,
    open_pages: Vec<OpenPage>,
    error: Option<String>,
}

impl BrowserState {
    fn new(pile_path: String) -> Self {
        Self {
            pile_path,
            search_query: String::new(),
            live: None,
            graph: None,
            open_pages: Vec::new(),
            error: None,
        }
    }

    fn load(&mut self) {
        self.graph = None;
        self.error = None;
        match WikiLive::open(std::path::Path::new(self.pile_path.trim())) {
            Ok(live) => self.live = Some(parking_lot::Mutex::new(live)),
            Err(e) => {
                self.live = None;
                self.error = Some(e);
            }
        }
    }
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

    nb.state(
        "browser",
        BrowserState::new(pile_path),
        move |ctx, state| {
            ctx.grid(|g| {
                g.place(10, |ctx| {
                    ctx.text_field(&mut state.pile_path);
                });
                g.place(2, |ctx| {
                    if ctx.button("Open").clicked() {
                        state.load();
                    }
                });
                if let Some(err) = &state.error {
                    g.full(|ctx| {
                        let color = ctx.visuals().error_fg_color;
                        ctx.label(egui::RichText::new(err.as_str()).color(color).monospace());
                    });
                }
            });

            let Some(live_mutex) = &mut state.live else {
                return;
            };
            let live = live_mutex.get_mut();

            // Search bar: open a fragment/version by hex ID or title substring.
            ctx.grid(|g| {
                g.place(10, |ctx| {
                    ctx.text_field(&mut state.search_query);
                });
                g.place(2, |ctx| {
                    if ctx.button("Go").clicked() && !state.search_query.trim().is_empty() {
                        let q = state.search_query.trim().to_string();
                        let is_hex = q.chars().all(|c| c.is_ascii_hexdigit());

                        let found = if is_hex {
                            // Hex prefix → resolve to fragment.
                            live.resolve_prefix(&q)
                        } else {
                            // Title substring search → first match.
                            let q_lower = q.to_lowercase();
                            let frags = live.fragments_sorted();
                            frags.iter()
                                .find(|(_, vid)| live.title(*vid).to_lowercase().contains(&q_lower))
                                .map(|(frag_id, _)| *frag_id)
                        };

                        if let Some(frag_id) = found {
                            if !state.open_pages.iter().any(|p| p.frag_id == frag_id) {
                                state.open_pages.push(OpenPage {
                                    frag_id,
                                    pinned_version: None,
                                });
                            }
                        }
                        state.search_query.clear();
                    }
                });
            });

            // Graph.
            if state.graph.is_none() {
                state.graph = Some(WikiGraph::from_wiki(live));
            }
            if let Some(graph) = &mut state.graph {
                ctx.grid(|g| {
                    let bundled = graph.is_bundled();
                    g.place(2, |ctx| {
                        if ctx.button(if bundled { "Re-bundle" } else { "Bundle" }).clicked() {
                            graph.bundle_edges();
                        }
                    });
                    g.place(2, |ctx| {
                        if ctx.button("Straight").clicked() {
                            graph.clear_bundling();
                        }
                    });
                });
                if !graph.is_bundled() {
                    graph.step();
                }
                if let Some(frag_id) = graph.show(ctx) {
                    if !state.open_pages.iter().any(|p| p.frag_id == frag_id) {
                        state.open_pages.push(OpenPage {
                            frag_id,
                            pinned_version: None,
                        });
                    }
                }
                ctx.ctx().request_repaint();
            }

            // ── floating wiki page cards ─────────────────────────────────
            let open_snapshot: Vec<OpenPage> = state
                .open_pages
                .iter()
                .map(|p| OpenPage {
                    frag_id: p.frag_id,
                    pinned_version: p.pinned_version,
                })
                .collect();
            let mut to_close = Vec::new();
            let mut to_open_from_link = Vec::new();
            let mut version_nav: Option<(Id, Option<Id>)> = None; // (frag_id, new_pinned)

            for page_idx in 0..open_snapshot.len() {
                let frag_id = open_snapshot[page_idx].frag_id;
                let pinned = open_snapshot[page_idx].pinned_version;
                let frag_bytes: &[u8] = frag_id.as_ref();
                let mut frag_key = [0u8; 16];
                frag_key.copy_from_slice(frag_bytes);

                let history = live.version_history(frag_id);
                let vid = pinned.or_else(|| live.latest_version(frag_id));
                if vid.is_none() {
                    eprintln!("[wiki] resolve {frag_id:x}: no versions found for fragment");
                }
                let title = vid.map(|v| live.title(v)).unwrap_or_default();
                let content = vid.map(|v| live.content(v)).unwrap_or_default();
                let current_idx = vid.and_then(|v| history.iter().position(|&h| h == v));
                let n_versions = history.len();

                ctx.push_id(frag_key, |ctx| {
                    let resp = ctx.float(|ctx| {
                        ctx.with_padding(padding, |ctx| {
                            if vid.is_none() {
                                ctx.add(egui::Label::new(
                                    egui::RichText::new("Link target not found").heading(),
                                ).wrap());
                                ctx.label(egui::RichText::new(
                                    format!("wiki:{frag_id:x}")
                                ).monospace().weak().small());
                                ctx.separator();
                                ctx.label(
                                    "This link points to an ID that doesn't exist in the wiki. \
                                     The target may have been deleted, or the link may contain a typo."
                                );
                                return;
                            }
                            ctx.add(egui::Label::new(egui::RichText::new(&title).heading()).wrap());
                            let vid_hex = vid.map(|v| format!("{v:x}")).unwrap_or_default();
                            ctx.label(egui::RichText::new(
                                format!("wiki:{frag_id:x}\nwiki:{vid_hex}")
                            ).monospace().weak().small());

                            // Version navigation bar.
                            if n_versions > 1 {
                                let vi = current_idx.unwrap_or(0);
                                let ver_label = if pinned.is_some() {
                                    format!("v{}/{}", n_versions - vi, n_versions)
                                } else {
                                    format!("latest (v{})", n_versions)
                                };
                                ctx.grid(|g| {
                                    g.place(8, |ctx| {
                                        ctx.label(
                                            egui::RichText::new(ver_label).weak().monospace(),
                                        );
                                    });
                                    g.place(1, |ctx| {
                                        if ctx.button("◀").clicked() && vi + 1 < n_versions {
                                            version_nav = Some((frag_id, Some(history[vi + 1])));
                                        }
                                    });
                                    g.place(1, |ctx| {
                                        if ctx.button("▶").clicked() {
                                            if vi > 0 {
                                                version_nav =
                                                    Some((frag_id, Some(history[vi - 1])));
                                            } else {
                                                version_nav = Some((frag_id, None));
                                            }
                                        }
                                    });
                                    if pinned.is_some() {
                                        g.place(2, |ctx| {
                                            if ctx.button("↻ latest").clicked() {
                                                version_nav = Some((frag_id, None));
                                            }
                                        });
                                    }
                                });
                            }

                            ctx.separator();

                            match render_wiki_content(ctx, &content) {
                                Some(LinkClick::Wiki(id)) => to_open_from_link.push(id),
                                Some(LinkClick::File(hex)) => {
                                    live.open_file(&hex);
                                }
                                None => {}
                            }
                        });
                    });
                    if resp.closed {
                        to_close.push(frag_id);
                    }
                });
            }

            for id in to_close {
                state.open_pages.retain(|p| p.frag_id != id);
            }
            if let Some((frag_id, new_pinned)) = version_nav {
                if let Some(page) = state.open_pages.iter_mut().find(|p| p.frag_id == frag_id) {
                    page.pinned_version = new_pinned;
                }
            }
            for id in to_open_from_link {
                let frag = live.to_fragment(id).unwrap_or_else(|| {
                    eprintln!("[wiki] link target {id:x}: could not resolve to fragment");
                    id
                });
                // Move to top if already open, otherwise open new.
                state.open_pages.retain(|p| p.frag_id != frag);
                state.open_pages.push(OpenPage {
                    frag_id: frag,
                    pinned_version: None,
                });
            }
        },
    );
}
