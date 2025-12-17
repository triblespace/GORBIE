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
use GORBIE::Notebook;

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

#[derive(Clone, Copy)]
struct CountScale {
    divisor: u64,
    suffix: &'static str,
}

impl CountScale {
    fn pick(max: u64) -> Self {
        if max >= 1_000_000_000 {
            Self {
                divisor: 1_000_000_000,
                suffix: "B",
            }
        } else if max >= 1_000_000 {
            Self {
                divisor: 1_000_000,
                suffix: "M",
            }
        } else if max >= 1_000 {
            Self {
                divisor: 1_000,
                suffix: "K",
            }
        } else {
            Self {
                divisor: 1,
                suffix: "",
            }
        }
    }

    fn format(self, value: u64) -> String {
        if value == 0 {
            return "0".to_owned();
        }

        if self.divisor == 1 {
            return format!("{value}");
        }

        let scaled = value as f64 / self.divisor as f64;
        if (scaled.fract() - 0.0).abs() < f64::EPSILON {
            format!("{}{suffix}", scaled as u64, suffix = self.suffix)
        } else {
            format!("{scaled:.1}{suffix}", suffix = self.suffix)
        }
    }
}

#[derive(Clone, Copy)]
struct BytesScale {
    divisor: u64,
    suffix: &'static str,
}

impl BytesScale {
    fn pick(step: u64) -> Self {
        if step >= (1u64 << 30) {
            Self {
                divisor: 1u64 << 30,
                suffix: "GiB",
            }
        } else if step >= (1u64 << 20) {
            Self {
                divisor: 1u64 << 20,
                suffix: "MiB",
            }
        } else if step >= (1u64 << 10) {
            Self {
                divisor: 1u64 << 10,
                suffix: "KiB",
            }
        } else {
            Self {
                divisor: 1,
                suffix: "B",
            }
        }
    }

    fn format(self, value: u64) -> String {
        if self.divisor == 1 {
            return format!("{value} B");
        }

        let scaled = value / self.divisor;
        format!("{scaled} {suffix}", suffix = self.suffix)
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

fn pile_inspector(nb: &mut Notebook) {
    let default_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./repo.pile".to_owned());

    let inspector = state!(
        nb,
        (),
        InspectorState {
            pile_path: default_path,
            max_rows: 200,
            histogram_bytes: false,
            snapshot: ComputedState::Undefined,
        },
        |ui, state| {
            md!(
                ui,
                "# Triblespace pile inspector\n\nOpen a `.pile` file on disk and inspect its blob and branch indices.\n\nTip: pass a path as the first CLI arg to prefill this field."
            );

            ui.horizontal(|ui| {
                ui.label("Pile path:");
                ui.text_edit_singleline(&mut state.pile_path);
                ui.label("Rows:");
                ui.add(
                    egui::DragValue::new(&mut state.max_rows)
                        .range(10..=10_000)
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

                if let Some(snapshot) = snapshot {
                    if let Err(err) = snapshot {
                        ui.label(err.as_str());
                    }
                }
            });
        }
    );

    view!(nb, (inspector), move |ui| {
        let mut state = inspector.write();

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
            ui.add(widgets::ChoiceToggle::new(
                &mut state.histogram_bytes,
                "COUNT",
                "BYTES",
            ));
        });

        let first_exp = MIN_BUCKET_EXP;
        let last_exp = MAX_BUCKET_EXP;

        let mut max_value = 0u64;
        for (_exp, (count, bytes)) in &buckets {
            let value = if state.histogram_bytes {
                *bytes
            } else {
                *count
            };
            max_value = max_value.max(value);
        }
        if max_value == 0 {
            md!(ui, "_No data to plot._");
            return;
        }

        let desired_width = ui.available_width().max(128.0);
        let font_id = egui::TextStyle::Small.resolve(ui.style());
        let tick_len = 4.0;
        let tick_pad = 2.0;
        let text_height = ui.fonts(|fonts| fonts.row_height(&font_id));
        let label_row_h = tick_len + tick_pad + text_height;
        let plot_h = 80.0;
        let total_h = plot_h + label_row_h;

        let (outer_rect, _resp) =
            ui.allocate_exact_size(egui::vec2(desired_width, total_h), egui::Sense::hover());

        fn paint_hatching(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
            let spacing = 8.0;
            let stroke = egui::Stroke::new(1.0, color);

            let h = rect.height();
            let mut x = rect.left() - h;
            while x < rect.right() + h {
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x + h, rect.bottom())],
                    stroke,
                );
                x += spacing;
            }
        }

