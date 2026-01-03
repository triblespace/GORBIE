#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! triblespace = { path = "../../triblespace-rs" }
//! ```

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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
        }
    }
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

fn normalize_log2(value: u64, min_exp: f32, max_exp: f32) -> f32 {
    let value = (value.max(1) as f32).log2();
    ((value - min_exp) / (max_exp - min_exp)).clamp(0.0, 1.0)
}

fn quantize_level(level: f32, steps: u32) -> f32 {
    let steps = steps.max(2) as f32;
    (level.clamp(0.0, 1.0) * (steps - 1.0)).round() / (steps - 1.0)
}

fn expand_card_rect(rect: egui::Rect, padding: egui::Margin) -> egui::Rect {
    egui::Rect::from_min_max(
        rect.min - padding.left_top(),
        rect.max + padding.right_bottom(),
    )
}

fn summary_panel_base(
    ui: &mut egui::Ui,
    width: f32,
    bg_color: egui::Color32,
    padding: egui::Margin,
) -> egui::Rect {
    let height = 150.0;
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
    direction: egui::Vec2,
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
            let dir = egui::vec2(angle.cos() * direction.x, angle.sin() * direction.y);
            points.push(corner + dir * radius);
        }
        painter.add(egui::Shape::line(points, stroke));
    }

    let spokes = 4;
    for idx in 0..spokes {
        let t = idx as f32 / (spokes - 1) as f32;
        let angle = t * angle_span;
        let dir = egui::vec2(angle.cos() * direction.x, angle.sin() * direction.y);
        painter.line_segment([corner, corner + dir * size], stroke);
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

fn summary_pile_panel(
    ui: &mut egui::Ui,
    width: f32,
    size_level: f32,
    blob_level: f32,
    avg_blob_level: f32,
    age_level: f32,
    branch_level: f32,
    bg_color: egui::Color32,
    label_color: egui::Color32,
    pile_color: egui::Color32,
    web_color: egui::Color32,
    sprout_color: egui::Color32,
    padding: egui::Margin,
) -> egui::Rect {
    let size_level = quantize_level(size_level, 5);
    let blob_level = quantize_level(blob_level, 5);
    let avg_blob_level = quantize_level(avg_blob_level, 4);
    let age_level = quantize_level(age_level, 4);
    let branch_level = quantize_level(branch_level, 4);

    let rect = summary_panel_base(ui, width, bg_color, padding);
    let painter = ui.painter();
    let stroke = egui::Stroke::new(1.0, pile_color);

    let inner = rect.shrink(6.0);
    let ground_y = rect.bottom() - 1.0;
    painter.hline(
        egui::Rangef::new(inner.left(), inner.right()),
        ground_y,
        egui::Stroke::new(1.0, label_color),
    );

    let pile_width = inner.width() * (0.45 + size_level * 0.45);
    let pile_height = inner.height() * (0.4 + size_level * 0.45);
    let pile_rect = egui::Rect::from_min_max(
        egui::pos2(inner.right() - pile_width, ground_y - pile_height),
        egui::pos2(inner.right(), ground_y),
    );

    let rows = (2.0 + size_level * 4.0).round().clamp(2.0, 6.0) as usize;
    let base_boxes = (3.0 + blob_level * 7.0).round().clamp(3.0, 10.0) as usize;
    let max_box_width = pile_rect.width() / base_boxes as f32;
    let max_box_height = pile_rect.height() / rows as f32;
    let base_box = max_box_width.min(max_box_height);
    let gap = (base_box * 0.15).clamp(1.0, 3.0);
    let scale = 0.9 + avg_blob_level * 0.25;
    let mut box_size = (base_box - gap).max(2.0) * scale;
    box_size = box_size.min(base_box);

    for row in 0..rows {
        let row_boxes = base_boxes.saturating_sub(row).max(1);
        let row_width = row_boxes as f32 * box_size + (row_boxes.saturating_sub(1)) as f32 * gap;
        let row_left = pile_rect.center().x - row_width / 2.0;
        let row_y = pile_rect.bottom() - box_size * (row + 1) as f32;
        for col in 0..row_boxes {
            let x = row_left + col as f32 * (box_size + gap);
            let y = row_y;
            let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(box_size, box_size));
            painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
        }
    }

    let web_count = (age_level * 2.0).round() as usize;
    let web_size = 14.0 + age_level * 16.0;
    if web_count >= 1 {
        let corner = egui::pos2(rect.right() - 2.0, rect.top() + 2.0);
        draw_cobweb(painter, corner, web_size, egui::vec2(-1.0, 1.0), web_color);
    }
    if web_count >= 2 {
        let corner = egui::pos2(rect.left() + 2.0, rect.top() + 2.0);
        draw_cobweb(
            painter,
            corner,
            web_size * 0.9,
            egui::vec2(1.0, 1.0),
            web_color,
        );
    }

    let sprout_count = (branch_level * 3.0).round() as usize;
    let sprout_height = 6.0 + branch_level * 10.0;
    for idx in 0..sprout_count {
        let t = (idx + 1) as f32 / (sprout_count + 1) as f32;
        let base = egui::pos2(
            pile_rect.left() + pile_rect.width() * t,
            pile_rect.bottom() - 1.0,
        );
        let height = sprout_height * (0.9 + idx as f32 * 0.06);
        draw_sprout(painter, base, height, sprout_color);
    }

    rect
}

