use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use parking_lot::Mutex;
use rapier2d::prelude::*;
use triblespace::core::blob::schemas::simplearchive::SimpleArchive;
use triblespace::core::repo::BlobStoreMeta;
use triblespace::core::value::schemas::hash::{Blake3, Handle};
use triblespace::core::value::{RawValue, Value};

const SUMMARY_PANEL_PADDING: f32 = 6.0;
const SUMMARY_SIM_ASPECT_RATIO: f32 = 2.0;

/// Input data needed to render a pile overview panel.
pub struct PileOverviewData<'a, R> {
    pub path: &'a std::path::Path,
    pub file_len: u64,
    pub blob_order: &'a [RawValue],
    pub branch_heads: &'a [RawValue],
    pub branch_count: usize,
    pub oldest_ts: Option<u64>,
    pub newest_ts: Option<u64>,
    pub reader: &'a R,
}

/// Tuning overrides for pile overview rendering.
#[derive(Clone, Debug)]
pub struct PileOverviewTuning {
    pub enabled: bool,
    pub size_level: f32,
    pub blob_level: f32,
    pub avg_blob_level: f32,
    pub age_level: f32,
    pub branch_level: f32,
    pub zoom: f32,
    pub sample_rate: f32,
    pub insert_rate: f32,
}

