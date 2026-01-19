#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.33"
//! hifitime = "4.2.3"
//! rapier2d = "0.18"
//! triblespace = "0.7.0"
//! ```

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use hifitime::Epoch;
use rapier2d::prelude::*;
use triblespace::core::blob::schemas::longstring::LongString;
use triblespace::core::blob::schemas::simplearchive::SimpleArchive;
use triblespace::core::id::Id;
use triblespace::core::metadata;
use triblespace::core::repo::pile::{Pile, PileReader};
use triblespace::core::repo::{
    head as branch_head, message as commit_message, parent as commit_parent,
    short_message as commit_short_message, signed_by as commit_signed_by,
    timestamp as commit_timestamp, BlobStore, BlobStoreGet, BlobStoreList, BlobStoreMeta,
    BranchStore,
};
use triblespace::core::trible::TribleSet;
use triblespace::core::value::schemas::ed25519 as ed;
use triblespace::core::value::schemas::hash::{Blake3, Handle};
use triblespace::core::value::schemas::time::NsTAIInterval;
use triblespace::core::value::RawValue;
use triblespace::core::value::Value;
use triblespace::macros::{find, pattern};
use triblespace::prelude::View;
use GORBIE::cards::with_padding;
use GORBIE::dataflow::ComputedState;
use GORBIE::md;
use GORBIE::notebook;
use GORBIE::widgets;
use GORBIE::NotebookCtx;

const HISTOGRAM_MIN_BUCKET_EXP: u32 = 6; // 64B (pile record alignment).
const HISTOGRAM_MAX_BUCKET_EXP: u32 = 36; // 64 GiB and above go into the last bucket.
const HISTOGRAM_BUCKET_COUNT: usize =
    (HISTOGRAM_MAX_BUCKET_EXP - HISTOGRAM_MIN_BUCKET_EXP + 1) as usize;

#[derive(Clone, Debug)]
struct BlobInfo {
    hash: RawValue,
    timestamp_ms: Option<u64>,
    length: Option<u64>,
}

#[derive(Clone, Debug)]
struct BranchInfo {
    id: Id,
    name: Option<String>,
    head: Option<RawValue>,
}

#[derive(Clone, Debug)]
struct CommitInfo {
    parents: Vec<RawValue>,
    summary: String,
    message: Option<String>,
    author: Option<RawValue>,
    timestamp_ms: Option<u64>,
}

#[derive(Clone, Debug)]
struct CommitGraph {
    order: Vec<RawValue>,
    commits: HashMap<RawValue, CommitInfo>,
    truncated: bool,
}

#[derive(Clone, Debug)]
struct BlobStats {
    oldest_ts: Option<u64>,
    newest_ts: Option<u64>,
    valid_blobs: u64,
    total_bytes: u64,
    buckets: Vec<(u64, u64)>,
    saw_underflow: bool,
    saw_overflow: bool,
}

#[derive(Clone, Debug)]
struct PileSnapshot {
    path: PathBuf,
    file_len: u64,
    reader: PileReader<Blake3>,
    blob_order: Vec<RawValue>,
    blob_stats: BlobStats,
    branches: Vec<BranchInfo>,
    commit_graph: CommitGraph,
}

#[derive(Clone, Debug)]
struct SummaryTuning {
    enabled: bool,
    size_level: f32,
    blob_level: f32,
    avg_blob_level: f32,
    age_level: f32,
    branch_level: f32,
    zoom: f32,
    sample_rate: f32,
    insert_rate: f32,
}

impl Default for SummaryTuning {
    fn default() -> Self {
        Self {
            enabled: false,
            size_level: 0.6,
            blob_level: 0.6,
            avg_blob_level: 0.5,
            age_level: 0.4,
            branch_level: 0.3,
            zoom: 1.0,
            sample_rate: 0.0,
            insert_rate: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SummaryLevels {
    size: f32,
    blob: f32,
    avg_blob: f32,
    age: f32,
    branch: f32,
    zoom: f32,
    sample_rate: f32,
    insert_rate: f32,
}

impl SummaryLevels {
    fn quantized(self) -> [u16; 8] {
        [
            quantize_level_u16(self.size),
            quantize_level_u16(self.blob),
            quantize_level_u16(self.avg_blob),
            quantize_level_u16(self.age),
            quantize_level_u16(self.branch),
            quantize_level_u16(self.zoom),
            quantize_level_u16(self.sample_rate),
            quantize_level_u16(self.insert_rate),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SummarySimFingerprint {
    data_hash: u64,
    width: u32,
    height: u32,
    levels: [u16; 8],
}

impl SummarySimFingerprint {
    fn new(snapshot: &PileSnapshot, levels: SummaryLevels, rect: egui::Rect) -> Self {
        let width = rect.width().round().max(1.0) as u32;
        let height = rect.height().round().max(1.0) as u32;
        Self {
            data_hash: snapshot_hash(snapshot),
            width,
            height,
            levels: levels.quantized(),
        }
    }

    fn seed(self) -> u64 {
        let mut seed = self.data_hash;
        for level in self.levels {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(u64::from(level));
        }
        seed
    }
}

#[derive(Clone, Copy, Debug)]
struct PileBlock {
    size: f32,
    age_level: f32,
    has_branch: bool,
    seed: u64,
}

#[derive(Clone, Copy, Debug)]
struct SimLayout {
    half_width: f32,
    ground_offset: f32,
    wall_thickness: f32,
    spawn_y: f32,
}

impl SimLayout {
    fn from_rect(rect: egui::Rect, zoom: f32) -> Self {
        let ground_offset = 2.0;
        let wall_thickness = 6.0;
        let height = (rect.height() - ground_offset - 2.0).max(40.0);
        let half_width = (rect.width() * 0.5).max(30.0);
        let zoom = zoom.clamp(0.35, 1.0);
        let spawn_y = height * 0.72 / zoom;
        Self {
            half_width,
            ground_offset,
            wall_thickness,
            spawn_y,
        }
    }
}

#[derive(Clone, Debug)]
struct BlockInstance {
    handle: RigidBodyHandle,
    block: PileBlock,
}

struct PileSimulation {
    layout: SimLayout,
    pending: VecDeque<PileBlock>,
    blocks: Vec<BlockInstance>,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: BroadPhase,
    narrow_phase: NarrowPhase,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    gravity: Vector<f32>,
    integration_parameters: IntegrationParameters,
    spawn_accumulator: f32,
    spawn_interval: f32,
    settled_frames: u32,
    settled: bool,
    rng: Lcg,
}

impl std::fmt::Debug for PileSimulation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PileSimulation")
            .field("pending", &self.pending.len())
            .field("blocks", &self.blocks.len())
            .field("settled_frames", &self.settled_frames)
            .field("settled", &self.settled)
            .finish()
    }
}

impl PileSimulation {
    fn new(blocks: Vec<PileBlock>, layout: SimLayout, seed: u64, spawn_interval: f32) -> Self {
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();

        let ground_handle = bodies.insert(
            RigidBodyBuilder::fixed()
                .translation(vector![0.0, 0.0])
                .build(),
        );
        let ground = ColliderBuilder::cuboid(layout.half_width * 8.0, layout.wall_thickness)
            .friction(0.9)
            .build();
        colliders.insert_with_parent(ground, ground_handle, &mut bodies);

        let pending = VecDeque::from(blocks);
        let rng = Lcg::new(seed);
        let mut simulation = Self {
            layout,
            pending,
            blocks: Vec::new(),
            bodies,
            colliders,
            pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: BroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            gravity: vector![0.0, -980.0],
            integration_parameters: IntegrationParameters::default(),
            spawn_accumulator: 0.0,
            spawn_interval: spawn_interval.max(0.005),
            settled_frames: 0,
            settled: false,
            rng,
        };
        simulation.spawn_next();
        simulation
    }

    fn spawn_next(&mut self) {
        let Some(block) = self.pending.pop_front() else {
            return;
        };
        let half = block.size * 0.5;
        let spawn_span = (self.layout.half_width - half).max(half);
        let x = self.rng.range_f32(spawn_span * 0.15, spawn_span);
        let y = self.layout.spawn_y + self.rng.range_f32(half * 0.4, half * 1.6);
        let body = RigidBodyBuilder::dynamic()
            .translation(vector![x, y])
            .linear_damping(1.7)
            .angular_damping(2.2)
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::cuboid(half, half)
            .friction(0.9)
            .restitution(0.0)
            .density(1.0)
            .build();
        self.colliders
            .insert_with_parent(collider, handle, &mut self.bodies);
        self.blocks.push(BlockInstance { handle, block });
    }

    fn step(&mut self, dt: f32) -> bool {
        if self.settled {
            return false;
        }

        let dt = dt.clamp(1.0 / 240.0, 1.0 / 30.0);
        self.spawn_accumulator += dt;
        while self.spawn_accumulator >= self.spawn_interval && !self.pending.is_empty() {
            self.spawn_accumulator -= self.spawn_interval;
            self.spawn_next();
        }

        self.integration_parameters.dt = dt;
        self.pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            None,
            &(),
            &(),
        );

        if self.pending.is_empty() && self.is_settled() {
            self.settled = true;
        }

        !self.settled
    }

    fn is_settled(&mut self) -> bool {
        let mut max_speed = 0.0_f32;
        for block in &self.blocks {
            if let Some(body) = self.bodies.get(block.handle) {
                max_speed = max_speed.max(body.linvel().norm());
            }
        }

        if max_speed < 5.0 {
            self.settled_frames += 1;
        } else {
            self.settled_frames = 0;
        }

        self.settled_frames > 20
    }
}

#[derive(Debug, Default)]
struct SummarySimState {
    fingerprint: Option<SummarySimFingerprint>,
    sim: Option<PileSimulation>,
}

#[derive(Clone, Copy, Debug)]
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.state >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        let value = self.next_u32();
        value as f32 / u32::MAX as f32
    }

    fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }
}

