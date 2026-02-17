#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = "..", features = ["triblespace"] }
//! ed25519-dalek = "2.1.1"
//! egui = "0.33"
//! hifitime = "4.2.3"
//! rand_core = "0.9.5"
//! triblespace = { path = "../../triblespace-rs", features = ["wasm"] }
//! ```

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{SecretKey, SigningKey};
use hifitime::Epoch;
use rand_core::OsRng;
use rand_core::TryRngCore;
use triblespace::core::blob::schemas::longstring::LongString;
use triblespace::core::blob::schemas::simplearchive::SimpleArchive;
use triblespace::core::blob::schemas::wasmcode::WasmCode;
use triblespace::core::blob::BlobCache;
use triblespace::core::id::Id;
use triblespace::core::metadata;
use triblespace::core::repo::pile::{Pile, PileReader};
use triblespace::core::repo::{
    head as branch_head, message as commit_message, parent as commit_parent,
    short_message as commit_short_message, signed_by as commit_signed_by,
    timestamp as commit_timestamp, BlobStore, BlobStoreGet, BlobStoreList, BlobStoreMeta,
    BranchStore, Repository,
};
use triblespace::core::trible::TribleSet;
use triblespace::core::value::schemas::ed25519 as ed;
use triblespace::core::value::schemas::hash::{Blake3, Handle};
use triblespace::core::value::schemas::time::NsTAIInterval;
use triblespace::core::value::RawValue;
use triblespace::core::value::Value;
use triblespace::core::value_formatter::WasmValueFormatter;
use triblespace::macros::{find, id_hex, pattern};
use triblespace::prelude::View;
use GORBIE::cards::with_padding;
use GORBIE::dataflow::ComputedState;
use GORBIE::md;
use GORBIE::notebook;
use GORBIE::widgets;
use GORBIE::widgets::triblespace::{
    id_short, CommitGraph, CommitHead, CommitHistoryState, CommitHistoryWidget, CommitInfo,
    CommitSelection, CommitSelectionState, EntityInspectorWidget, PileOverviewData,
    PileOverviewState, PileOverviewTuning, PileOverviewWidget,
};
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
struct CheckoutData {
    data: TribleSet,
    metadata: TribleSet,
}

#[derive(Clone, Debug)]
struct CommitCheckout {
    selection: CommitSelection,
    result: Result<CheckoutData, String>,
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

fn open_pile(path: &PathBuf) -> Result<Pile, String> {
    let mut pile: Pile = Pile::open(path).map_err(|err| err.to_string())?;
    if let Err(err) = pile.restore() {
        // Avoid Drop warnings on early errors.
        let _ = pile.close();
        return Err(err.to_string());
    }
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
                    (handle: Value<Handle<Blake3, LongString>>),
                    pattern!(&metadata_set, [{ metadata::name: ?handle }])
                )
                .into_iter()
                .filter_map(|(handle,)| reader.get::<View<str>, LongString>(handle).ok())
                .map(|view| view.to_string())
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

fn checkout_space(
    pile: Pile,
    branch_id: Id,
    selection: CommitSelection,
) -> (Pile, Result<CheckoutData, String>) {
    let mut rng = OsRng;
    let mut secret = SecretKey::default();
    if let Err(err) = rng.try_fill_bytes(&mut secret) {
        return (pile, Err(format!("rng failed: {err}")));
    }
    let signing_key = SigningKey::from_bytes(&secret);
    let mut repo = Repository::new(pile, signing_key);
    let space_result = repo
        .pull(branch_id)
        .map_err(|err| format!("{err:?}"))
        .and_then(|mut ws| {
            ws.checkout_with_metadata(selection)
                .map(|(data, metadata)| CheckoutData { data, metadata })
                .map_err(|err| err.to_string())
        });
    let pile = repo.into_storage();
    (pile, space_result)
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

fn default_entity_selection() -> Id {
    id_hex!("11111111111111111111111111111111")
}

fn seed_entity_selection(space: &TribleSet) -> Id {
    space
        .iter()
        .next()
        .map(|trible| *trible.e())
        .unwrap_or_else(default_entity_selection)
}

struct InspectorState {
    pile_path: String,
    pile: Option<Pile>,
    pile_open_path: Option<PathBuf>,
    max_rows: usize,
    blob_page: usize,
    histogram_bytes: bool,
    snapshot: ComputedState<Option<Result<PileSnapshot, String>>>,
    commit_selection: CommitSelectionState,
    commit_checkout: Option<CommitCheckout>,
    entity_selection: Id,
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
            .field("commit_selection", &self.commit_selection)
            .field("commit_checkout", &self.commit_checkout)
            .field("entity_selection", &self.entity_selection)
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
            commit_selection: CommitSelectionState::default(),
            commit_checkout: None,
            entity_selection: default_entity_selection(),
        }
    }
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    let default_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./repo.pile".to_owned());
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;

