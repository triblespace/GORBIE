#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! rapier2d = "0.18"
//! triblespace = { path = "../../triblespace-rs" }
//! ```

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rapier2d::prelude::*;
use triblespace::core::id::Id;
use triblespace::core::repo::pile::Pile;
use triblespace::core::repo::{BlobStore, BlobStoreList, BlobStoreMeta, BranchStore};
use triblespace::core::value::RawValue;
use GORBIE::dataflow::ComputedState;
use GORBIE::md;
use GORBIE::notebook;
use GORBIE::state;
use GORBIE::view;
use GORBIE::widgets;

#[derive(Clone, Debug)]
struct BlobInfo {
    hash: RawValue,
    timestamp_ms: Option<u64>,
    length: Option<u64>,
}

#[derive(Clone, Debug)]
struct BranchInfo {
    id: Id,
    head: Option<RawValue>,
}

#[derive(Clone, Debug)]
struct PileSnapshot {
    path: PathBuf,
    file_len: u64,
    blobs: Vec<BlobInfo>,
    branches: Vec<BranchInfo>,
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

        let ground_handle =
            bodies.insert(RigidBodyBuilder::fixed().translation(vector![0.0, 0.0]).build());
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
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
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
    snapshot.blobs.len().hash(&mut hasher);
    snapshot.branches.len().hash(&mut hasher);
    if let Some(blob) = snapshot.blobs.first() {
        blob.hash.hash(&mut hasher);
        blob.timestamp_ms.hash(&mut hasher);
        blob.length.hash(&mut hasher);
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
    let candidates: Vec<&BlobInfo> = snapshot
        .blobs
        .iter()
        .filter(|blob| blob.length.is_some())
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }

    let branch_heads: HashSet<RawValue> = snapshot
        .branches
        .iter()
        .filter_map(|branch| branch.head)
        .collect();

    let sample_count = target_sample_count(candidates.len(), levels.blob);
    let sample_rate = levels.sample_rate.clamp(0.0, 1.0);
    let max_samples = candidates.len();
    let sample_count = if sample_rate > 0.0 {
        let extra = max_samples.saturating_sub(sample_count) as f32;
        (sample_count as f32 + extra * sample_rate).round() as usize
    } else {
        sample_count
    };
    let mut branch_blobs: Vec<&BlobInfo> = candidates
        .iter()
        .copied()
        .filter(|blob| branch_heads.contains(&blob.hash))
        .collect();
    branch_blobs.sort_by_key(|blob| hash_u64(blob.hash));
    if branch_blobs.len() > sample_count {
        branch_blobs.truncate(sample_count);
    }

    let remaining = sample_count.saturating_sub(branch_blobs.len());
    let mut selected = select_sample(&candidates, remaining, &branch_heads);
    selected.extend(branch_blobs);
    if selected.is_empty() {
        return Vec::new();
    }

    selected.sort_by_key(|blob| (blob.timestamp_ms.unwrap_or(u64::MAX), blob.hash));

    let (min_log, max_log) = size_log_range(&selected);
    let (min_size, max_size) = size_range(sim_rect, levels);
    let age_weight = 0.4 + levels.age * 0.8;

    let oldest_ts = snapshot.blobs.iter().filter_map(|blob| blob.timestamp_ms).min();
    let newest_ts = snapshot.blobs.iter().filter_map(|blob| blob.timestamp_ms).max();
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
            painter.line_segment(
                [points[idx], points[(idx + 1) % 4]],
                stroke,
            );
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

fn summary_overlay_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    value_color: egui::Color32,
) {
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
                    summary_overlay_row(
                        ui,
                        "BLOBS",
                        &format!("{blob_count}"),
                        pile_color,
                    );
                    summary_overlay_row(
                        ui,
                        "BRANCHES",
                        &format!("{branch_count}"),
                        sprout_color,
                    );
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

fn load_pile(path: PathBuf) -> Result<PileSnapshot, String> {
    let mut pile: Pile = Pile::open(&path).map_err(|err| err.to_string())?;
    pile.restore().map_err(|err| err.to_string())?;

    let file_len = std::fs::metadata(&path)
        .map_err(|err| err.to_string())?
        .len();

    let reader = pile.reader().map_err(|err| err.to_string())?;

    let mut blobs = Vec::new();
    for handle in reader.blobs().map(|res| res.map_err(|err| err.to_string())) {
        let handle = handle?;
        let meta = reader
            .metadata(handle)
            .map_err(|_infallible| "metadata() failed".to_owned())?;
        let (timestamp_ms, length) = match meta {
            Some(meta) => (Some(meta.timestamp), Some(meta.length)),
            None => (None, None),
        };
        blobs.push(BlobInfo {
            hash: handle.raw,
            timestamp_ms,
            length,
        });
    }

    let mut branches = Vec::new();
    let branch_iter = pile.branches().map_err(|err| err.to_string())?;
    for id in branch_iter {
        let id = id.map_err(|err| err.to_string())?;
        let head = pile
            .head(id)
            .map_err(|err| err.to_string())?
            .map(|handle| handle.raw);
        branches.push(BranchInfo { id, head });
    }

    blobs.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));

    Ok(PileSnapshot {
        path,
        file_len,
        blobs,
        branches,
    })
}