fn quantize_level_u16(level: f32) -> u16 {
    (level.clamp(0.0, 1.0) * 1000.0).round() as u16
}

fn hex_prefix(bytes: impl AsRef<[u8]>, prefix_len: usize) -> String {
    let bytes = bytes.as_ref();
    let prefix_len = prefix_len.min(bytes.len());
    let mut out = String::with_capacity(prefix_len * 2);
    for byte in bytes.iter().take(prefix_len) {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

fn blob_info(reader: &PileReader<Blake3>, hash: RawValue) -> BlobInfo {
    let handle = Value::<Handle<Blake3, SimpleArchive>>::new(hash);
    let meta = reader.metadata(handle).ok().flatten();
    let (timestamp_ms, length) = match meta {
        Some(meta) => (Some(meta.timestamp), Some(meta.length)),
        None => (None, None),
    };
    BlobInfo {
        hash,
        timestamp_ms,
        length,
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GiB", b / GB)
    } else if b >= MB {
        format!("{:.2} MiB", b / MB)
    } else if b >= KB {
        format!("{:.2} KiB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn format_compact_unit(value: f64, unit: &str) -> String {
    if value >= 10.0 {
        format!("{value:.0}{unit}")
    } else {
        format!("{value:.1}{unit}")
    }
}

fn format_bytes_compact(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format_compact_unit(b / GB, "G")
    } else if b >= MB {
        format_compact_unit(b / MB, "M")
    } else if b >= KB {
        format_compact_unit(b / KB, "K")
    } else {
        format!("{bytes}B")
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn format_age(now_ms: u64, ts_ms: u64) -> String {
    let delta_ms = now_ms.saturating_sub(ts_ms);
    let delta_s = delta_ms / 1000;
    if delta_s < 60 {
        format!("{delta_s}s ago")
    } else if delta_s < 60 * 60 {
        format!("{}m ago", delta_s / 60)
    } else if delta_s < 24 * 60 * 60 {
        format!("{}h ago", delta_s / (60 * 60))
    } else {
        format!("{}d ago", delta_s / (24 * 60 * 60))
    }
}

fn format_age_compact(now_ms: u64, ts_ms: u64) -> String {
    let delta_ms = now_ms.saturating_sub(ts_ms);
    let delta_s = delta_ms / 1000;
    if delta_s < 60 {
        format!("{delta_s}s")
    } else if delta_s < 60 * 60 {
        format!("{}m", delta_s / 60)
    } else if delta_s < 24 * 60 * 60 {
        format!("{}h", delta_s / (60 * 60))
    } else {
        format!("{}d", delta_s / (24 * 60 * 60))
    }
}

fn normalize_log2(value: u64, min_exp: f32, max_exp: f32) -> f32 {
    let value = (value.max(1) as f32).log2();
    ((value - min_exp) / (max_exp - min_exp)).clamp(0.0, 1.0)
}

fn expand_card_rect(rect: egui::Rect, padding: egui::Margin) -> egui::Rect {
    egui::Rect::from_min_max(
        rect.min - padding.left_top(),
        rect.max + padding.right_bottom(),
    )
}

const SUMMARY_PANEL_PADDING: f32 = 6.0;
const SUMMARY_SIM_ASPECT_RATIO: f32 = 2.0;

fn summary_panel_height(width: f32) -> f32 {
    let inner_width = (width - SUMMARY_PANEL_PADDING * 2.0).max(0.0);
    let sim_width = inner_width;
    let sim_height = if SUMMARY_SIM_ASPECT_RATIO > 0.0 {
        sim_width / SUMMARY_SIM_ASPECT_RATIO
    } else {
        sim_width
    };
    sim_height + SUMMARY_PANEL_PADDING * 2.0
}

fn summary_panel_base(
    ui: &mut egui::Ui,
    width: f32,
    bg_color: egui::Color32,
    padding: egui::Margin,
) -> egui::Rect {
    let height = summary_panel_height(width);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let full_rect = expand_card_rect(rect, padding);
    ui.painter()
        .with_clip_rect(full_rect)
        .rect_filled(full_rect, 0.0, bg_color);

    rect
}

fn draw_cobweb(
    painter: &egui::Painter,
    corner: egui::Pos2,
    size: f32,
    x_axis: egui::Vec2,
    y_axis: egui::Vec2,
    color: egui::Color32,
) {
    let stroke = egui::Stroke::new(1.0, color);
    let steps = 6;
    let rings = 3;
    let angle_span = std::f32::consts::FRAC_PI_2;

    for ring in 1..=rings {
        let radius = size * ring as f32 / rings as f32;
        let mut points = Vec::with_capacity(steps + 1);
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let angle = t * angle_span;
            let dx = angle.cos() * radius;
            let dy = angle.sin() * radius;
            points.push(corner + x_axis * dx + y_axis * dy);
        }
        painter.add(egui::Shape::line(points, stroke));
    }

    let spokes = 4;
    for idx in 0..spokes {
        let t = idx as f32 / (spokes - 1) as f32;
        let angle = t * angle_span;
        let dx = angle.cos() * size;
        let dy = angle.sin() * size;
        painter.line_segment([corner, corner + x_axis * dx + y_axis * dy], stroke);
    }
}

fn draw_sprout(painter: &egui::Painter, base: egui::Pos2, height: f32, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.0, color);
    let tip = base + egui::vec2(0.0, -height);
    painter.line_segment([base, tip], stroke);

    let leaf_span = height * 0.35;
    let leaf_y = base.y - height * 0.6;
    painter.line_segment(
        [
            egui::pos2(base.x, leaf_y),
            egui::pos2(base.x - leaf_span, leaf_y - leaf_span * 0.4),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(base.x, leaf_y),
            egui::pos2(base.x + leaf_span, leaf_y - leaf_span * 0.3),
        ],
        stroke,
    );
    painter.circle_filled(tip, 1.5, color);
}

fn rotated_rect_points(center: egui::Pos2, size: f32, angle: f32) -> [egui::Pos2; 4] {
    let half = size * 0.5;
    let (sin, cos) = angle.sin_cos();
    let corners = [
        egui::vec2(-half, -half),
        egui::vec2(half, -half),
        egui::vec2(half, half),
        egui::vec2(-half, half),
    ];

    let mut out = [egui::Pos2::ZERO; 4];
    for (idx, corner) in corners.iter().enumerate() {
        let rotated = egui::vec2(
            corner.x * cos - corner.y * sin,
            corner.x * sin + corner.y * cos,
        );
        out[idx] = center + rotated;
    }

    out
}

fn top_edge_indices(points: &[egui::Pos2; 4]) -> (usize, usize) {
    let mut best = (0usize, 1usize, (points[0].y + points[1].y) * 0.5);
    for idx in 1..4 {
        let a = idx;
        let b = (idx + 1) % 4;
        let a_pos = points[a];
        let b_pos = points[b];
        let avg_y = (a_pos.y + b_pos.y) * 0.5;
        if avg_y < best.2 {
            best = (a, b, avg_y);
        }
    }
    (best.0, best.1)
}

fn normalize_vec2(vec: egui::Vec2) -> egui::Vec2 {
    let len = vec.length();
    if len > f32::EPSILON {
        vec / len
    } else {
        vec
    }
}

fn corner_axes(points: &[egui::Pos2; 4], corner: usize) -> (egui::Vec2, egui::Vec2) {
    let next = (corner + 1) % 4;
    let prev = (corner + 3) % 4;
    let axis_a = normalize_vec2(points[next] - points[corner]);
    let axis_b = normalize_vec2(points[prev] - points[corner]);
    (axis_a, axis_b)
}

fn snapshot_hash(snapshot: &PileSnapshot) -> u64 {
    let mut hasher = DefaultHasher::new();
    snapshot.path.hash(&mut hasher);
    snapshot.file_len.hash(&mut hasher);
    snapshot.blob_order.len().hash(&mut hasher);
    snapshot.branches.len().hash(&mut hasher);
    snapshot.blob_stats.valid_blobs.hash(&mut hasher);
    snapshot.blob_stats.total_bytes.hash(&mut hasher);
    snapshot.blob_stats.oldest_ts.hash(&mut hasher);
    snapshot.blob_stats.newest_ts.hash(&mut hasher);
    if let Some(blob) = snapshot.blob_order.first() {
        blob.hash(&mut hasher);
    }
    if let Some(blob) = snapshot.blob_order.last() {
        blob.hash(&mut hasher);
    }
    if let Some(branch) = snapshot.branches.first() {
        branch.id.hash(&mut hasher);
        branch.head.hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_u64(hash: RawValue) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&hash[..8]);
    u64::from_le_bytes(bytes)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn target_sample_count(blob_count: usize, level: f32) -> usize {
    if blob_count <= 3 {
        return blob_count;
    }
    let min_samples = 12usize.min(blob_count);
    let max_samples = 72usize.min(blob_count);
    let t = level.clamp(0.0, 1.0);
    let count = min_samples as f32 + (max_samples - min_samples) as f32 * t;
    count.round() as usize
}

fn spawn_interval_for_rate(rate: f32) -> f32 {
    let t = rate.clamp(0.0, 1.0);
    lerp(0.18, 0.02, t)
}

fn size_log_range(blobs: &[&BlobInfo]) -> (f32, f32) {
    let mut min_log = f32::MAX;
    let mut max_log = f32::MIN;
    for blob in blobs {
        let Some(length) = blob.length else {
            continue;
        };
        let log = (length.max(1) as f32).log2();
        min_log = min_log.min(log);
        max_log = max_log.max(log);
    }
    if !min_log.is_finite() || !max_log.is_finite() {
        return (0.0, 1.0);
    }
    if (max_log - min_log).abs() < f32::EPSILON {
        return (min_log, min_log + 1.0);
    }
    (min_log, max_log)
}

fn size_range(sim_rect: egui::Rect, levels: SummaryLevels) -> (f32, f32) {
    let base_min = (sim_rect.height() * 0.06).clamp(8.0, 18.0);
    let base_max = (sim_rect.height() * 0.14).clamp(14.0, 32.0);
    let size_scale = 0.75 + levels.size * 0.7;
    let variance_scale = 0.85 + levels.avg_blob * 0.7;
    let mut min_size = base_min * size_scale;
    let mut max_size = base_max * size_scale * variance_scale;
    let cap = (sim_rect.width() * 0.2).max(12.0);
    max_size = max_size.min(cap);
    min_size = min_size.min(max_size * 0.85).max(6.0);
    (min_size, max_size.max(min_size + 1.0))
}

fn select_sample<'a>(
    candidates: &[&'a BlobInfo],
    count: usize,
    branch_heads: &HashSet<RawValue>,
) -> Vec<&'a BlobInfo> {
    if count == 0 {
        return Vec::new();
    }
    let mut entries: Vec<(u64, &BlobInfo)> = candidates
        .iter()
        .copied()
        .filter(|blob| !branch_heads.contains(&blob.hash))
        .map(|blob| (hash_u64(blob.hash), blob))
        .collect();
    entries.sort_by_key(|(key, _)| *key);
    entries.truncate(count);
    entries.into_iter().map(|(_, blob)| blob).collect()
}

fn build_summary_blocks(
    snapshot: &PileSnapshot,
    levels: SummaryLevels,
    sim_rect: egui::Rect,
) -> Vec<PileBlock> {
    let candidates: Vec<BlobInfo> = snapshot
        .blob_order
        .iter()
        .map(|&hash| blob_info(&snapshot.reader, hash))
        .filter(|blob| blob.length.is_some())
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }

    let candidate_refs: Vec<&BlobInfo> = candidates.iter().collect();

    let branch_heads: HashSet<RawValue> = snapshot
        .branches
        .iter()
        .filter_map(|branch| branch.head)
        .collect();

    let sample_count = target_sample_count(candidate_refs.len(), levels.blob);
    let sample_rate = levels.sample_rate.clamp(0.0, 1.0);
    let max_samples = candidate_refs.len();
    let sample_count = if sample_rate > 0.0 {
        let extra = max_samples.saturating_sub(sample_count) as f32;
        (sample_count as f32 + extra * sample_rate).round() as usize
    } else {
        sample_count
    };
    let mut branch_blobs: Vec<&BlobInfo> = candidate_refs
        .iter()
        .copied()
        .filter(|blob| branch_heads.contains(&blob.hash))
        .collect();
    branch_blobs.sort_by_key(|blob| hash_u64(blob.hash));
    if branch_blobs.len() > sample_count {
        branch_blobs.truncate(sample_count);
    }

    let remaining = sample_count.saturating_sub(branch_blobs.len());
    let mut selected = select_sample(&candidate_refs, remaining, &branch_heads);
    selected.extend(branch_blobs);
    if selected.is_empty() {
        return Vec::new();
    }

    selected.sort_by_key(|blob| (blob.timestamp_ms.unwrap_or(u64::MAX), blob.hash));

    let (min_log, max_log) = size_log_range(&selected);
    let (min_size, max_size) = size_range(sim_rect, levels);
    let age_weight = 0.4 + levels.age * 0.8;

    let oldest_ts = snapshot.blob_stats.oldest_ts;
    let newest_ts = snapshot.blob_stats.newest_ts;
    let age_span = match (oldest_ts, newest_ts) {
        (Some(oldest), Some(newest)) => newest.saturating_sub(oldest),
        _ => 0,
    };

    let blocks: Vec<PileBlock> = selected
        .into_iter()
        .map(|blob| {
            let length = blob.length.unwrap_or(1);
            let log = (length.max(1) as f32).log2();
            let t = ((log - min_log) / (max_log - min_log)).clamp(0.0, 1.0);
            let size = lerp(min_size, max_size, t).clamp(6.0, max_size);
            let age_level = match (blob.timestamp_ms, newest_ts, age_span) {
                (Some(ts), Some(newest), span) if span > 0 => {
                    newest.saturating_sub(ts) as f32 / span as f32
                }
                _ => 0.0,
            };
            PileBlock {
                size,
                age_level: (age_level * age_weight).clamp(0.0, 1.0),
                has_branch: branch_heads.contains(&blob.hash),
                seed: hash_u64(blob.hash),
            }
        })
        .collect();

    blocks
}

fn summary_sim_rect(panel_rect: egui::Rect) -> egui::Rect {
    panel_rect.shrink(SUMMARY_PANEL_PADDING)
}

fn draw_pile_sim(
    ui: &egui::Ui,
    sim: Option<&PileSimulation>,
    sim_rect: egui::Rect,
    levels: SummaryLevels,
    pile_color: egui::Color32,
    web_color: egui::Color32,
    sprout_color: egui::Color32,
) {
    let zoom = levels.zoom.clamp(0.35, 1.0);
    let layout = sim
        .map(|sim| sim.layout)
        .unwrap_or_else(|| SimLayout::from_rect(sim_rect, zoom));
    let painter = ui.painter().with_clip_rect(sim_rect);
    let ground_y = sim_rect.bottom() - layout.ground_offset;
    painter.hline(
        egui::Rangef::new(sim_rect.left(), sim_rect.right()),
        ground_y,
        egui::Stroke::new(1.0, pile_color),
    );

    let Some(sim) = sim else {
        return;
    };

    let center_x = sim_rect.center().x;
    let web_scale = 0.5 + levels.age * 0.9;
    let sprout_scale = 0.6 + levels.branch * 0.8;
    let stroke = egui::Stroke::new(1.0, pile_color);
    for block in &sim.blocks {
        let Some(body) = sim.bodies.get(block.handle) else {
            continue;
        };
        let pos = body.translation();
        let center = egui::pos2(center_x + pos.x * zoom, ground_y - pos.y * zoom);
        let angle = body.rotation().angle();
        let size = block.block.size * zoom;
        let points = rotated_rect_points(center, size, angle);
        for idx in 0..4 {
            painter.line_segment([points[idx], points[(idx + 1) % 4]], stroke);
        }

        if block.block.has_branch {
            let (edge_a, edge_b) = top_edge_indices(&points);
            let a = points[edge_a];
            let b = points[edge_b];
            let base = egui::pos2((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            let height = size * 0.65 * sprout_scale;
            draw_sprout(&painter, base, height, sprout_color);
        }

        if block.block.age_level > 0.55 {
            let web_size = (size * 0.35 * block.block.age_level * web_scale).max(4.0 * zoom);
            let corner = if block.block.seed & 1 == 0 { 0 } else { 1 };
            let (axis_a, axis_b) = corner_axes(&points, corner);
            draw_cobweb(
                &painter,
                points[corner],
                web_size,
                axis_a,
                axis_b,
                web_color,
            );
        }
    }
}

impl SummarySimState {
    fn update_and_draw(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &PileSnapshot,
        levels: SummaryLevels,
        sim_rect: egui::Rect,
        spawn_interval: f32,
        pile_color: egui::Color32,
        web_color: egui::Color32,
        sprout_color: egui::Color32,
    ) -> bool {
        let fingerprint = SummarySimFingerprint::new(snapshot, levels, sim_rect);
        if self.fingerprint != Some(fingerprint) {
            let blocks = build_summary_blocks(snapshot, levels, sim_rect);
            self.sim = if blocks.is_empty() {
                None
            } else {
                Some(PileSimulation::new(
                    blocks,
                    SimLayout::from_rect(sim_rect, levels.zoom),
                    fingerprint.seed(),
                    spawn_interval,
                ))
            };
            self.fingerprint = Some(fingerprint);
        }

        let mut active = false;
        if let Some(sim) = &mut self.sim {
            sim.spawn_interval = spawn_interval.max(0.005);
            active = sim.step(1.0 / 60.0);
        }

        draw_pile_sim(
            ui,
            self.sim.as_ref(),
            sim_rect,
            levels,
            pile_color,
            web_color,
            sprout_color,
        );

        active
    }
}

fn summary_overlay_row(ui: &mut egui::Ui, label: &str, value: &str, value_color: egui::Color32) {
    ui.label(egui::RichText::new(label).monospace().color(value_color));
    ui.label(egui::RichText::new(value).monospace().color(value_color));
    ui.end_row();
}

fn summary_overlay_text(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    size_value: &str,
    blob_count: usize,
    branch_count: usize,
    oldest: &str,
    newest: &str,
    path: &str,
    label_color: egui::Color32,
    pile_color: egui::Color32,
    web_color: egui::Color32,
    sprout_color: egui::Color32,
) {
    let overlay_rect = rect.shrink(8.0).with_max_y(rect.bottom() - 6.0);
    let previous_clip = ui.clip_rect();
    ui.set_clip_rect(previous_clip.intersect(rect));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(overlay_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);

            egui::Grid::new("pile-summary-overlay")
                .min_col_width(70.0)
                .spacing(egui::vec2(12.0, 0.0))
                .show(ui, |ui| {
                    summary_overlay_row(ui, "SIZE", size_value, pile_color);
                    summary_overlay_row(ui, "BLOBS", &format!("{blob_count}"), pile_color);
                    summary_overlay_row(ui, "BRANCHES", &format!("{branch_count}"), sprout_color);
                    summary_overlay_row(ui, "OLDEST", oldest, web_color);
                    summary_overlay_row(ui, "NEWEST", newest, web_color);
                });

            ui.add_space(1.0);
            ui.label(egui::RichText::new("PATH").monospace().color(label_color));
            ui.add(
                egui::Label::new(egui::RichText::new(path).monospace().color(label_color))
                    .truncate()
                    .wrap_mode(egui::TextWrapMode::Truncate),
            );
        },
    );
    ui.set_clip_rect(previous_clip);
}

fn extend_panel_background(
    ui: &mut egui::Ui,
    panel_rect: egui::Rect,
    bg_color: egui::Color32,
    padding: egui::Margin,
) {
    let content_rect = ui.min_rect();
    if content_rect.bottom() <= panel_rect.bottom() {
        return;
    }

    let fill_rect = egui::Rect::from_min_max(
        egui::pos2(panel_rect.left() - padding.leftf(), panel_rect.bottom()),
        egui::pos2(
            panel_rect.right() + padding.rightf(),
            content_rect.bottom() + padding.bottomf(),
        ),
    );
    ui.painter()
        .with_clip_rect(fill_rect)
        .rect_filled(fill_rect, 0.0, bg_color);
}

fn summary_status_panel(
    ui: &mut egui::Ui,
    width: f32,
    message: &str,
    accent: egui::Color32,
    bg_color: egui::Color32,
    padding: egui::Margin,
) {
    let rect = summary_panel_base(ui, width, bg_color, padding);
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(8.0))
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 2.0);
            ui.label(egui::RichText::new(message).monospace().color(accent));
        },
    );
}