        let visuals = ui.visuals();
        let background = visuals.window_fill;
        let outline = visuals.widgets.noninteractive.bg_stroke.color;
        let stroke = egui::Stroke::new(1.0, outline);
        let ink = visuals.widgets.noninteractive.fg_stroke.color;
        let grid_color = GORBIE::themes::blend(background, ink, 0.22);
        let y_label_color = ink;

        fn nice_decimal_step(max_value: u64, segments: u64) -> u64 {
            let segments = segments.max(1);
            let raw_step = max_value.div_ceil(segments).max(1);
            let magnitude = 10u64.pow(raw_step.ilog10());
            for mult in [1u64, 2, 5, 10] {
                let step = mult.saturating_mul(magnitude);
                if step >= raw_step {
                    return step;
                }
            }
            10u64.saturating_mul(magnitude)
        }

        let y_segments = 4u64;
        let y_step = if state.histogram_bytes {
            max_value.div_ceil(y_segments).max(1).next_power_of_two()
        } else {
            nice_decimal_step(max_value, y_segments)
        };
        let y_max = y_step.saturating_mul(y_segments).max(1);
        let y_ticks: Vec<u64> = (0..=y_segments).map(|i| y_step.saturating_mul(i)).collect();

        let bytes_scale = if state.histogram_bytes {
            Some(BytesScale::pick(y_step))
        } else {
            None
        };
        let count_scale = if state.histogram_bytes {
            None
        } else {
            Some(CountScale::pick(y_max))
        };

        let y_label_width = ui.fonts(|fonts| {
            y_ticks
                .iter()
                .map(|&value| {
                    let text = match (bytes_scale, count_scale) {
                        (Some(scale), _) => scale.format(value),
                        (_, Some(scale)) => scale.format(value),
                        _ => unreachable!(),
                    };
                    fonts
                        .layout_no_wrap(text, font_id.clone(), y_label_color)
                        .size()
                        .x
                })
                .fold(0.0, f32::max)
        });
        let y_axis_w = (y_label_width + 10.0).clamp(24.0, 80.0);
        let y_axis_pad = 6.0;

        let plot_rect = egui::Rect::from_min_max(
            egui::pos2(
                (outer_rect.left() + y_axis_w + y_axis_pad).min(outer_rect.right()),
                outer_rect.top(),
            ),
            egui::pos2(outer_rect.right(), outer_rect.bottom() - label_row_h),
        );
        let plot_area = plot_rect.shrink(4.0);

        let painter = ui.painter().with_clip_rect(outer_rect);
        painter.rect_stroke(plot_rect, 0.0, stroke, egui::StrokeKind::Inside);

        for value in &y_ticks {
            let frac = (*value as f64 / y_max as f64) as f32;
            let y = plot_area.bottom() - frac * plot_area.height();
            painter.line_segment(
                [
                    egui::pos2(plot_area.left(), y),
                    egui::pos2(plot_area.right(), y),
                ],
                egui::Stroke::new(1.0, grid_color),
            );

            let text = match (bytes_scale, count_scale) {
                (Some(scale), _) => scale.format(*value),
                (_, Some(scale)) => scale.format(*value),
                _ => unreachable!(),
            };
            painter.text(
                egui::pos2(plot_rect.left() - 4.0, y),
                egui::Align2::RIGHT_CENTER,
                text,
                font_id.clone(),
                y_label_color,
            );
        }

        let bucket_count = (last_exp - first_exp + 1) as usize;
        let gap = 2.0;
        let bar_w = ((plot_area.width() - gap * (bucket_count.saturating_sub(1) as f32))
            / bucket_count as f32)
            .max(1.0);