    nb.view(|ui| {
        md!(
            ui,
            "# Triblespace pile inspector\n\nOpen a `.pile` file on disk and inspect its blob and branch indices.\n\nTip: pass a path as the first CLI arg to prefill this field."
        );
    });

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
            commit_selection: CommitSelectionState::default(),
            commit_checkout: None,
            entity_selection: default_entity_selection(),
        },
        move |ui, state| {
            with_padding(ui, padding, |ui| {
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
                                state.commit_selection.clear();
                                state.commit_checkout = None;
                                state.entity_selection = default_entity_selection();
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
        },
    );

    let summary_tuning = nb.state(
        "summary_tuning",
        PileOverviewTuning::default(),
        move |ui, tuning| {
            with_padding(ui, padding, |ui| {
                widgets::markdown(ui, "## Summary knobs");
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
            widgets::markdown(ui, "## Blob size distribution");

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
                widgets::markdown(ui, "_Load a pile to see the distribution._");
                return;
            };
            let Ok(snapshot) = result else {
                widgets::markdown(ui, "_Load a valid pile to see the distribution._");
                return;
            };

            let stats = &snapshot.blob_stats;
            if stats.valid_blobs == 0 {
                widgets::markdown(ui, "_No valid blob sizes found._");
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
                widgets::markdown(ui, "_No data to plot._");
                return;
            }

            ui.add(
                widgets::Histogram::new(&histogram_buckets, y_axis)
                    .plot_height(80.0)
                    .max_x_labels(7),
            );

            widgets::markdown(
                ui,
                &format!(
                    "_{} blobs, {} total._",
                    stats.valid_blobs,
                    format_bytes(stats.total_bytes)
                ),
            );
        });
    });

    nb.view(move |ui| {
        let summary_padding = egui::Margin::ZERO;
        let mut state = inspector.read_mut(ui);
        let tuning = summary_tuning.read(ui);
        with_padding(ui, summary_padding, |ui| {
            state.snapshot.poll();
            let snapshot_value = state.snapshot.value();
            let mut branch_heads = Vec::new();
            let overview_state = if state.snapshot.is_running() {
                let message = if snapshot_value.is_some() {
                    "Refreshing pile data."
                } else {
                    "Loading pile data."
                };
                PileOverviewState::Loading { message }
            } else {
                match snapshot_value.as_ref() {
                    None => PileOverviewState::Empty {
                        message: "No pile loaded yet.",
                    },
                    Some(Err(err)) => PileOverviewState::Error { message: err },
                    Some(Ok(snapshot)) => {
                        branch_heads
                            .extend(snapshot.branches.iter().filter_map(|branch| branch.head));
                        let data = PileOverviewData {
                            path: &snapshot.path,
                            file_len: snapshot.file_len,
                            blob_order: &snapshot.blob_order,
                            branch_heads: &branch_heads,
                            branch_count: snapshot.branches.len(),
                            oldest_ts: snapshot.blob_stats.oldest_ts,
                            newest_ts: snapshot.blob_stats.newest_ts,
                            reader: &snapshot.reader,
                        };
                        PileOverviewState::Ready { data }
                    }
                }
            };

            PileOverviewWidget::new(overview_state)
                .tuning(&tuning)
                .padding(summary_padding)
                .cache_id(ui.id().with("pile_overview"))
                .show(ui);
        });
    });

    nb.view(move |ui| {
        let mut state = inspector.read_mut(ui);
        with_padding(ui, padding, |ui| {
            widgets::markdown(ui, "## Commit graph");

            state.snapshot.poll();
            let snapshot_value = state.snapshot.value().clone();
            let mut commit_heads = Vec::new();
            let mut commit_graph = None;
            let history_state = match snapshot_value {
                None => {
                    if state.snapshot.is_running() {
                        let message = "Loading pile data.";
                        CommitHistoryState::Loading { message }
                    } else {
                        CommitHistoryState::Empty {
                            message: "Load a pile to see the commit graph.",
                        }
                    }
                }
                Some(Err(_)) => CommitHistoryState::Error {
                    message: "Load a valid pile to see the commit graph.",
                },
                Some(Ok(snapshot)) => {
                    if snapshot.branches.is_empty() {
                        CommitHistoryState::Empty {
                            message: "No branches found.",
                        }
                    } else if snapshot.commit_graph.order.is_empty() {
                        CommitHistoryState::Empty {
                            message: "No commits found.",
                        }
                    } else {
                        for branch in &snapshot.branches {
                            let Some(head) = branch.head else {
                                continue;
                            };
                            let label = branch
                                .name
                                .clone()
                                .unwrap_or_else(|| hex_prefix(branch.id, 6));
                            commit_heads.push(CommitHead::new(label, head));
                        }

                        if commit_heads.is_empty() {
                            CommitHistoryState::Empty {
                                message: "No branch heads found.",
                            }
                        } else {
                            commit_graph.get_or_insert(snapshot.commit_graph);
                            let graph = commit_graph.as_ref().expect("commit graph missing");
                            CommitHistoryState::Ready {
                                graph,
                                heads: &commit_heads,
                            }
                        }
                    }
                }
            };

            CommitHistoryWidget::new(history_state, &mut state.commit_selection).show(ui);
        });
    });

    nb.view(move |ui| {
        let mut state = inspector.read_mut(ui);
        with_padding(ui, padding, |ui| {
            widgets::markdown(ui, "## Commit checkout");

            state.snapshot.poll();
            let snapshot_value = state.snapshot.value().clone();
            let snapshot = match snapshot_value {
                None => {
                    widgets::markdown(ui, "_Load a pile to inspect commits._");
                    return;
                }
                Some(Err(_)) => {
                    widgets::markdown(ui, "_Load a valid pile to inspect commits._");
                    return;
                }
                Some(Ok(snapshot)) => snapshot,
            };

            let selection = state.commit_selection.selection();
            if selection == CommitSelection::None {
                state.commit_checkout = None;
                state.entity_selection = default_entity_selection();
                widgets::markdown(ui, "_Select a commit or range in the graph to inspect._");
                return;
            }

            if let Some(label) = selection.label() {
                ui.label(egui::RichText::new(label).small());
            }

            let needs_checkout = state
                .commit_checkout
                .as_ref()
                .map(|checkout| checkout.selection != selection)
                .unwrap_or(true);

            if needs_checkout {
                let result = if let Some(pile) = state.pile.take() {
                    let branch_id = snapshot
                        .branches
                        .iter()
                        .find(|branch| branch.head.is_some())
                        .map(|branch| branch.id)
                        .or_else(|| snapshot.branches.first().map(|branch| branch.id));
                    let result = if let Some(branch_id) = branch_id {
                        let (pile, result) = checkout_space(pile, branch_id, selection);
                        state.pile = Some(pile);
                        result
                    } else {
                        state.pile = Some(pile);
                        Err("No branches available for checkout.".to_owned())
                    };
                    result
                } else {
                    Err("Pile is not open.".to_owned())
                };

                state.entity_selection = match result.as_ref() {
                    Ok(checkout) => seed_entity_selection(&checkout.data),
                    Err(_) => default_entity_selection(),
                };
                state.commit_checkout = Some(CommitCheckout { selection, result });
            }

            let Some(checkout) = state.commit_checkout.take() else {
                widgets::markdown(ui, "_No commit data loaded yet._");
                return;
            };

            let checkout_data = match checkout.result.as_ref() {
                Ok(checkout_data) => checkout_data,
                Err(err) => {
                    widgets::markdown(ui, &format!("_Checkout failed: {err}_"));
                    state.commit_checkout = Some(checkout);
                    return;
                }
            };

            let formatter_cache: BlobCache<_, Blake3, WasmCode, WasmValueFormatter> =
                BlobCache::new(snapshot.reader.clone());
            let name_cache: BlobCache<_, Blake3, LongString, View<str>> =
                BlobCache::new(snapshot.reader.clone());
            let response = EntityInspectorWidget::new(
                &checkout_data.data,
                &checkout_data.metadata,
                &name_cache,
                &formatter_cache,
                &mut state.entity_selection,
            )
            .cache_id(ui.id().with("commit_checkout_graph"))
            .show(ui);
            state.commit_checkout = Some(checkout);
            let stats = response.stats;
            if stats.nodes == 0 {
                widgets::markdown(ui, "_Selection has no entities._");
                return;
            }
            let selected = state.entity_selection;
            ui.label(
                egui::RichText::new(format!(
                    "Entities: {}  Edges: {}  Components: {}  Columns: {}",
                    stats.nodes, stats.edges, stats.connected_components, stats.columns
                ))
                .small(),
            );
            ui.label(
                egui::RichText::new(format!("Selected entity: {}", id_short(selected))).small(),
            );
        });
    });

    nb.view(move |ui| {
        let mut state = inspector.read_mut(ui);
        with_padding(ui, padding, |ui| {
            widgets::markdown(ui, "## Blobs");

            state.snapshot.poll();
            let snapshot_value = state.snapshot.value().as_ref();
            let Some(result) = snapshot_value else {
                widgets::markdown(ui, "_Load a pile to see blobs._");
                return;
            };
            let Ok(snapshot) = result else {
                widgets::markdown(ui, "_Load a valid pile to see blobs._");
                return;
            };

            if snapshot.blob_order.is_empty() {
                widgets::markdown(ui, "_No blobs found._");
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

                if total_pages > 1 {
                    ui.add_space(6.0);
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
            });

            if total_pages > 0 {
                state.blob_page = page_next.min(total_pages.saturating_sub(1));
            } else {
                state.blob_page = 0;
            }
        });
    });
}