fn open_pile(path: &PathBuf) -> Result<Pile, String> {
    let mut pile: Pile = Pile::open(path).map_err(|err| err.to_string())?;
    pile.restore().map_err(|err| err.to_string())?;
    Ok(pile)
}

fn snapshot_pile(pile: &mut Pile, path: &PathBuf) -> Result<PileSnapshot, String> {
    let file_len = std::fs::metadata(path)
        .map_err(|err| err.to_string())?
        .len();

    let reader = pile.reader().map_err(|err| err.to_string())?;

    let mut blob_entries = Vec::new();
    let mut oldest_ts: Option<u64> = None;
    let mut newest_ts: Option<u64> = None;
    let mut valid_blobs = 0u64;
    let mut total_bytes = 0u64;
    let mut buckets = vec![(0u64, 0u64); HISTOGRAM_BUCKET_COUNT];
    let mut saw_underflow = false;
    let mut saw_overflow = false;

    for handle in reader.blobs().map(|res| res.map_err(|err| err.to_string())) {
        let handle = handle?;
        let meta = reader
            .metadata(handle)
            .map_err(|_infallible| "metadata() failed".to_owned())?;
        let (timestamp_ms, length) = match meta {
            Some(meta) => (Some(meta.timestamp), Some(meta.length)),
            None => (None, None),
        };
        if let Some(timestamp_ms) = timestamp_ms {
            oldest_ts = Some(oldest_ts.map_or(timestamp_ms, |oldest| oldest.min(timestamp_ms)));
            newest_ts = Some(newest_ts.map_or(timestamp_ms, |newest| newest.max(timestamp_ms)));
        }
        if let Some(length) = length {
            valid_blobs += 1;
            total_bytes = total_bytes.saturating_add(length);

            let raw_exp = length.max(1).ilog2();
            let exp = raw_exp.clamp(HISTOGRAM_MIN_BUCKET_EXP, HISTOGRAM_MAX_BUCKET_EXP);
            saw_underflow |= raw_exp < HISTOGRAM_MIN_BUCKET_EXP;
            saw_overflow |= raw_exp > HISTOGRAM_MAX_BUCKET_EXP;
            let idx = (exp - HISTOGRAM_MIN_BUCKET_EXP) as usize;
            if let Some(bucket) = buckets.get_mut(idx) {
                bucket.0 += 1;
                bucket.1 = bucket.1.saturating_add(length);
            }
        }

        blob_entries.push((timestamp_ms, handle.raw));
    }

    blob_entries
        .sort_by(|(ts_a, hash_a), (ts_b, hash_b)| ts_b.cmp(ts_a).then_with(|| hash_a.cmp(hash_b)));
    let blob_order = blob_entries.into_iter().map(|(_, hash)| hash).collect();

    let blob_stats = BlobStats {
        oldest_ts,
        newest_ts,
        valid_blobs,
        total_bytes,
        buckets,
        saw_underflow,
        saw_overflow,
    };

    let mut branches = Vec::new();
    let branch_iter = pile.branches().map_err(|err| err.to_string())?;
    for id in branch_iter {
        let id = id.map_err(|err| err.to_string())?;
        let branch_meta = pile.head(id).map_err(|err| err.to_string())?;
        let mut name = None;
        let mut head = None;
        if let Some(branch_meta) = branch_meta {
            if let Ok(metadata_set) = reader.get::<TribleSet, SimpleArchive>(branch_meta) {
                name = find!(
                    (shortname: String),
                    pattern!(&metadata_set, [{ metadata::shortname: ?shortname }])
                )
                .into_iter()
                .map(|(shortname,)| shortname)
                .next();
                head = find!(
                    (commit_head: Value<Handle<Blake3, SimpleArchive>>),
                    pattern!(&metadata_set, [{ branch_head: ?commit_head }])
                )
                .into_iter()
                .map(|(commit_head,)| commit_head.raw)
                .next();
            }
        }
        branches.push(BranchInfo { id, name, head });
    }

    let commit_heads: Vec<RawValue> = branches.iter().filter_map(|branch| branch.head).collect();
    let commit_graph = build_commit_graph(&reader, &commit_heads);

    Ok(PileSnapshot {
        path: path.clone(),
        file_len,
        reader,
        blob_order,
        blob_stats,
        branches,
        commit_graph,
    })
}

