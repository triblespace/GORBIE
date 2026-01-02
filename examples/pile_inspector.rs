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

fn summary_header(ui: &mut egui::Ui, title: &str, status: &str, status_color: egui::Color32) {
    let height = 22.0;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();

    let fill = GORBIE::themes::blend(
        ui.visuals().window_fill,
        ui.visuals().widgets.noninteractive.bg_stroke.color,
        0.2,
    );
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;

    painter.rect_filled(rect, 0.0, fill);
    painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);

    let stripe_color = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let stripe_x = rect.x_range();
    let stripe_top = rect.top() + 4.0;
    let stripe_spacing = 4.0;
    for idx in 0..3 {
        painter.hline(
            stripe_x,
            stripe_top + idx as f32 * stripe_spacing,
            egui::Stroke::new(1.0, stripe_color),
        );
    }

    painter.text(
        rect.left_center() + egui::vec2(6.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::monospace(10.0),
        ui.visuals().text_color(),
    );
    painter.text(
        rect.right_center() - egui::vec2(6.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        status,
        egui::FontId::monospace(10.0),
        status_color,
    );
}

fn summary_tile(
    ui: &mut egui::Ui,
    label: &str,
    value: impl std::fmt::Display,
    accent: egui::Color32,
    width: f32,
) {
    let height = 64.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();
    let fill = GORBIE::themes::blend(ui.visuals().window_fill, accent, 0.08);
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;

    painter.rect_filled(rect, 0.0, fill);
    painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);

    let accent_rect = egui::Rect::from_min_max(
        rect.left_top(),
        egui::pos2(rect.right(), rect.top() + 4.0),
    );
    painter.rect_filled(accent_rect, 0.0, accent);

    painter.text(
        rect.left_top() + egui::vec2(6.0, 8.0),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::monospace(9.0),
        accent,
    );
    painter.text(
        rect.left_bottom() + egui::vec2(6.0, -6.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{value}"),
        egui::FontId::monospace(20.0),
        ui.visuals().text_color(),
    );
}

fn summary_meta(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    accent: egui::Color32,
    width: f32,
) {
    let height = 36.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();
    let fill = GORBIE::themes::blend(ui.visuals().window_fill, accent, 0.04);
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;

    painter.rect_filled(rect, 0.0, fill);
    painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);

    let bar_rect = egui::Rect::from_min_max(
        rect.left_top(),
        egui::pos2(rect.left() + 4.0, rect.bottom()),
    );
    painter.rect_filled(bar_rect, 0.0, accent);

    painter.text(
        rect.left_top() + egui::vec2(10.0, 6.0),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::monospace(9.0),
        accent,
    );
    painter.text(
        rect.left_bottom() + egui::vec2(10.0, -6.0),
        egui::Align2::LEFT_BOTTOM,
        value,
        egui::FontId::monospace(12.0),
        ui.visuals().text_color(),
    );
}

fn summary_path(ui: &mut egui::Ui, path: &str, accent: egui::Color32) {
    let fill = GORBIE::themes::blend(ui.visuals().window_fill, accent, 0.06);
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.label(egui::RichText::new("PATH").monospace().color(accent));
            ui.add(
                egui::Label::new(egui::RichText::new(path).monospace())
                    .truncate()
                    .wrap_mode(egui::TextWrapMode::Truncate),
            );
        });
}

fn summary_message(ui: &mut egui::Ui, label: &str, message: &str, accent: egui::Color32) {
    let fill = GORBIE::themes::blend(ui.visuals().window_fill, accent, 0.06);
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).monospace().color(accent));
            ui.add(
                egui::Label::new(egui::RichText::new(message).monospace())
                    .wrap_mode(egui::TextWrapMode::Wrap),
            );
        });
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

    state!(
        inspector = InspectorState {
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
        }
    );

    view!(move |ui| {
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

    view!(move |ui| {
        let state = ui.read(inspector).expect("inspector state missing");
        let now_ms = now_ms();
        let accent_ok = GORBIE::themes::ral(6024);
        let accent_warn = GORBIE::themes::ral(1023);
        let accent_error = GORBIE::themes::ral(3020);
        let accent_primary = GORBIE::themes::ral(2009);
        let accent_secondary = GORBIE::themes::ral(5015);

        let (status_label, status_color) = match &state.snapshot {
            ComputedState::Undefined => ("NO PILE", ui.visuals().widgets.noninteractive.bg_stroke.color),
            ComputedState::Init(_) => ("LOADING", accent_warn),
            ComputedState::Stale(_, _, _) => ("REFRESH", accent_warn),
            ComputedState::Ready(Ok(_), _) => ("READY", accent_ok),
            ComputedState::Ready(Err(_), _) => ("ERROR", accent_error),
        };

        let panel_fill = ui.visuals().widgets.noninteractive.bg_fill;
        let panel_stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        egui::Frame::new()
            .fill(panel_fill)
            .stroke(panel_stroke)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);

                summary_header(ui, "PILE SUMMARY", status_label, status_color);
                ui.add_space(8.0);

                match &state.snapshot {
                    ComputedState::Undefined => {
                        summary_message(ui, "STATE", "No pile loaded yet.", status_color);
                    }
                    ComputedState::Init(_) => {
                        summary_message(ui, "STATE", "Loading pile data.", status_color);
                    }
                    ComputedState::Stale(_, _, _) => {
                        summary_message(ui, "STATE", "Refreshing pile data.", status_color);
                    }
                    ComputedState::Ready(Err(err), _) => {
                        summary_message(ui, "ERROR", &format!("{err}"), status_color);
                    }
                    ComputedState::Ready(Ok(snapshot), _) => {
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

                        let tile_spacing = ui.spacing().item_spacing.x;
                        let tile_width =
                            ((ui.available_width() - tile_spacing * 2.0) / 3.0).max(120.0);

                        ui.horizontal_wrapped(|ui| {
                            summary_tile(
                                ui,
                                "SIZE",
                                format_bytes(snapshot.file_len),
                                accent_primary,
                                tile_width,
                            );
                            summary_tile(ui, "BLOBS", blob_count, accent_secondary, tile_width);
                            summary_tile(
                                ui,
                                "BRANCHES",
                                branch_count,
                                accent_secondary,
                                tile_width,
                            );
                        });

                        summary_path(ui, &snapshot.path.display().to_string(), accent_primary);

                        let meta_width =
                            ((ui.available_width() - tile_spacing) / 2.0).max(160.0);
                        ui.horizontal_wrapped(|ui| {
                            summary_meta(ui, "OLDEST", &oldest, accent_warn, meta_width);
                            summary_meta(ui, "NEWEST", &newest, accent_warn, meta_width);
                        });
                    }
                }
            });
    });

    view!(move |ui| {
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

    view!(move |ui| {
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
}