impl Default for PileOverviewTuning {
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

/// Colors used by the pile overview panel.
#[derive(Clone, Copy, Debug)]
pub struct PileOverviewPalette {
    pub background: egui::Color32,
    pub label: egui::Color32,
    pub pile: egui::Color32,
    pub web: egui::Color32,
    pub sprout: egui::Color32,
    pub accent_ok: egui::Color32,
    pub accent_warn: egui::Color32,
    pub accent_error: egui::Color32,
}

impl Default for PileOverviewPalette {
    fn default() -> Self {
        let sprout = egui::Color32::from_rgb(95, 210, 85);
        Self {
            background: egui::Color32::from_rgb(8, 8, 8),
            label: egui::Color32::from_rgb(245, 140, 45),
            pile: egui::Color32::from_rgb(60, 170, 230),
            web: egui::Color32::from_rgb(235, 235, 235),
            sprout,
            accent_ok: sprout,
            accent_warn: egui::Color32::from_rgb(255, 196, 0),
            accent_error: egui::Color32::from_rgb(255, 80, 90),
        }
    }
}

/// High-level state for the overview panel.
pub enum PileOverviewState<'a, R> {
    Loading { message: &'a str },
    Empty { message: &'a str },
    Error { message: &'a str },
    Ready { data: PileOverviewData<'a, R> },
}

pub struct PileOverviewResponse {
    pub response: egui::Response,
    pub active: bool,
}

#[must_use = "Use `PileOverviewWidget::show(ui)` to render this widget."]
pub struct PileOverviewWidget<'a, R>
where
    R: BlobStoreMeta<Blake3>,
{
    state: PileOverviewState<'a, R>,
    tuning: Option<&'a PileOverviewTuning>,
    palette: PileOverviewPalette,
    padding: egui::Margin,
    cache_id: Option<egui::Id>,
}

impl<'a, R> PileOverviewWidget<'a, R>
where
    R: BlobStoreMeta<Blake3>,
{
    pub fn new(state: PileOverviewState<'a, R>) -> Self {
        Self {
            state,
            tuning: None,
            palette: PileOverviewPalette::default(),
            padding: egui::Margin::ZERO,
            cache_id: None,
        }
    }

    pub fn tuning(mut self, tuning: &'a PileOverviewTuning) -> Self {
        self.tuning = Some(tuning);
        self
    }

    pub fn palette(mut self, palette: PileOverviewPalette) -> Self {
        self.palette = palette;
        self
    }

    pub fn padding(mut self, padding: egui::Margin) -> Self {
        self.padding = padding;
        self
    }

    pub fn cache_id(mut self, cache_id: egui::Id) -> Self {
        self.cache_id = Some(cache_id);
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> PileOverviewResponse {
        let palette = self.palette;
        let padding = self.padding;
        let mut active = false;

        let response = match self.state {
            PileOverviewState::Loading { message } => summary_status_panel(
                ui,
                ui.available_width(),
                message,
                palette.accent_warn,
                palette.background,
                padding,
            ),
            PileOverviewState::Empty { message } => summary_status_panel(
                ui,
                ui.available_width(),
                message,
                palette.label,
                palette.background,
                padding,
            ),
            PileOverviewState::Error { message } => summary_status_panel(
                ui,
                ui.available_width(),
                message,
                palette.accent_error,
                palette.background,
                padding,
            ),
            PileOverviewState::Ready { data } => {
                let fallback_tuning = PileOverviewTuning::default();
                let tuning = self.tuning.unwrap_or(&fallback_tuning);

                let now_ms = now_ms();
                let blob_count = data.blob_order.len();

                let oldest = data
                    .oldest_ts
                    .map(|ts| format_age(now_ms, ts))
                    .unwrap_or_else(|| "n/a".to_owned());
                let newest = data
                    .newest_ts
                    .map(|ts| format_age(now_ms, ts))
                    .unwrap_or_else(|| "n/a".to_owned());

                let age_span_secs = match (data.oldest_ts, data.newest_ts) {
                    (Some(oldest_ts), Some(newest_ts)) => {
                        newest_ts.saturating_sub(oldest_ts) / 1000
                    }
                    _ => 0,
                };

                let live_size_level = normalize_log2(data.file_len, 10.0, 30.0);
                let live_blob_level = normalize_log2(blob_count as u64 + 1, 0.0, 20.0);
                let live_branch_level = normalize_log2(data.branch_count as u64 + 1, 0.0, 12.0);
                let live_age_level = normalize_log2(age_span_secs + 1, 0.0, 20.0);
                let avg_blob_size = if blob_count > 0 {
                    data.file_len / blob_count as u64
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

                let base =
                    summary_panel_base(ui, ui.available_width(), palette.background, padding);
                let sim_rect = summary_sim_rect(base.rect);
                let spawn_interval = spawn_interval_for_rate(levels.insert_rate);
                let cache_id = self
                    .cache_id
                    .unwrap_or_else(|| ui.id().with("pile_overview_sim"));
                let sim_handle = ui.ctx().data_mut(|data| {
                    data.get_temp_mut_or_default::<SummarySimHandle>(cache_id)
                        .clone()
                });
                let mut sim_state = sim_handle.state.lock();
                active = sim_state.update_and_draw(
                    ui,
                    &data,
                    levels,
                    sim_rect,
                    spawn_interval,
                    palette.pile,
                    palette.web,
                    palette.sprout,
                );
                if active {
                    ui.ctx().request_repaint();
                }
                summary_overlay_text(
                    ui,
                    base.rect,
                    &format_bytes(data.file_len),
                    blob_count,
                    data.branch_count,
                    &oldest,
                    &newest,
                    &data.path.display().to_string(),
                    palette.label,
                    palette.pile,
                    palette.web,
                    palette.sprout,
                );
                extend_panel_background(ui, base.rect, palette.background, padding);
                base.response
            }
        };

        PileOverviewResponse { response, active }
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
    fn new<R>(data: &PileOverviewData<'_, R>, levels: SummaryLevels, rect: egui::Rect) -> Self {
        let width = rect.width().round().max(1.0) as u32;
        let height = rect.height().round().max(1.0) as u32;
        Self {
            data_hash: overview_hash(data),
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
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    gravity: Vector,
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
                .translation(Vector::new(0.0, 0.0))
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
            broad_phase: BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            gravity: Vector::new(0.0, -980.0),
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
            .translation(Vector::new(x, y))
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
            self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
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
                max_speed = max_speed.max(body.linvel().length());
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

#[derive(Clone)]
struct SummarySimHandle {
    state: Arc<Mutex<SummarySimState>>,
}

impl Default for SummarySimHandle {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(SummarySimState::default())),
        }
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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

struct PanelBase {
    rect: egui::Rect,
    response: egui::Response,
}

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
) -> PanelBase {
    let height = summary_panel_height(width);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let full_rect = expand_card_rect(rect, padding);
    ui.painter()
        .with_clip_rect(full_rect)
        .rect_filled(full_rect, 0.0, bg_color);

    PanelBase { rect, response }
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

fn overview_hash<R>(data: &PileOverviewData<'_, R>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.path.hash(&mut hasher);
    data.file_len.hash(&mut hasher);
    data.blob_order.len().hash(&mut hasher);
    data.branch_count.hash(&mut hasher);
    data.oldest_ts.hash(&mut hasher);
    data.newest_ts.hash(&mut hasher);
    if let Some(blob) = data.blob_order.first() {
        blob.hash(&mut hasher);
    }
    if let Some(blob) = data.blob_order.last() {
        blob.hash(&mut hasher);
    }
    if let Some(head) = data.branch_heads.first() {
        head.hash(&mut hasher);
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

#[derive(Clone, Debug)]
struct BlobInfo {
    hash: RawValue,
    timestamp_ms: Option<u64>,
    length: Option<u64>,
}

fn blob_info(reader: &impl BlobStoreMeta<Blake3>, hash: RawValue) -> BlobInfo {
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

fn build_summary_blocks<R>(
    data: &PileOverviewData<'_, R>,
    levels: SummaryLevels,
    sim_rect: egui::Rect,
) -> Vec<PileBlock>
where
    R: BlobStoreMeta<Blake3>,
{
    let candidates: Vec<BlobInfo> = data
        .blob_order
        .iter()
        .map(|&hash| blob_info(data.reader, hash))
        .filter(|blob| blob.length.is_some())
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }

    let candidate_refs: Vec<&BlobInfo> = candidates.iter().collect();

    let branch_heads: HashSet<RawValue> = data.branch_heads.iter().copied().collect();

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

    let oldest_ts = data.oldest_ts;
    let newest_ts = data.newest_ts;
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
    fn update_and_draw<R>(
        &mut self,
        ui: &mut egui::Ui,
        data: &PileOverviewData<'_, R>,
        levels: SummaryLevels,
        sim_rect: egui::Rect,
        spawn_interval: f32,
        pile_color: egui::Color32,
        web_color: egui::Color32,
        sprout_color: egui::Color32,
    ) -> bool
    where
        R: BlobStoreMeta<Blake3>,
    {
        let fingerprint = SummarySimFingerprint::new(data, levels, sim_rect);
        if self.fingerprint != Some(fingerprint) {
            let blocks = build_summary_blocks(data, levels, sim_rect);
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
) -> egui::Response {
    let base = summary_panel_base(ui, width, bg_color, padding);
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(base.rect.shrink(8.0))
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 2.0);
            ui.label(egui::RichText::new(message).monospace().color(accent));
        },
    );
    base.response
}