#[derive(Debug)]
struct InspectorState {
    pile_path: String,
    max_rows: usize,
    blob_page: usize,
    histogram_bytes: bool,
    snapshot: ComputedState<Result<PileSnapshot, String>>,
    summary_sim: SummarySimState,
}

impl Default for InspectorState {
    fn default() -> Self {
        Self {
            pile_path: "./repo.pile".to_owned(),
            max_rows: 200,
            blob_page: 0,
            histogram_bytes: false,
            snapshot: ComputedState::Undefined,
            summary_sim: SummarySimState::default(),
        }
    }
}

#[notebook]
fn main() {
    let default_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./repo.pile".to_owned());
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;

    state!(
        inspector = InspectorState {
            pile_path: default_path,
            max_rows: 200,
            blob_page: 0,
            histogram_bytes: false,
            snapshot: ComputedState::Undefined,
            summary_sim: SummarySimState::default(),
        },
        move |ui, state| {
            ui.with_padding(padding, |ui| {
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

                    let pile_path = PathBuf::from(state.pile_path.trim());
                    let snapshot = widgets::load_button(
                        ui,
                        &mut state.snapshot,
                        "Open pile",
                        "Refresh pile",
                        move || load_pile(pile_path.clone()),
                    );

                    if let Some(Err(err)) = snapshot {
                        ui.label(err.as_str());
                    }
                });
            });
        }
    );

    state!(summary_tuning = SummaryTuning::default(), move |ui, tuning| {
        ui.with_padding(padding, |ui| {
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
                    egui::RichText::new("Enable TUNE to override the pile visualization.").small(),
                );
            }
        });
    });

    view!(move |ui| {
        ui.with_padding(padding, |ui| {
            let mut state = ui.read_mut(inspector).expect("inspector state missing");
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

        let Some(result) = state.snapshot.ready() else {
            md!(ui, "_Load a pile to see the distribution._");
            return;
        };
        let Ok(snapshot) = result else {
            md!(ui, "_Load a valid pile to see the distribution._");
            return;
        };

        const MIN_BUCKET_EXP: u32 = 6; // 64B (pile record alignment).
        const MAX_BUCKET_EXP: u32 = 36; // 64 GiB and above go into the last bucket.

        let mut buckets = std::collections::BTreeMap::<u32, (u64, u64)>::new(); // exp -> (count, bytes)
        let mut valid_blobs = 0u64;
        let mut total_bytes = 0u64;
        let mut saw_underflow = false;
        let mut saw_overflow = false;

        for blob in &snapshot.blobs {
            let Some(len) = blob.length else {
                continue;
            };
            valid_blobs += 1;
            total_bytes = total_bytes.saturating_add(len);

            let raw_exp = len.max(1).ilog2();
            let exp = raw_exp.clamp(MIN_BUCKET_EXP, MAX_BUCKET_EXP);
            saw_underflow |= raw_exp < MIN_BUCKET_EXP;
            saw_overflow |= raw_exp > MAX_BUCKET_EXP;
            let entry = buckets.entry(exp).or_insert((0, 0));
            entry.0 += 1;
            entry.1 = entry.1.saturating_add(len);
        }

        if buckets.is_empty() {
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
        for exp in MIN_BUCKET_EXP..=MAX_BUCKET_EXP {
            let (count, bytes) = buckets.get(&exp).copied().unwrap_or((0, 0));
            let value = if histogram_bytes { bytes } else { count };
            max_value = max_value.max(value);

            let mut bucket = widgets::HistogramBucket::new(
                value,
                bucket_label(
                    exp,
                    MIN_BUCKET_EXP,
                    MAX_BUCKET_EXP,
                    saw_underflow,
                    saw_overflow,
                ),
            );

            if value > 0 {
                let range = if saw_overflow && exp == MAX_BUCKET_EXP {
                    let start = bucket_start(exp);
                    format!("≥ {}", format_bytes(start))
                } else if saw_underflow && exp == MIN_BUCKET_EXP {
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
                valid_blobs,
                format_bytes(total_bytes)
            );
        });
    });

    view!(move |ui| {
        let summary_padding = egui::Margin::ZERO;
        ui.with_padding(summary_padding, |ui| {
            let mut state = ui.read_mut(inspector).expect("inspector state missing");
            let tuning = ui.read(summary_tuning).expect("summary tuning missing");
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
            let status_color = match &*snapshot {
                ComputedState::Undefined => label_color,
                ComputedState::Init(_) | ComputedState::Stale(_, _, _) => accent_warn,
                ComputedState::Ready(Ok(_), _) => accent_ok,
                ComputedState::Ready(Err(_), _) => accent_error,
            };

            egui::Frame::NONE
                .inner_margin(egui::Margin::same(0))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);

                    match snapshot {
                        ComputedState::Undefined => {
                            summary_status_panel(
                                ui,
                                ui.available_width(),
                                "No pile loaded yet.",
                                status_color,
                                bg_color,
                                summary_padding,
                            );
                        }
                        ComputedState::Init(_) => {
                            summary_status_panel(
                                ui,
                                ui.available_width(),
                                "Loading pile data.",
                                status_color,
                                bg_color,
                                summary_padding,
                            );
                        }
                        ComputedState::Stale(_, _, _) => {
                            summary_status_panel(
                                ui,
                                ui.available_width(),
                                "Refreshing pile data.",
                                status_color,
                                bg_color,
                                summary_padding,
                            );
                        }
                        ComputedState::Ready(Err(err), _) => {
                            summary_status_panel(
                                ui,
                                ui.available_width(),
                                &format!("{err}"),
                                status_color,
                                bg_color,
                                summary_padding,
                            );
                        }
                        ComputedState::Ready(Ok(snapshot), _) => {
                            let blob_count = snapshot.blobs.len();
                            let branch_count = snapshot.branches.len();
                            let oldest_ts =
                                snapshot.blobs.iter().filter_map(|b| b.timestamp_ms).min();
                            let newest_ts =
                                snapshot.blobs.iter().filter_map(|b| b.timestamp_ms).max();

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
                            let live_avg_blob_level = normalize_log2(avg_blob_size + 1, 6.0, 24.0);
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
                });
        });
    });

    view!(move |ui| {
        ui.with_padding(padding, |ui| {
            let state = ui.read(inspector).expect("inspector state missing");
            md!(ui, "## Branches");

        let Some(result) = state.snapshot.ready() else {
            md!(ui, "_Load a pile to see branches._");
            return;
        };
        let Ok(snapshot) = result else {
            md!(ui, "_Load a valid pile to see branches._");
            return;
        };

        if snapshot.branches.is_empty() {
            md!(ui, "_No branches found._");
            return;
        }

            egui::Grid::new("pile-branches")
                .num_columns(2)
                .striped(false)
                .show(ui, |ui| {
                    ui.strong("BRANCH");
                    ui.strong("HEAD");
                    ui.end_row();

                    for branch in &snapshot.branches {
                        ui.monospace(hex_prefix(branch.id, 6));
                        match &branch.head {
                            Some(raw) => ui.monospace(hex_prefix(raw, 6)),
                            None => ui.label("—"),
                        };
                        ui.end_row();
                    }
                });
        });
    });

    view!(move |ui| {
        ui.with_padding(padding, |ui| {
            let mut state = ui.read_mut(inspector).expect("inspector state missing");
            md!(ui, "## Blobs");

        let Some(result) = state.snapshot.ready() else {
            md!(ui, "_Load a pile to see blobs._");
            return;
        };
        let Ok(snapshot) = result else {
            md!(ui, "_Load a valid pile to see blobs._");
            return;
        };

        if snapshot.blobs.is_empty() {
            md!(ui, "_No blobs found._");
            return;
        }

        let page_size = state.max_rows.max(1);
        let mut page = state.blob_page;
        let total = snapshot.blobs.len();
        let total_pages = total.saturating_add(page_size - 1) / page_size;
        page = page.min(total_pages.saturating_sub(1));
        let mut page_next = page;
        let start = page * page_size;
        let end = (start + page_size).min(total);
        let display_start = start + 1;

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 2.0);

            ui.horizontal(|ui| {
                let page_label = if total_pages == 0 {
                    "Page —".to_owned()
                } else {
                    let page_display = page + 1;
                    format!("Page {page_display} of {total_pages}")
                };
                ui.label(egui::RichText::new(page_label).small());

                ui.add_enabled_ui(page > 0, |ui| {
                    if ui.add(widgets::Button::new("Prev").small()).clicked() {
                        page_next = page_next.saturating_sub(1);
                    }
                });
                ui.add_enabled_ui(page + 1 < total_pages, |ui| {
                    if ui.add(widgets::Button::new("Next").small()).clicked() {
                        page_next = page_next.saturating_add(1);
                    }
                });

                if total_pages > 0 {
                    ui.label(egui::RichText::new(format!("{display_start}–{end} of {total}")).small());
                }
            });

            let now_ms = now_ms();
            let branch_heads: HashSet<RawValue> = snapshot
                .branches
                .iter()
                .filter_map(|branch| branch.head)
                .collect();
            let card_spacing = egui::vec2(1.0, 1.0);
            let card_height = ui.text_style_height(&egui::TextStyle::Small) + 6.0;
            let min_card_width = 56.0;
            let card_fill = ui.visuals().widgets.noninteractive.bg_fill;
            let outline_stroke = egui::Stroke::new(
                1.0,
                ui.visuals().widgets.noninteractive.bg_stroke.color,
            );
            let card_rounding = egui::CornerRadius::same(4);
            let branch_color = egui::Color32::from_rgb(95, 210, 85);
            let items: Vec<&BlobInfo> = snapshot
                .blobs
                .iter()
                .skip(start)
                .take(page_size)
                .collect();
            let available_width = ui.available_width();
            let columns = ((available_width + card_spacing.x) / (min_card_width + card_spacing.x))
                .floor()
                .max(1.0) as usize;
            let columns_f = columns as f32;
            let card_width =
                ((available_width - card_spacing.x * (columns_f - 1.0)) / columns_f)
                    .floor()
                    .max(1.0);
            let card_size = egui::vec2(card_width, card_height);

            ui.add_space(2.0);
            ui.spacing_mut().item_spacing = card_spacing;

            let render_blob_card = |ui: &mut egui::Ui, blob: &BlobInfo| {
                let is_head = branch_heads.contains(&blob.hash);
                let (rect, response) = ui.allocate_exact_size(card_size, egui::Sense::click());
                ui.painter()
                    .rect_filled(rect, card_rounding, card_fill);

                if is_head {
                    let band_inset = outline_stroke.width;
                    let band_rect = rect.shrink(band_inset);
                    let inset_u8 =
                        band_inset.round().clamp(0.0, u8::MAX as f32) as u8;
                    let band_rounding = card_rounding - inset_u8;
                    let line_height = f32::from(band_rounding.nw)
                        .min(band_rect.height())
                        .max(1.0);
                    let line_rect = egui::Rect::from_min_max(
                        band_rect.min,
                        egui::pos2(band_rect.max.x, band_rect.min.y + line_height),
                    );
                    ui.painter()
                        .with_clip_rect(line_rect)
                        .rect_filled(band_rect, band_rounding, branch_color);
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
                    ui.label(
                        egui::RichText::new(format!("hash: {hash_text}"))
                            .monospace(),
                    );
                });
                if response.clicked() {
                    ui.ctx().copy_text(hash_text);
                }
            };

            for row in items.chunks(columns) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = card_spacing;
                    for blob in row {
                        render_blob_card(ui, blob);
                    }
                });
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