        for i in 0..bucket_count {
            let exp = first_exp + i as u32;
            let (count, bytes) = buckets.get(&exp).copied().unwrap_or((0, 0));
            let value = if state.histogram_bytes { bytes } else { count };
            if value == 0 {
                continue;
            }

            let frac = (value as f64 / y_max as f64) as f32;
            let bar_h = (frac * plot_area.height()).clamp(1.0, plot_area.height());

            let x0 = plot_area.left() + i as f32 * (bar_w + gap);
            let x1 = (x0 + bar_w).min(plot_area.right());
            let bar_rect = egui::Rect::from_min_max(
                egui::pos2(x0, plot_area.bottom() - bar_h),
                egui::pos2(x1, plot_area.bottom()),
            );

            let id = ui.make_persistent_id(("pile_hist_bar", exp));
            let resp = ui.interact(bar_rect, id, egui::Sense::hover());
            let stroke_color = if resp.hovered() {
                ui.visuals().selection.stroke.color
            } else {
                outline
            };
            let bar_stroke = egui::Stroke::new(1.0, stroke_color);

            let hatch_rect = bar_rect.shrink(1.0);
            if hatch_rect.is_positive() {
                paint_hatching(&painter.with_clip_rect(hatch_rect), hatch_rect, ink);
            }
            painter.rect_stroke(bar_rect, 0.0, bar_stroke, egui::StrokeKind::Inside);

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
            let _ = resp.on_hover_text(format!("{range}\n{metric}"));
        }

        let max_labels = 7usize;
        let step = (bucket_count.div_ceil(max_labels)).max(1);

        for i in (0..bucket_count).step_by(step) {
            let exp = first_exp + i as u32;
            let x = plot_area.left() + i as f32 * (bar_w + gap) + bar_w * 0.5;
            let tick_top = plot_rect.bottom();
            painter.line_segment(
                [egui::pos2(x, tick_top), egui::pos2(x, tick_top + tick_len)],
                egui::Stroke::new(1.0, outline),
            );
            painter.text(
                egui::pos2(x, tick_top + tick_len + tick_pad),
                egui::Align2::CENTER_TOP,
                bucket_label(
                    exp,
                    MIN_BUCKET_EXP,
                    MAX_BUCKET_EXP,
                    saw_underflow,
                    saw_overflow,
                ),
                font_id.clone(),
                ink,
            );
        }

        md!(
            ui,
            "_{} blobs, {} total._",
            valid_blobs,
            format_bytes(total_bytes)
        );
    });

    view!(nb, (inspector), move |ui| {
        let state = inspector.read();

        md!(ui, "## Summary");

        let now_ms = now_ms();
        match &state.snapshot {
            ComputedState::Undefined => {
                md!(ui, "_No pile loaded yet._");
            }
            ComputedState::Init(_) => {
                md!(ui, "_Loading…_");
            }
            ComputedState::Stale(_, _, _) => {
                md!(ui, "_Refreshing…_");
            }
            ComputedState::Ready(result, _) => match result {
                Ok(snapshot) => {
                    let blob_count = snapshot.blobs.len();
                    let branch_count = snapshot.branches.len();
                    let oldest = snapshot
                        .blobs
                        .iter()
                        .filter_map(|b| b.timestamp_ms)
                        .min()
                        .map(|ts| format_age(now_ms, ts));
                    let newest = snapshot
                        .blobs
                        .iter()
                        .filter_map(|b| b.timestamp_ms)
                        .max()
                        .map(|ts| format_age(now_ms, ts));

                    let oldest = oldest.unwrap_or_else(|| "—".to_owned());
                    let newest = newest.unwrap_or_else(|| "—".to_owned());

                    md!(
                        ui,
                        "- Path: `{}`\n- Size: `{}`\n- Blobs: `{}`\n- Branches: `{}`\n- Oldest: `{}`\n- Newest: `{}`",
                        snapshot.path.display(),
                        format_bytes(snapshot.file_len),
                        blob_count,
                        branch_count,
                        oldest,
                        newest
                    );
                }
                Err(err) => {
                    md!(ui, "Error: `{err}`");
                }
            },
        }
    });

    view!(nb, (inspector), move |ui| {
        let state = inspector.read();

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
                    ui.monospace(hex_prefix(&branch.id, 6));
                    match &branch.head {
                        Some(raw) => ui.monospace(hex_prefix(raw, 6)),
                        None => ui.label("—"),
                    };
                    ui.end_row();
                }
            });
    });

    view!(nb, (inspector), move |ui| {
        let state = inspector.read();

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
                    ui.monospace(hex_prefix(&blob.hash, 6));
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
}

fn main() {
    notebook!(pile_inspector);
}