fn summary_overlay_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    label_color: egui::Color32,
    value_color: egui::Color32,
) {
    ui.label(egui::RichText::new(label).monospace().color(label_color));
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
                    summary_overlay_row(ui, "SIZE", size_value, label_color, pile_color);
                    summary_overlay_row(
                        ui,
                        "BLOBS",
                        &format!("{blob_count}"),
                        label_color,
                        pile_color,
                    );
                    summary_overlay_row(
                        ui,
                        "BRANCHES",
                        &format!("{branch_count}"),
                        label_color,
                        sprout_color,
                    );
                    summary_overlay_row(ui, "OLDEST", oldest, label_color, web_color);
                    summary_overlay_row(ui, "NEWEST", newest, label_color, web_color);
                });

            ui.add_space(1.0);
            ui.label(egui::RichText::new("PATH").monospace().color(pile_color));
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
    histogram_bytes: bool,
    snapshot: ComputedState<Result<PileSnapshot, String>>,
}

impl Default for InspectorState {
    fn default() -> Self {
        Self {
            pile_path: "./repo.pile".to_owned(),
            max_rows: 200,
            histogram_bytes: false,
            snapshot: ComputedState::Undefined,
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
            histogram_bytes: false,
            snapshot: ComputedState::Undefined,
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
                    ui.label("Rows:");
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

        ui.horizontal(|ui| {
            ui.label("METRIC:");
            ui.add(widgets::ChoiceToggle::binary(
                &mut state.histogram_bytes,
                "COUNT",
                "BYTES",
            ));
        });
        let y_axis = if state.histogram_bytes {
            widgets::HistogramYAxis::Bytes
        } else {
            widgets::HistogramYAxis::Count
        };

        let mut max_value = 0u64;
        let mut histogram_buckets: Vec<widgets::HistogramBucket<'static>> = Vec::new();
        for exp in MIN_BUCKET_EXP..=MAX_BUCKET_EXP {
            let (count, bytes) = buckets.get(&exp).copied().unwrap_or((0, 0));
            let value = if state.histogram_bytes { bytes } else { count };
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
                let metric = if state.histogram_bytes {
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
            let state = ui.read(inspector).expect("inspector state missing");
            let tuning = ui.read(summary_tuning).expect("summary tuning missing");
            let now_ms = now_ms();
            let bg_color = egui::Color32::from_rgb(8, 8, 8);
            let label_color = egui::Color32::from_rgb(200, 200, 200);
            let pile_color = egui::Color32::from_rgb(255, 140, 0);
            let web_color = egui::Color32::from_rgb(235, 235, 235);
            let sprout_color = egui::Color32::from_rgb(0, 220, 120);
            let accent_ok = sprout_color;
            let accent_warn = egui::Color32::from_rgb(255, 196, 0);
            let accent_error = egui::Color32::from_rgb(255, 80, 90);

            let status_color = match &state.snapshot {
                ComputedState::Undefined => label_color,
                ComputedState::Init(_) | ComputedState::Stale(_, _, _) => accent_warn,
                ComputedState::Ready(Ok(_), _) => accent_ok,
                ComputedState::Ready(Err(_), _) => accent_error,
            };

            egui::Frame::NONE
                .inner_margin(egui::Margin::same(0))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);

                    match &state.snapshot {
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
                            let size_level = if tuning.enabled {
                                tuning.size_level
                            } else {
                                live_size_level
                            };
                            let blob_level = if tuning.enabled {
                                tuning.blob_level
                            } else {
                                live_blob_level
                            };
                            let branch_level = if tuning.enabled {
                                tuning.branch_level
                            } else {
                                live_branch_level
                            };
                            let age_level = if tuning.enabled {
                                tuning.age_level
                            } else {
                                live_age_level
                            };
                            let avg_blob_level = if tuning.enabled {
                                tuning.avg_blob_level
                            } else {
                                live_avg_blob_level
                            };

                            let total_width = ui.available_width();
                            let panel_rect = summary_pile_panel(
                                ui,
                                total_width,
                                size_level,
                                blob_level,
                                avg_blob_level,
                                age_level,
                                branch_level,
                                bg_color,
                                label_color,
                                pile_color,
                                web_color,
                                sprout_color,
                                summary_padding,
                            );
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
            let state = ui.read(inspector).expect("inspector state missing");
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

        let max_rows = state.max_rows.max(1);
        md!(ui, "_Showing up to {max_rows} blobs (most recent first)._");

        let now_ms = now_ms();

            egui::Grid::new("pile-blobs")
                .num_columns(3)
                .striped(false)
                .show(ui, |ui| {
                    ui.strong("BLOB");
                    ui.strong("BYTES");
                    ui.strong("TIME");
                    ui.end_row();

                    for blob in snapshot.blobs.iter().take(max_rows) {
                        ui.monospace(hex_prefix(blob.hash, 6));
                        match blob.length {
                            Some(len) => ui.monospace(format_bytes(len)),
                            None => ui.label("invalid"),
                        };
                        match blob.timestamp_ms {
                            Some(ts) => ui.label(format_age(now_ms, ts)),
                            None => ui.label("—"),
                        };
                        ui.end_row();
                    }
                });
        });
    });
}