const DAG_MAX_COMMITS: usize = 240;

fn build_commit_graph(reader: &impl BlobStoreGet<Blake3>, heads: &[RawValue]) -> CommitGraph {
    let mut commits = HashMap::new();
    let mut order = Vec::new();
    let mut queue = VecDeque::new();
    let mut queued = HashSet::new();
    let mut truncated = false;

    for head in heads {
        if queued.insert(*head) {
            queue.push_back(*head);
        }
    }

    while let Some(commit) = queue.pop_front() {
        if commits.contains_key(&commit) {
            continue;
        }
        if commits.len() >= DAG_MAX_COMMITS {
            truncated = true;
            break;
        }
        let info = commit_info(reader, commit);
        let parents = info.parents.clone();
        commits.insert(commit, info);
        order.push(commit);
        for parent in parents {
            if queued.insert(parent) {
                queue.push_back(parent);
            }
        }
    }

    CommitGraph {
        order,
        commits,
        truncated,
    }
}

fn commit_summary(short_message: Option<&str>, long_message: Option<&str>) -> String {
    let summary = short_message
        .and_then(|line| {
            let line = line.trim();
            if line.is_empty() {
                None
            } else {
                Some(line)
            }
        })
        .or_else(|| {
            long_message.and_then(|msg| msg.lines().map(str::trim).find(|line| !line.is_empty()))
        })
        .unwrap_or("commit");
    summary.to_owned()
}

