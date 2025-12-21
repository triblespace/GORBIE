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
                ui.add(widgets::TextField::singleline(&mut state.pile_path));
                ui.label("Rows:");
                ui.add(
                    widgets::NumberField::new(&mut state.max_rows)
                        .constrain_value(&|next| next.clamp(10, 10_000))
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
                    ui.monospace(hex_prefix(branch.id, 6));
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

fn main() {
    notebook!(pile_inspector);
}