fn commit_timestamp_ms(interval: Value<NsTAIInterval>) -> Option<u64> {
    let (_, upper): (Epoch, Epoch) = interval.from_value();
    let ms = upper.to_unix_milliseconds();
    if ms.is_finite() {
        Some(ms.max(0.0).round() as u64)
    } else {
        None
    }
}

fn commit_info(reader: &impl BlobStoreGet<Blake3>, commit: RawValue) -> CommitInfo {
    let handle = Value::<Handle<Blake3, SimpleArchive>>::new(commit);
    let Ok(metadata_set) = reader.get::<TribleSet, SimpleArchive>(handle) else {
        return CommitInfo {
            parents: Vec::new(),
            summary: "commit".to_owned(),
            message: None,
            author: None,
            timestamp_ms: None,
        };
    };

    let parents: Vec<RawValue> = find!(
        (parent_handle: Value<Handle<Blake3, SimpleArchive>>),
        pattern!(&metadata_set, [{ commit_parent: ?parent_handle }])
    )
    .into_iter()
    .map(|(parent_handle,)| parent_handle.raw)
    .collect();

    let short_message = find!(
        (short_message: String),
        pattern!(&metadata_set, [{ commit_short_message: ?short_message }])
    )
    .into_iter()
    .map(|(short_message,)| short_message)
    .next();

    let long_message = find!(
        (message_handle: Value<Handle<Blake3, LongString>>),
        pattern!(&metadata_set, [{ commit_message: ?message_handle }])
    )
    .into_iter()
    .map(|(message_handle,)| message_handle)
    .next()
    .and_then(|message_handle| reader.get::<View<str>, LongString>(message_handle).ok())
    .map(|view| view.to_string());

    let summary = commit_summary(short_message.as_deref(), long_message.as_deref());
    let message = long_message.or(short_message);

    let author = find!(
        (pubkey: Value<ed::ED25519PublicKey>),
        pattern!(&metadata_set, [{ commit_signed_by: ?pubkey }])
    )
    .into_iter()
    .map(|(pubkey,)| pubkey.raw)
    .next();

    let timestamp_ms = find!(
        (ts: Value<NsTAIInterval>),
        pattern!(&metadata_set, [{ commit_timestamp: ?ts }])
    )
    .into_iter()
    .map(|(ts,)| ts)
    .next()
    .and_then(commit_timestamp_ms);

    CommitInfo {
        parents,
        summary,
        message,
        author,
        timestamp_ms,
    }
}

#[derive(Clone, Debug)]
struct CommitLayout {
    positions: HashMap<RawValue, (usize, usize)>,
    lane_count: usize,
}

fn layout_commit_graph(graph: &CommitGraph, heads: &[RawValue]) -> CommitLayout {
    let mut lane_by_commit = HashMap::new();
    let mut next_lane = 0usize;

    for head in heads {
        if lane_by_commit.insert(*head, next_lane).is_none() {
            next_lane += 1;
        }
    }

    let mut positions = HashMap::new();
    for (row, commit) in graph.order.iter().enumerate() {
        let lane = *lane_by_commit.entry(*commit).or_insert_with(|| {
            let lane = next_lane;
            next_lane += 1;
            lane
        });
        positions.insert(*commit, (lane, row));
        if let Some(info) = graph.commits.get(commit) {
            for (idx, parent) in info.parents.iter().enumerate() {
                lane_by_commit.entry(*parent).or_insert_with(|| {
                    if idx == 0 {
                        lane
                    } else {
                        let lane = next_lane;
                        next_lane += 1;
                        lane
                    }
                });
            }
        }
    }

    CommitLayout {
        positions,
        lane_count: next_lane.max(1),
    }
}

fn commit_lane_color(index: usize) -> egui::Color32 {
    const COLORS: [egui::Color32; 5] = [
        egui::Color32::from_rgb(95, 210, 85),
        egui::Color32::from_rgb(80, 160, 245),
        egui::Color32::from_rgb(245, 165, 65),
        egui::Color32::from_rgb(80, 200, 200),
        egui::Color32::from_rgb(235, 90, 90),
    ];
    COLORS[index % COLORS.len()]
}

fn text_width(ui: &egui::Ui, text: &str, style: &egui::TextStyle) -> f32 {
    let font_id = style.resolve(ui.style());
    ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font_id, ui.visuals().text_color())
            .size()
            .x
    })
}

fn truncate_to_width(ui: &egui::Ui, text: &str, max_width: f32, style: &egui::TextStyle) -> String {
    if text.is_empty() || max_width <= 0.0 {
        return String::new();
    }
    if text_width(ui, text, style) <= max_width {
        return text.to_owned();
    }

    let mut trimmed = text.to_owned();
    while !trimmed.is_empty() {
        trimmed.pop();
        let candidate = format!("{trimmed}...");
        if text_width(ui, &candidate, style) <= max_width {
            return candidate;
        }
    }

    if text_width(ui, "...", style) <= max_width {
        "...".to_owned()
    } else {
        String::new()
    }
}

fn commit_detail_line(now_ms: u64, info: &CommitInfo) -> String {
    let mut parts = Vec::new();
    if let Some(author) = info.author {
        parts.push(format!("by {}", hex_prefix(author, 6)));
    }
    if let Some(timestamp_ms) = info.timestamp_ms {
        parts.push(format_age_compact(now_ms, timestamp_ms));
    }
    parts.join("  ")
}

fn draw_commit_dag(ui: &mut egui::Ui, snapshot: &PileSnapshot) {
    let mut branch_heads: Vec<(String, RawValue)> = snapshot
        .branches
        .iter()
        .filter_map(|branch| {
            branch.head.map(|head| {
                let label = branch
                    .name
                    .clone()
                    .unwrap_or_else(|| hex_prefix(branch.id, 6));
                (label, head)
            })
        })
        .collect();

    if branch_heads.is_empty() {
        md!(ui, "_No branch heads found._");
        return;
    }

    branch_heads.sort_by(|a, b| a.0.cmp(&b.0));

    let mut head_labels: HashMap<RawValue, Vec<String>> = HashMap::new();
    let mut head_order = Vec::new();
    let mut seen_heads = HashSet::new();
    for (label, head) in branch_heads {
        head_labels.entry(head).or_default().push(label);
        if seen_heads.insert(head) {
            head_order.push(head);
        }
    }

    let graph = &snapshot.commit_graph;
    if graph.order.is_empty() {
        md!(ui, "_No commits found._");
        return;
    }

    let layout = layout_commit_graph(graph, &head_order);
    let line_height = ui.text_style_height(&egui::TextStyle::Small);
    let card_padding = egui::vec2(6.0, 3.0);
    let card_height = (line_height * 2.0 + card_padding.y * 2.0).max(20.0);
    let row_height = (card_height + 6.0).max(18.0);
    let lane_width = 18.0;
    let node_radius = 4.0;
    let label_gap = 8.0;
    let card_width = 240.0;

    let label_width = head_order
        .iter()
        .filter_map(|head| head_labels.get(head))
        .map(|labels| labels.join(", "))
        .map(|label| text_width(ui, &label, &egui::TextStyle::Small))
        .fold(0.0, f32::max);

    let rows = graph.order.len();
    let lanes_width = layout.lane_count as f32 * lane_width;
    let left_padding = node_radius + 6.0;
    let top_padding = (card_height * 0.5 + 4.0).max(node_radius + 4.0);
    let width = left_padding + lanes_width + label_width + label_gap + card_width + 12.0;
    let height = top_padding + (rows as f32 * row_height) + 4.0;

    let head_set: HashSet<RawValue> = head_labels.keys().copied().collect();

    egui::ScrollArea::horizontal()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let available_width = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(width.max(available_width), height),
                egui::Sense::hover(),
            );
            let painter = ui.painter_at(rect);
            let origin = rect.min + egui::vec2(left_padding, top_padding);
            let card_origin_x = origin.x + lanes_width + label_width + label_gap;

            let lane_colors: Vec<egui::Color32> =
                (0..layout.lane_count).map(commit_lane_color).collect();
            let base_stroke = ui.visuals().widgets.noninteractive.bg_stroke;
            let base_fill = ui.visuals().widgets.noninteractive.bg_fill;
            let font_id = egui::TextStyle::Small.resolve(ui.style());
            let now_ms = now_ms();

            for (commit, info) in &graph.commits {
                let Some(&(lane, row)) = layout.positions.get(commit) else {
                    continue;
                };
                let start = origin + egui::vec2(lane as f32 * lane_width, row as f32 * row_height);
                let stroke = egui::Stroke::new(1.0, lane_colors[lane % lane_colors.len()]);

                for parent in &info.parents {
                    let Some(&(parent_lane, parent_row)) = layout.positions.get(parent) else {
                        continue;
                    };
                    let end = origin
                        + egui::vec2(
                            parent_lane as f32 * lane_width,
                            parent_row as f32 * row_height,
                        );
                    let mid = egui::pos2(start.x, end.y);
                    painter.line_segment([start, mid], stroke);
                    painter.line_segment([mid, end], stroke);
                }
            }

            for commit in &graph.order {
                let Some(&(lane, row)) = layout.positions.get(commit) else {
                    continue;
                };
                let pos = origin + egui::vec2(lane as f32 * lane_width, row as f32 * row_height);
                let is_head = head_set.contains(commit);
                let color = lane_colors[lane % lane_colors.len()];
                let stroke = if is_head {
                    egui::Stroke::new(1.5, color)
                } else {
                    egui::Stroke::new(1.0, color)
                };
                let fill = if is_head { color } else { base_fill };

                painter.circle_filled(pos, node_radius, fill);
                painter.circle_stroke(pos, node_radius, stroke);

                let Some(info) = graph.commits.get(commit) else {
                    continue;
                };
                let card_rect = egui::Rect::from_min_size(
                    egui::pos2(card_origin_x, pos.y - card_height * 0.5),
                    egui::vec2(card_width, card_height),
                );
                painter.rect_filled(card_rect, 6.0, base_fill);
                painter.rect_stroke(card_rect, 6.0, base_stroke, egui::StrokeKind::Inside);

                let summary = truncate_to_width(
                    ui,
                    &info.summary,
                    card_width - card_padding.x * 2.0,
                    &egui::TextStyle::Small,
                );
                let detail_line = commit_detail_line(now_ms, info);
                let detail = truncate_to_width(
                    ui,
                    &detail_line,
                    card_width - card_padding.x * 2.0,
                    &egui::TextStyle::Small,
                );
                let text_pos = card_rect.min + card_padding;
                painter.text(
                    text_pos,
                    egui::Align2::LEFT_TOP,
                    summary,
                    font_id.clone(),
                    ui.visuals().text_color(),
                );
                if !detail.is_empty() {
                    painter.text(
                        text_pos + egui::vec2(0.0, line_height + 1.0),
                        egui::Align2::LEFT_TOP,
                        detail,
                        font_id.clone(),
                        ui.visuals().weak_text_color(),
                    );
                }

                let response = ui.interact(
                    card_rect,
                    ui.id().with((commit, "card")),
                    egui::Sense::hover(),
                );
                if response.hovered() {
                    let hash = hex_prefix(commit, 32);
                    let mut tooltip = format!("hash: {hash}");
                    if let Some(message) = info.message.as_deref() {
                        tooltip.push_str(&format!("\nmessage: {message}"));
                    }
                    if let Some(author) = info.author {
                        tooltip.push_str(&format!("\nauthor: {}", hex_prefix(author, 12)));
                    }
                    if let Some(timestamp_ms) = info.timestamp_ms {
                        tooltip.push_str(&format!("\nwhen: {}", format_age(now_ms, timestamp_ms)));
                    }
                    response.on_hover_text(tooltip);
                }
            }

            for head in &head_order {
                let Some(&(lane, row)) = layout.positions.get(head) else {
                    continue;
                };
                let Some(labels) = head_labels.get(head) else {
                    continue;
                };
                let label = labels.join(", ");
                let pos = origin + egui::vec2(lane as f32 * lane_width, row as f32 * row_height);
                let label_pos = pos + egui::vec2(node_radius + 6.0, 0.0);
                let color = lane_colors[lane % lane_colors.len()];
                painter.text(
                    label_pos,
                    egui::Align2::LEFT_CENTER,
                    label,
                    font_id.clone(),
                    color,
                );
            }

            if graph.truncated {
                let label = format!("Showing first {} commits.", graph.order.len());
                let pos = rect.max - egui::vec2(0.0, 6.0);
                painter.text(
                    pos,
                    egui::Align2::RIGHT_BOTTOM,
                    label,
                    font_id,
                    base_stroke.color,
                );
            }
        });
}

struct InspectorState {
    pile_path: String,
    pile: Option<Pile>,
    pile_open_path: Option<PathBuf>,
    max_rows: usize,
    blob_page: usize,
    histogram_bytes: bool,
    snapshot: ComputedState<Option<Result<PileSnapshot, String>>>,
    summary_sim: SummarySimState,
}

impl std::fmt::Debug for InspectorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InspectorState")
            .field("pile_path", &self.pile_path)
            .field("pile_open", &self.pile.is_some())
            .field("pile_open_path", &self.pile_open_path)
            .field("max_rows", &self.max_rows)
            .field("blob_page", &self.blob_page)
            .field("histogram_bytes", &self.histogram_bytes)
            .field("snapshot", &self.snapshot)
            .field("summary_sim", &self.summary_sim)
            .finish()
    }
}

impl Drop for InspectorState {
    fn drop(&mut self) {
        if let Some(pile) = self.pile.take() {
            let _ = pile.close();
        }
    }
}

impl Default for InspectorState {
    fn default() -> Self {
        Self {
            pile_path: "./repo.pile".to_owned(),
            pile: None,
            pile_open_path: None,
            max_rows: 360,
            blob_page: 0,
            histogram_bytes: false,
            snapshot: ComputedState::default(),
            summary_sim: SummarySimState::default(),
        }
    }
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    let default_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./repo.pile".to_owned());
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;

    let inspector = nb.state(
        "inspector",
        InspectorState {
            pile_path: default_path,
            pile: None,
            pile_open_path: None,
            max_rows: 360,
            blob_page: 0,
            histogram_bytes: false,
            snapshot: ComputedState::default(),
            summary_sim: SummarySimState::default(),
        },
        move |ui, state| {
            with_padding(ui, padding, |ui| {
                md!(
                    ui,
                    "# Triblespace pile inspector\n\nOpen a `.pile` file on disk and inspect its blob and branch indices.\n\nTip: pass a path as the first CLI arg to prefill this field."
                );

                ui.horizontal(|ui| {
                    ui.label("Pile path:");
                    ui.add(widgets::TextField::singleline(&mut state.pile_path));
                    ui.label("Items/page:");
                    ui.add(
                        widgets::NumberField::new(&mut state.max_rows)
                            .constrain_value(&|_, next| next.clamp(10, 10_000))
                            .speed(10.0),
                    );

                    let open_clicked = ui.add(widgets::Button::new("Open pile")).clicked();
                    if open_clicked {
                        let open_path = PathBuf::from(state.pile_path.trim());
                        match open_pile(&open_path) {
                            Ok(mut new_pile) => {
                                let snapshot = snapshot_pile(&mut new_pile, &open_path);
                                if let Some(old_pile) = state.pile.take() {
                                    let _ = old_pile.close();
                                }
                                state.pile = Some(new_pile);
                                state.pile_open_path = Some(open_path);
                                state.snapshot.set(Some(snapshot));
                            }
                            Err(err) => {
                                state.snapshot.set(Some(Err(err)));
                            }
                        }
                    }
                });

                if let (Some(pile), Some(path)) =
                    (state.pile.as_mut(), state.pile_open_path.as_ref())
                {
                    state.snapshot.set(Some(snapshot_pile(pile, path)));
                    ui.ctx().request_repaint();
                }
            });
        }
    );

    let summary_tuning = nb.state(
        "summary_tuning",
        SummaryTuning::default(),
        move |ui, tuning| {
            with_padding(ui, padding, |ui| {
                md!(ui, "## Summary knobs");
                ui.horizontal(|ui| {
                    ui.label("MODE:");
                    ui.add(widgets::ChoiceToggle::binary(
                        &mut tuning.enabled,
                        "LIVE",
                        "TUNE",
                    ));
                });
                ui.add_space(6.0);
                ui.add(
                    widgets::Slider::new(&mut tuning.zoom, 0.35..=1.0)
                        .text("ZOOM")
                        .max_decimals(2),
                );
                ui.add(
                    widgets::Slider::new(&mut tuning.sample_rate, 0.0..=1.0)
                        .text("SAMPLE")
                        .max_decimals(2),
                );
                ui.add(
                    widgets::Slider::new(&mut tuning.insert_rate, 0.0..=1.0)
                        .text("INSERT")
                        .max_decimals(2),
                );
                ui.add_space(6.0);
                ui.add_enabled_ui(tuning.enabled, |ui| {
                    ui.add(
                        widgets::Slider::new(&mut tuning.size_level, 0.0..=1.0)
                            .text("SIZE")
                            .max_decimals(2),
                    );
                    ui.add(
                        widgets::Slider::new(&mut tuning.blob_level, 0.0..=1.0)
                            .text("BLOBS")
                            .max_decimals(2),
                    );
                    ui.add(
                        widgets::Slider::new(&mut tuning.avg_blob_level, 0.0..=1.0)
                            .text("AVG BLOB")
                            .max_decimals(2),
                    );
                    ui.add(
                        widgets::Slider::new(&mut tuning.age_level, 0.0..=1.0)
                            .text("AGE")
                            .max_decimals(2),
                    );
                    ui.add(
                        widgets::Slider::new(&mut tuning.branch_level, 0.0..=1.0)
                            .text("BRANCHES")
                            .max_decimals(2),
                    );
                });
                if !tuning.enabled {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Enable TUNE to override the pile visualization.")
                            .small(),
                    );
                }
            });
        },
    );

    nb.view(move |ui| {
        let mut state = inspector.read_mut(ui);
        with_padding(ui, padding, |ui| {
            md!(ui, "## Blob size distribution");

            ui.horizontal(|ui| {
                ui.label("METRIC:");
                ui.add(widgets::ChoiceToggle::binary(
                    &mut state.histogram_bytes,
                    "COUNT",
                    "BYTES",
                ));
            });
            let histogram_bytes = state.histogram_bytes;

            state.snapshot.poll();
            let snapshot_value = state.snapshot.value().as_ref();
            let Some(result) = snapshot_value else {
                md!(ui, "_Load a pile to see the distribution._");
                return;
            };
            let Ok(snapshot) = result else {
                md!(ui, "_Load a valid pile to see the distribution._");
                return;
            };

            let stats = &snapshot.blob_stats;
            if stats.valid_blobs == 0 {
                md!(ui, "_No valid blob sizes found._");
                return;
            }

            fn bucket_start(exp: u32) -> u64 {
                1u64 << exp
            }

            fn bucket_end(exp: u32) -> u64 {
                if exp >= 63 {
                    u64::MAX
                } else {
                    (1u64 << (exp + 1)).saturating_sub(1)
                }
            }

            fn bucket_label(
                exp: u32,
                min_exp: u32,
                max_exp: u32,
                underflowed: bool,
                overflowed: bool,
            ) -> String {
                let start = bucket_start(exp);
                let prefix = if underflowed && exp == min_exp {
                    "≤"
                } else {
                    ""
                };
                let suffix = if overflowed && exp == max_exp {
                    "+"
                } else {
                    ""
                };

                if start >= (1u64 << 30) {
                    let start_g = start >> 30;
                    format!("{prefix}{start_g}G{suffix}")
                } else if start >= (1u64 << 20) {
                    let start_m = start >> 20;
                    format!("{prefix}{start_m}M{suffix}")
                } else if start >= (1u64 << 10) {
                    let start_k = start >> 10;
                    format!("{prefix}{start_k}K{suffix}")
                } else {
                    format!("{prefix}{start}B{suffix}")
                }
            }

            let y_axis = if histogram_bytes {
                widgets::HistogramYAxis::Bytes
            } else {
                widgets::HistogramYAxis::Count
            };

            let mut max_value = 0u64;
            let mut histogram_buckets: Vec<widgets::HistogramBucket<'static>> = Vec::new();
            for exp in HISTOGRAM_MIN_BUCKET_EXP..=HISTOGRAM_MAX_BUCKET_EXP {
                let idx = (exp - HISTOGRAM_MIN_BUCKET_EXP) as usize;
                let (count, bytes) = stats.buckets.get(idx).copied().unwrap_or((0, 0));
                let value = if histogram_bytes { bytes } else { count };
                max_value = max_value.max(value);

                let mut bucket = widgets::HistogramBucket::new(
                    value,
                    bucket_label(
                        exp,
                        HISTOGRAM_MIN_BUCKET_EXP,
                        HISTOGRAM_MAX_BUCKET_EXP,
                        stats.saw_underflow,
                        stats.saw_overflow,
                    ),
                );

                if value > 0 {
                    let range = if stats.saw_overflow && exp == HISTOGRAM_MAX_BUCKET_EXP {
                        let start = bucket_start(exp);
                        format!("≥ {}", format_bytes(start))
                    } else if stats.saw_underflow && exp == HISTOGRAM_MIN_BUCKET_EXP {
                        let end = bucket_end(exp);
                        format!("≤ {}", format_bytes(end))
                    } else {
                        let start = bucket_start(exp);
                        let end = bucket_end(exp);
                        format!("{}–{}", format_bytes(start), format_bytes(end))
                    };
                    let metric = if histogram_bytes {
                        format_bytes(bytes)
                    } else {
                        format!("{count}")
                    };
                    bucket = bucket.tooltip(format!("{range}\n{metric}"));
                }

                histogram_buckets.push(bucket);
            }

            if max_value == 0 {
                md!(ui, "_No data to plot._");
                return;
            }

            ui.add(
                widgets::Histogram::new(&histogram_buckets, y_axis)
                    .plot_height(80.0)
                    .max_x_labels(7),
            );

            md!(
                ui,
                "_{} blobs, {} total._",
                stats.valid_blobs,
                format_bytes(stats.total_bytes)
            );
        });
    });

    nb.view(move |ui| {
        let summary_padding = egui::Margin::ZERO;
        let mut state = inspector.read_mut(ui);
        let tuning = summary_tuning.read(ui);
        with_padding(ui, summary_padding, |ui| {
            let now_ms = now_ms();
            let bg_color = egui::Color32::from_rgb(8, 8, 8);
            let label_color = egui::Color32::from_rgb(245, 140, 45);
            let pile_color = egui::Color32::from_rgb(60, 170, 230);
            let web_color = egui::Color32::from_rgb(235, 235, 235);
            let sprout_color = egui::Color32::from_rgb(95, 210, 85);
            let accent_ok = sprout_color;
            let accent_warn = egui::Color32::from_rgb(255, 196, 0);
            let accent_error = egui::Color32::from_rgb(255, 80, 90);

            let InspectorState {
                snapshot,
                summary_sim,
                ..
            } = &mut *state;
            snapshot.poll();
            let snapshot_value = snapshot.value();
            let status_color = if snapshot.is_running() {
                accent_warn
            } else if let Some(result) = snapshot_value.as_ref() {
                match result {
                    Ok(_) => accent_ok,
                    Err(_) => accent_error,
                }
            } else {
                label_color
            };

            egui::Frame::NONE
                .inner_margin(egui::Margin::same(0))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);

                    if snapshot.is_running() {
                        let status = if snapshot_value.is_some() {
                            "Refreshing pile data."
                        } else {
                            "Loading pile data."
                        };
                        summary_status_panel(
                            ui,
                            ui.available_width(),
                            status,
                            status_color,
                            bg_color,
                            summary_padding,
                        );
                    } else {
                        match snapshot_value.as_ref() {
                            None => {
                                summary_status_panel(
                                    ui,
                                    ui.available_width(),
                                    "No pile loaded yet.",
                                    status_color,
                                    bg_color,
                                    summary_padding,
                                );
                            }
                            Some(Err(err)) => {
                                summary_status_panel(
                                    ui,
                                    ui.available_width(),
                                    &format!("{err}"),
                                    status_color,
                                    bg_color,
                                    summary_padding,
                                );
                            }
                            Some(Ok(snapshot)) => {
                                let blob_count = snapshot.blob_order.len();
                                let branch_count = snapshot.branches.len();
                                let oldest_ts = snapshot.blob_stats.oldest_ts;
                                let newest_ts = snapshot.blob_stats.newest_ts;

                                let oldest = oldest_ts
                                    .map(|ts| format_age(now_ms, ts))
                                    .unwrap_or_else(|| "—".to_owned());
                                let newest = newest_ts
                                    .map(|ts| format_age(now_ms, ts))
                                    .unwrap_or_else(|| "—".to_owned());

                                let age_span_secs = match (oldest_ts, newest_ts) {
                                    (Some(oldest_ts), Some(newest_ts)) => {
                                        newest_ts.saturating_sub(oldest_ts) / 1000
                                    }
                                    _ => 0,
                                };

                                let live_size_level = normalize_log2(snapshot.file_len, 10.0, 30.0);
                                let live_blob_level =
                                    normalize_log2(blob_count as u64 + 1, 0.0, 20.0);
                                let live_branch_level =
                                    normalize_log2(branch_count as u64 + 1, 0.0, 12.0);
                                let live_age_level = normalize_log2(age_span_secs + 1, 0.0, 20.0);
                                let avg_blob_size = if blob_count > 0 {
                                    snapshot.file_len / blob_count as u64
                                } else {
                                    0
                                };
                                let live_avg_blob_level =
                                    normalize_log2(avg_blob_size + 1, 6.0, 24.0);
                                let levels = SummaryLevels {
                                    size: if tuning.enabled {
                                        tuning.size_level
                                    } else {
                                        live_size_level
                                    },
                                    blob: if tuning.enabled {
                                        tuning.blob_level
                                    } else {
                                        live_blob_level
                                    },
                                    avg_blob: if tuning.enabled {
                                        tuning.avg_blob_level
                                    } else {
                                        live_avg_blob_level
                                    },
                                    age: if tuning.enabled {
                                        tuning.age_level
                                    } else {
                                        live_age_level
                                    },
                                    branch: if tuning.enabled {
                                        tuning.branch_level
                                    } else {
                                        live_branch_level
                                    },
                                    zoom: tuning.zoom,
                                    sample_rate: tuning.sample_rate,
                                    insert_rate: tuning.insert_rate,
                                };

                                let panel_rect = summary_panel_base(
                                    ui,
                                    ui.available_width(),
                                    bg_color,
                                    summary_padding,
                                );
                                let sim_rect = summary_sim_rect(panel_rect);
                                let spawn_interval = spawn_interval_for_rate(levels.insert_rate);
                                let active = summary_sim.update_and_draw(
                                    ui,
                                    snapshot,
                                    levels,
                                    sim_rect,
                                    spawn_interval,
                                    pile_color,
                                    web_color,
                                    sprout_color,
                                );
                                if active {
                                    ui.ctx().request_repaint();
                                }
                                summary_overlay_text(
                                    ui,
                                    panel_rect,
                                    &format_bytes(snapshot.file_len),
                                    blob_count,
                                    branch_count,
                                    &oldest,
                                    &newest,
                                    &snapshot.path.display().to_string(),
                                    label_color,
                                    pile_color,
                                    web_color,
                                    sprout_color,
                                );
                                extend_panel_background(ui, panel_rect, bg_color, summary_padding);
                            }
                        }
                    }
                });
        });
    });

    nb.view(move |ui| {
        let mut state = inspector.read_mut(ui);
        with_padding(ui, padding, |ui| {
            md!(ui, "## Commit graph");

            state.snapshot.poll();
            let snapshot_value = state.snapshot.value().as_ref();
            let Some(result) = snapshot_value else {
                md!(ui, "_Load a pile to see the commit graph._");
                return;
            };
            let Ok(snapshot) = result else {
                md!(ui, "_Load a valid pile to see the commit graph._");
                return;
            };

            if snapshot.branches.is_empty() {
                md!(ui, "_No branches found._");
                return;
            }

            draw_commit_dag(ui, snapshot);
        });
    });

    nb.view(move |ui| {
        let mut state = inspector.read_mut(ui);
        with_padding(ui, padding, |ui| {
            md!(ui, "## Blobs");

            state.snapshot.poll();
            let snapshot_value = state.snapshot.value().as_ref();
            let Some(result) = snapshot_value else {
                md!(ui, "_Load a pile to see blobs._");
                return;
            };
            let Ok(snapshot) = result else {
                md!(ui, "_Load a valid pile to see blobs._");
                return;
            };

            if snapshot.blob_order.is_empty() {
                md!(ui, "_No blobs found._");
                return;
            }

            let page_size = state.max_rows.max(1);
            let mut page = state.blob_page;
            let total = snapshot.blob_order.len();
            let total_pages = total.saturating_add(page_size - 1) / page_size;
            page = page.min(total_pages.saturating_sub(1));
            let mut page_next = page;
            let start = page * page_size;
            let end = (start + page_size).min(total);
            let display_start = start + 1;

            ui.scope(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 2.0);

                let page_display = page + 1;
                ui.horizontal(|ui| {
                    let page_label = if total_pages == 0 {
                        "Page —".to_owned()
                    } else {
                        format!("Page {page_display} of {total_pages}")
                    };
                    ui.label(egui::RichText::new(page_label).small());

                    if total_pages > 0 {
                        ui.label(
                            egui::RichText::new(format!("{display_start}–{end} of {total}"))
                                .small(),
                        );
                    }
                });

                if total_pages > 0 {
                    let mut page_select = page_display;
                    ui.scope(|ui| {
                        ui.spacing_mut().slider_width = ui.available_width();
                        ui.add(
                            widgets::Slider::new(&mut page_select, 1..=total_pages)
                                .show_value(false),
                        );
                    });
                    page_next = page_select.saturating_sub(1);
                }

                let now_ms = now_ms();
                let branch_heads: HashSet<RawValue> = snapshot
                    .branches
                    .iter()
                    .filter_map(|branch| branch.head)
                    .collect();
                let card_spacing = egui::vec2(4.0, 4.0);
                let card_height = ui.text_style_height(&egui::TextStyle::Small) + 6.0;
                let min_card_width = 56.0;
                let card_fill = ui.visuals().widgets.noninteractive.bg_fill;
                let outline_stroke =
                    egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
                let card_rounding = egui::CornerRadius::same(4);
                let branch_color = egui::Color32::from_rgb(95, 210, 85);
                let items: Vec<BlobInfo> = snapshot
                    .blob_order
                    .iter()
                    .skip(start)
                    .take(page_size)
                    .map(|&hash| blob_info(&snapshot.reader, hash))
                    .collect();
                let available_width = ui.available_width();
                let columns = ((available_width + card_spacing.x)
                    / (min_card_width + card_spacing.x))
                    .floor()
                    .max(1.0) as usize;
                let columns_f = columns as f32;
                let card_width = ((available_width - card_spacing.x * (columns_f - 1.0))
                    / columns_f)
                    .floor()
                    .max(1.0);
                let card_size = egui::vec2(card_width, card_height);

                ui.add_space(2.0);
                ui.spacing_mut().item_spacing = card_spacing;

                let render_blob_card = |ui: &mut egui::Ui, blob: &BlobInfo| {
                    let is_head = branch_heads.contains(&blob.hash);
                    let (rect, response) = ui.allocate_exact_size(card_size, egui::Sense::click());
                    ui.painter().rect_filled(rect, card_rounding, card_fill);

                    if is_head {
                        let band_inset = outline_stroke.width;
                        let band_rect = rect.shrink(band_inset);
                        let inset_u8 = band_inset.round().clamp(0.0, u8::MAX as f32) as u8;
                        let band_rounding = card_rounding - inset_u8;
                        let line_height =
                            f32::from(band_rounding.nw).min(band_rect.height()).max(1.0);
                        let line_rect = egui::Rect::from_min_max(
                            band_rect.min,
                            egui::pos2(band_rect.max.x, band_rect.min.y + line_height),
                        );
                        ui.painter().with_clip_rect(line_rect).rect_filled(
                            band_rect,
                            band_rounding,
                            branch_color,
                        );
                    }

                    ui.painter().rect_stroke(
                        rect,
                        card_rounding,
                        outline_stroke,
                        egui::StrokeKind::Inside,
                    );

                    let content_rect = rect.shrink2(egui::vec2(4.0, 1.0));
                    let mut card_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(content_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    card_ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);

                    let size_text = blob
                        .length
                        .map(format_bytes_compact)
                        .unwrap_or_else(|| "invalid".to_owned());
                    let age_text = blob
                        .timestamp_ms
                        .map(|timestamp| format_age_compact(now_ms, timestamp))
                        .unwrap_or_else(|| "--".to_owned());
                    let line = format!("{size_text} {age_text}");
                    card_ui.add(
                        egui::Label::new(egui::RichText::new(line).monospace().small())
                            .truncate()
                            .wrap_mode(egui::TextWrapMode::Truncate),
                    );

                    let hash_text = hex_prefix(blob.hash, 32);
                    let response = response.on_hover_ui(|ui| {
                        ui.label(egui::RichText::new(format!("hash: {hash_text}")).monospace());
                    });
                    if response.clicked() {
                        ui.ctx().copy_text(hash_text);
                    }
                };

                let row_width = available_width;
                for row in items.chunks(columns) {
                    let row_layout = if ui.layout().prefer_right_to_left() {
                        egui::Layout::right_to_left(egui::Align::Center)
                    } else {
                        egui::Layout::left_to_right(egui::Align::Center)
                    };
                    ui.allocate_ui_with_layout(
                        egui::vec2(row_width, card_height),
                        row_layout,
                        |ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(card_spacing.x, 0.0);
                            for blob in row {
                                render_blob_card(ui, blob);
                            }
                        },
                    );
                }
            });

            if total_pages > 0 {
                state.blob_page = page_next.min(total_pages.saturating_sub(1));
            } else {
                state.blob_page = 0;
            }
        });
    });
}
