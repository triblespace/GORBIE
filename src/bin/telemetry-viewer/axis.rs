// Session-comparison "Axis" view for `telemetry-viewer`.
//
// A small pivot model where the loaded sessions are an axis: X is the
// sessions of a branch in chronological order, each series is a span
// name, and Y is a render-time aggregate (min/p50/max/total/count) over
// that session's observations of the span name. This answers "how did
// measurement X change across sessions/versions" for ANY telemetry
// pile: session tick labels are discovered generically from the data
// (every short-string-valued attribute found on a session entity is
// offered — e.g. a bench run's commit or engine label appears without
// the viewer knowing any bench-specific ids).
//
// V2 (explicitly out of scope for now): group-by-equal-attribute-value —
// collapsing reruns that share a label value (e.g. the same commit)
// into one x position with a spread/error-bar instead of one x slot
// per session.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use triblespace::core::blob::encodings::longstring::LongString;
use triblespace::core::id::{ExclusiveId, Id};
use triblespace::core::inline::encodings::genid::GenId;
use triblespace::core::inline::encodings::hash::Handle;
use triblespace::core::inline::encodings::iu256::U256BE;
use triblespace::core::inline::encodings::shortstring::ShortString;
use triblespace::core::inline::encodings::UnknownInline;
use triblespace::core::inline::{Inline, InlineEncoding, IntoInline};
use triblespace::core::metadata::{self, MetaDescribe};
use triblespace::core::query::TriblePattern;
use triblespace::core::trible::TribleSet;
use triblespace::macros::{find, pattern};

use GORBIE::widgets;

use GORBIE::telemetry::schema as t;

use crate::app::{
    contains_case_insensitive_ascii, fmt_duration_ns, load_longstring, now_ms, u256be_to_u64,
    CommitHandle, RepoCache,
};

/// The per-session aggregate plotted on Y. Aggregation is strictly
/// render-time: the index keeps raw observations, and the pivot is
/// recomputed from them on every snapshot/draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AxisAgg {
    Min,
    P50,
    Max,
    Total,
    Count,
}

impl AxisAgg {
    pub(crate) fn label(self) -> &'static str {
        match self {
            AxisAgg::Min => "min",
            AxisAgg::P50 => "p50",
            AxisAgg::Max => "max",
            AxisAgg::Total => "total",
            AxisAgg::Count => "count",
        }
    }

    fn is_duration(self) -> bool {
        !matches!(self, AxisAgg::Count)
    }
}

/// One session column of the pivot: identity, generically discovered
/// short-string attributes (label choices), and raw duration
/// observations per span name.
#[derive(Clone, Debug)]
pub(crate) struct AxisSessionData {
    pub(crate) id: Id,
    pub(crate) title: Option<String>,
    pub(crate) attrs: HashMap<Id, String>,
    pub(crate) observations: HashMap<String, Vec<u64>>,
}

impl AxisSessionData {
    fn new(id: Id) -> Self {
        Self {
            id,
            title: None,
            attrs: HashMap::new(),
            observations: HashMap::new(),
        }
    }
}

/// Accumulated per-span facts until (session, name, duration) are all
/// known; `recorded` guards against double-counting on re-observation.
#[derive(Clone, Debug, Default)]
struct AxisSpanState {
    session: Option<Id>,
    name: Option<String>,
    duration_ns: Option<u64>,
    recorded: bool,
}

/// Incrementally maintained index over ALL sessions of a telemetry
/// branch (unlike `SessionIndex`, which narrows to one session). Kept
/// separate so switching the per-session view never resets the axis.
#[derive(Clone, Debug)]
pub(crate) struct AxisIndex {
    pub(crate) branch_id: Id,
    pub(crate) head: Option<CommitHandle>,
    /// Commit-level metadata facts (attribute descriptions), unioned
    /// across the loaded commits. Drives attribute classification.
    meta: TribleSet,
    long_cache: HashMap<[u8; 32], String>,
    sessions: HashMap<Id, AxisSessionData>,
    span_states: HashMap<Id, AxisSpanState>,
    span_names: BTreeSet<String>,
    /// Display names for discovered label attributes.
    attr_names: HashMap<Id, String>,
}

impl AxisIndex {
    fn new(branch_id: Id) -> Self {
        Self {
            branch_id,
            head: None,
            meta: TribleSet::new(),
            long_cache: HashMap::new(),
            sessions: HashMap::new(),
            span_states: HashMap::new(),
            span_names: BTreeSet::new(),
            attr_names: HashMap::new(),
        }
    }

    pub(crate) fn snapshot(&self, now_prefix: u32) -> AxisSnapshot {
        let mut sessions: Vec<AxisSessionData> = self.sessions.values().cloned().collect();
        order_sessions(&mut sessions, now_prefix);

        let mut label_attrs: Vec<(Id, String)> = self
            .attr_names
            .iter()
            .map(|(id, name)| (*id, name.clone()))
            .collect();
        label_attrs.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

        AxisSnapshot {
            sessions,
            span_names: self.span_names.iter().cloned().collect(),
            label_attrs,
        }
    }
}

/// Render-side copy of the pivot inputs: sessions in chronological
/// order, all span names, and the discovered label-attribute choices.
#[derive(Clone, Debug)]
pub(crate) struct AxisSnapshot {
    pub(crate) sessions: Vec<AxisSessionData>,
    pub(crate) span_names: Vec<String>,
    pub(crate) label_attrs: Vec<(Id, String)>,
}

/// The lower 32 bits of unix-epoch milliseconds at which a UFOID was
/// minted. Canonical telemetry sessions are UFOIDs, so this prefix
/// orders sessions chronologically (modulo the ~50-day rollover, which
/// [`order_sessions`] handles relative to `now`). Foreign ids without
/// the prefix still sort deterministically.
pub(crate) fn ufoid_millis_prefix(id: &Id) -> u32 {
    u32::from_be_bytes(id[0..4].try_into().expect("ids are 16 bytes"))
}

/// The current wall-clock reference for rollover-aware ordering.
pub(crate) fn now_prefix() -> u32 {
    now_ms() as u32
}

/// Sort sessions chronologically by session begin time (UFOID mint
/// prefix), rollover-aware relative to `now_prefix`: the session with
/// the largest wrapped distance from now is the oldest and sorts first.
pub(crate) fn order_sessions(sessions: &mut [AxisSessionData], now_prefix: u32) {
    sessions.sort_by(|a, b| {
        let da = now_prefix.wrapping_sub(ufoid_millis_prefix(&a.id));
        let db = now_prefix.wrapping_sub(ufoid_millis_prefix(&b.id));
        db.cmp(&da).then_with(|| a.id.cmp(&b.id))
    });
}

/// Render-time aggregate over one pivot cell. Observations below
/// `floor_ns` are rejected before aggregation (the floor knob; pass 0
/// for off). Returns `(value, observation_count)` where `value` is in
/// nanoseconds for duration aggregates and the count itself for
/// [`AxisAgg::Count`]; `None` when no observation survives the floor.
pub(crate) fn aggregate(
    observations: &[u64],
    agg: AxisAgg,
    floor_ns: u64,
) -> Option<(f64, usize)> {
    let mut passing: Vec<u64> = observations
        .iter()
        .copied()
        .filter(|ns| *ns >= floor_ns)
        .collect();
    if passing.is_empty() {
        return None;
    }
    let count = passing.len();
    let value = match agg {
        AxisAgg::Min => passing.iter().copied().min().unwrap_or(0) as f64,
        AxisAgg::Max => passing.iter().copied().max().unwrap_or(0) as f64,
        AxisAgg::Total => passing.iter().fold(0u64, |acc, ns| acc.saturating_add(*ns)) as f64,
        AxisAgg::Count => count as f64,
        AxisAgg::P50 => {
            // Nearest-rank p50 (matches MetricStrip::percentile).
            passing.sort_unstable();
            let rank = ((0.5 * count as f64).ceil() as usize).clamp(1, count);
            passing[rank - 1] as f64
        }
    };
    Some((value, count))
}

/// Extract one series: for each session (by chronological index) that
/// has observations of `name` surviving the floor, the aggregate value
/// and observation count. Sessions without survivors are gaps.
pub(crate) fn series_points(
    sessions: &[AxisSessionData],
    name: &str,
    agg: AxisAgg,
    floor_ns: u64,
) -> Vec<(usize, f64, usize)> {
    let mut out = Vec::new();
    for (idx, session) in sessions.iter().enumerate() {
        let Some(observations) = session.observations.get(name) else {
            continue;
        };
        if let Some((value, count)) = aggregate(observations, agg, floor_ns) {
            out.push((idx, value, count));
        }
    }
    out
}

/// Decode a raw value as a non-empty `ShortString`, if it is one.
fn decode_short_string(value: Inline<UnknownInline>) -> Option<String> {
    let short = ShortString::validate(value.transmute::<ShortString>()).ok()?;
    let text: String = short.try_from_inline().ok()?;
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Generic label-attribute discovery for one session entity.
///
/// Queries the session entity for ALL its attributes with a free
/// attribute variable (`space.pattern(session, ?attr, ?value)`) and
/// offers every short-string-valued one as a tick-label choice:
///
/// * an attribute the commit metadata declares (via
///   `metadata::value_encoding`) is offered iff it is declared as
///   `ShortString`;
/// * an undeclared attribute is offered iff its raw value validates as
///   a non-empty `ShortString` (canonical non-string values — ids,
///   handles, counters — start with a NUL byte or are not UTF-8, so
///   they self-exclude).
pub(crate) fn discover_session_attrs(
    space: &TribleSet,
    meta: &TribleSet,
    session: &Id,
) -> Vec<(Id, String)> {
    let shortstring_schema: Id = <ShortString as MetaDescribe>::id();
    let session_term: Inline<GenId> = (*session).to_inline();

    let mut out = Vec::new();
    for (attr_raw, value) in find!(
        (attr: Inline<GenId>, value: Inline<UnknownInline>),
        space.pattern(session_term, attr, value)
    ) {
        let Ok(attr_id) = attr_raw.try_from_inline::<Id>() else {
            continue;
        };

        let attr_entity = ExclusiveId::force_ref(&attr_id);
        let declared: Option<Id> = find!(
            (enc: Inline<GenId>),
            pattern!(meta, [{ attr_entity @ metadata::value_encoding: ?enc }])
        )
        .into_iter()
        .next()
        .and_then(|(enc,)| enc.try_from_inline().ok());

        let offered = match declared {
            Some(encoding) if encoding == shortstring_schema => decode_short_string(value),
            Some(_) => None,
            None => decode_short_string(value),
        };
        if let Some(text) = offered {
            out.push((attr_id, text));
        }
    }
    out
}

/// The display name for a label attribute, from the commit metadata:
/// `metadata::name` on the attribute entity itself (dynamic
/// attributes), else on a usage annotation linked to it via
/// `metadata::attribute` (the `attributes!{}` describe() shape), else
/// the attribute id in hex.
fn attr_display_name(
    meta: &TribleSet,
    ws: &mut triblespace::core::repo::Workspace<triblespace::core::repo::pile::Pile>,
    long_cache: &mut HashMap<[u8; 32], String>,
    attr_id: &Id,
) -> String {
    let attr_entity = ExclusiveId::force_ref(attr_id);
    let attr_term: Inline<GenId> = (*attr_id).to_inline();

    let direct = find!(
        (name: Inline<Handle<LongString>>),
        pattern!(meta, [{ attr_entity @ metadata::name: ?name }])
    )
    .into_iter()
    .next();

    let handle = direct.or_else(|| {
        find!(
            (name: Inline<Handle<LongString>>),
            pattern!(meta, [{
                metadata::attribute: attr_term,
                metadata::name: ?name,
            }])
        )
        .into_iter()
        .next()
    });

    handle
        .and_then(|(h,)| load_longstring(ws, h, long_cache).ok())
        .unwrap_or_else(|| format!("{attr_id:x}"))
}

/// Incrementally (re)load the axis index for a branch: only the delta
/// since the previously observed head is checked out, mirroring
/// `load_session`.
pub(crate) fn load_axis(
    cache: &mut RepoCache,
    pile_path: PathBuf,
    branch_id: Id,
    prev: Option<AxisIndex>,
) -> Result<AxisIndex, String> {
    cache.ensure_open(&pile_path)?;
    let repo = cache
        .repo
        .as_mut()
        .and_then(|repo| repo.as_mut())
        .ok_or_else(|| "repo missing after open".to_owned())?;

    let mut ws = repo
        .pull(branch_id)
        .map_err(|err| format!("pull branch {branch_id:x}: {err:?}"))?;

    let head = ws.head();
    let mut index = prev
        .filter(|prev| prev.branch_id == branch_id)
        .unwrap_or_else(|| AxisIndex::new(branch_id));

    let (space, meta_delta) = match (index.head, head) {
        (Some(prev_head), Some(new_head)) if prev_head == new_head => {
            (TribleSet::new(), TribleSet::new())
        }
        (Some(prev_head), Some(_)) => ws
            .checkout_with_metadata(prev_head..)
            .map_err(|err| format!("checkout delta: {err}"))?,
        (None, Some(_)) => ws
            .checkout_with_metadata(..)
            .map_err(|err| format!("checkout branch: {err}"))?,
        (_, None) => (TribleSet::new(), TribleSet::new()),
    };
    index.head = head;
    index.meta += meta_delta;

    // Sessions appearing in this delta.
    for (session_id,) in find!(
        (id: triblespace::core::id::Id),
        pattern!(&space, [{ ?id @ metadata::tag: t::kind_session }])
    ) {
        index
            .sessions
            .entry(session_id)
            .or_insert_with(|| AxisSessionData::new(session_id));
    }
    for (session_id, name_handle) in find!(
        (id: triblespace::core::id::Id, name: Inline<Handle<LongString>>),
        pattern!(&space, [{
            ?id @
                metadata::tag: t::kind_session,
                t::name: ?name,
        }])
    ) {
        let title = load_longstring(&mut ws, name_handle, &mut index.long_cache).ok();
        index
            .sessions
            .entry(session_id)
            .or_insert_with(|| AxisSessionData::new(session_id))
            .title = title;
    }

    // Generic label-attribute discovery over this delta, for every
    // known session (attributes can be appended by later commits, e.g.
    // a bench labeling its run after the session opened).
    let session_ids: Vec<Id> = index.sessions.keys().copied().collect();
    for session_id in session_ids {
        for (attr_id, text) in discover_session_attrs(&space, &index.meta, &session_id) {
            if !index.attr_names.contains_key(&attr_id) {
                let name = attr_display_name(&index.meta, &mut ws, &mut index.long_cache, &attr_id);
                index.attr_names.insert(attr_id, name);
            }
            if let Some(session) = index.sessions.get_mut(&session_id) {
                session.attrs.insert(attr_id, text);
            }
        }
    }

    // Span observations: session + name arrive with the begin fact,
    // duration with the completion fact — possibly in different deltas.
    for (span_id, span_session, name_handle) in find!(
        (
            span: triblespace::core::id::Id,
            session: triblespace::core::id::Id,
            name: Inline<Handle<LongString>>
        ),
        pattern!(&space, [{
            ?span @
                metadata::tag: t::kind_span,
                t::session: ?session,
                t::name: ?name,
        }])
    ) {
        let name = load_longstring(&mut ws, name_handle, &mut index.long_cache)?;
        let state = index.span_states.entry(span_id).or_default();
        state.session = Some(span_session);
        state.name = Some(name);
    }

    for (span_id, dur_raw) in find!(
        (span: triblespace::core::id::Id, dur: Inline<U256BE>),
        pattern!(&space, [{ ?span @ t::duration_ns: ?dur }])
    ) {
        let Some(dur) = u256be_to_u64(dur_raw) else {
            continue;
        };
        let state = index.span_states.entry(span_id).or_default();
        if state.duration_ns.is_none() {
            state.duration_ns = Some(dur);
        }
    }

    // Record every span whose (session, name, duration) is now complete.
    for state in index.span_states.values_mut() {
        if state.recorded {
            continue;
        }
        let (Some(session_id), Some(name), Some(dur)) =
            (state.session, state.name.as_ref(), state.duration_ns)
        else {
            continue;
        };
        index.span_names.insert(name.clone());
        index
            .sessions
            .entry(session_id)
            .or_insert_with(|| AxisSessionData::new(session_id))
            .observations
            .entry(name.clone())
            .or_default()
            .push(dur);
        state.recorded = true;
    }

    Ok(index)
}

#[derive(Default)]
pub(crate) struct AxisLoader {
    cache: RepoCache,
    pub(crate) result: Option<Result<AxisIndex, String>>,
}

impl std::fmt::Debug for AxisLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AxisLoader")
            .field("cache", &self.cache)
            .field("loaded", &self.result.is_some())
            .finish()
    }
}

impl AxisLoader {
    pub(crate) fn refresh(&mut self, pile_path: PathBuf, branch_id: Id) {
        let prev = self
            .result
            .take()
            .and_then(|res| res.ok())
            .filter(|prev| prev.branch_id == branch_id);

        self.result = Some(load_axis(&mut self.cache, pile_path, branch_id, prev));
    }
}

/// UI state of the axis panel.
#[derive(Debug)]
pub(crate) struct AxisViewState {
    /// Which discovered attribute labels the x ticks; `None` falls
    /// back to the session name/id.
    label_attr: Option<Id>,
    selected_series: BTreeSet<String>,
    series_initialized: bool,
    search: String,
    agg: AxisAgg,
    floor_enabled: bool,
    floor_us: u64,
}

impl Default for AxisViewState {
    fn default() -> Self {
        Self {
            label_attr: None,
            selected_series: BTreeSet::new(),
            series_initialized: false,
            search: String::new(),
            agg: AxisAgg::P50,
            floor_enabled: false,
            floor_us: FLOOR_PRESET_US,
        }
    }
}

const FLOOR_PRESET_US: u64 = 20;

fn session_label(session: &AxisSessionData, label_attr: Option<&Id>) -> String {
    if let Some(attr) = label_attr {
        if let Some(value) = session.attrs.get(attr) {
            return value.clone();
        }
    }
    if let Some(title) = session.title.as_deref() {
        return title.to_owned();
    }
    format!("{:x}", session.id)
}

fn truncate_label(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        return label.to_owned();
    }
    let head: String = label.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{head}…")
}

/// Series are distinguished by hue AND point-marker shape — never by a
/// red-vs-green pair alone. Colors come from the colorblind-safe RAL
/// palette (orange/blue axis + luminance carry the signal); the shape
/// cycle length (7) is coprime with the palette length (8), so
/// (color, shape) pairs stay unique for 56 series.
fn series_color(idx: usize) -> egui::Color32 {
    let palette = GORBIE::themes::colorhash::RAL_CVD_SAFE;
    GORBIE::themes::ral(palette[idx % palette.len()])
}

#[cfg(feature = "plots")]
const SERIES_SHAPES: [egui_plot::MarkerShape; 7] = [
    egui_plot::MarkerShape::Circle,
    egui_plot::MarkerShape::Diamond,
    egui_plot::MarkerShape::Square,
    egui_plot::MarkerShape::Up,
    egui_plot::MarkerShape::Cross,
    egui_plot::MarkerShape::Plus,
    egui_plot::MarkerShape::Down,
];

fn fmt_axis_value(agg: AxisAgg, value: f64, count: usize) -> String {
    if agg.is_duration() {
        format!("{}  (n={count})", fmt_duration_ns(value.round() as u64))
    } else {
        format!("{} obs", count)
    }
}

pub(crate) fn show_axis(ui: &mut egui::Ui, state: &mut AxisViewState, snapshot: &AxisSnapshot) {
    ui.heading("Axis");
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(format!(
            "{} sessions × {} span names — sessions in session-begin order.",
            snapshot.sessions.len(),
            snapshot.span_names.len()
        ))
        .italics()
        .small(),
    );
    ui.add_space(6.0);

    if snapshot.sessions.is_empty() {
        ui.label(egui::RichText::new("<no sessions>").italics().small());
        return;
    }

    // Default to the two busiest span names so the first open shows data.
    if !state.series_initialized && !snapshot.span_names.is_empty() {
        let mut by_volume: Vec<(usize, &String)> = snapshot
            .span_names
            .iter()
            .map(|name| {
                let total: usize = snapshot
                    .sessions
                    .iter()
                    .map(|s| s.observations.get(name).map_or(0, Vec::len))
                    .sum();
                (total, name)
            })
            .collect();
        by_volume.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        for (_, name) in by_volume.into_iter().take(2) {
            state.selected_series.insert(name.clone());
        }
        state.series_initialized = true;
    }

    // Label attribute + aggregate.
    ui.horizontal_wrapped(|ui| {
        widgets::row_label(ui, "Tick label:");
        let selected_text = state
            .label_attr
            .as_ref()
            .and_then(|attr| {
                snapshot
                    .label_attrs
                    .iter()
                    .find(|(id, _)| id == attr)
                    .map(|(_, name)| name.clone())
            })
            .unwrap_or_else(|| "session name/id".to_owned());
        egui::ComboBox::from_id_salt("telemetry_axis_label_attr")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.label_attr.is_none(), "session name/id")
                    .clicked()
                {
                    state.label_attr = None;
                }
                for (attr_id, name) in &snapshot.label_attrs {
                    let selected = state.label_attr.as_ref() == Some(attr_id);
                    if ui.selectable_label(selected, name).clicked() {
                        state.label_attr = Some(*attr_id);
                    }
                }
            });

        ui.add_space(12.0);
        widgets::row_label(ui, "Y:");
        ui.add(
            widgets::ChoiceToggle::new(&mut state.agg)
                .choice(AxisAgg::Min, "min")
                .choice(AxisAgg::P50, "p50")
                .choice(AxisAgg::Max, "max")
                .choice(AxisAgg::Total, "total")
                .choice(AxisAgg::Count, "count"),
        );
    });

    // Floor-rejection knob. ASCII "us" — the label font has no µ glyph.
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        widgets::row_label(ui, "Floor:");
        ui.add(widgets::ChoiceToggle::binary(
            &mut state.floor_enabled,
            "off",
            "on",
        ))
        .on_hover_text("Exclude observations below the floor from every aggregate.");
        if ui
            .add(widgets::Button::new(format!("{FLOOR_PRESET_US}us")))
            .on_hover_text("Enable the floor at the 20us preset.")
            .clicked()
        {
            state.floor_us = FLOOR_PRESET_US;
            state.floor_enabled = true;
        }
        widgets::row_label(ui, "us:");
        let available_width = ui.available_width();
        ui.add_sized(
            [available_width, 0.0],
            widgets::NumberField::new(&mut state.floor_us)
                .constrain_value(&|_, next| next.min(60_000_000))
                .speed(5.0),
        )
        .on_hover_text("Floor in microseconds.");
    });

    // Span-name multi-select (searchable).
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        widgets::row_label(ui, "Series:");
        let available_width = ui.available_width();
        ui.add_sized(
            [available_width, 0.0],
            widgets::TextField::singleline(&mut state.search),
        )
        .on_hover_text("Case-insensitive substring filter over span names.");
    });
    ui.add_space(2.0);
    let needle = state.search.trim().to_ascii_lowercase();
    egui::ScrollArea::vertical()
        .id_salt("telemetry_axis_series")
        .max_height(140.0)
        .show(ui, |ui| {
            for name in &snapshot.span_names {
                if !needle.is_empty()
                    && !contains_case_insensitive_ascii(name, needle.as_bytes())
                {
                    continue;
                }
                let selected = state.selected_series.contains(name);
                if ui
                    .selectable_label(selected, egui::RichText::new(name).monospace().small())
                    .clicked()
                {
                    if selected {
                        state.selected_series.remove(name);
                    } else {
                        state.selected_series.insert(name.clone());
                    }
                }
            }
        });
    ui.label(
        egui::RichText::new(format!("{} selected", state.selected_series.len()))
            .italics()
            .small(),
    );

    if state.selected_series.is_empty() {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Select span names above to plot them across sessions.")
                .italics()
                .small(),
        );
        return;
    }

    let floor_ns = if state.floor_enabled {
        state.floor_us.saturating_mul(1_000)
    } else {
        0
    };
    let label_attr = state.label_attr;
    let labels: Vec<String> = snapshot
        .sessions
        .iter()
        .map(|s| session_label(s, label_attr.as_ref()))
        .collect();

    // (series name, [(session index, value, observation count)]) —
    // computed at render time from the raw observations.
    let series: Vec<(String, Vec<(usize, f64, usize)>)> = state
        .selected_series
        .iter()
        .map(|name| {
            (
                name.clone(),
                series_points(&snapshot.sessions, name, state.agg, floor_ns),
            )
        })
        .collect();

    ui.add_space(8.0);
    show_axis_plot(ui, state.agg, &labels, &series);

    // Table fallback: the same numbers, exactly.
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Values").strong());
    ui.add_space(2.0);
    egui::ScrollArea::both()
        .id_salt("telemetry_axis_table")
        .max_height(260.0)
        .show(ui, |ui| {
            egui::Grid::new("telemetry_axis_grid")
                .striped(true)
                .min_col_width(60.0)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("session").small().strong());
                    for (name, _) in &series {
                        ui.label(
                            egui::RichText::new(truncate_label(name, 28))
                                .small()
                                .strong(),
                        )
                        .on_hover_text(format!("{} ({})", name, state.agg.label()));
                    }
                    ui.end_row();

                    for (idx, session) in snapshot.sessions.iter().enumerate() {
                        let title = session.title.as_deref().unwrap_or("<untitled>");
                        ui.label(
                            egui::RichText::new(truncate_label(&labels[idx], 24))
                                .monospace()
                                .small(),
                        )
                        .on_hover_text(format!("{title}\n{:x}", session.id));
                        for (_, points) in &series {
                            match points.iter().find(|(x, _, _)| *x == idx) {
                                Some((_, value, count)) => {
                                    ui.label(
                                        egui::RichText::new(fmt_axis_value(
                                            state.agg, *value, *count,
                                        ))
                                        .monospace()
                                        .small(),
                                    );
                                }
                                None => {
                                    ui.label(egui::RichText::new("–").small());
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
        });
}

#[cfg(feature = "plots")]
fn show_axis_plot(
    ui: &mut egui::Ui,
    agg: AxisAgg,
    labels: &[String],
    series: &[(String, Vec<(usize, f64, usize)>)],
) {
    use egui_plot::{Legend, Line, Plot, PlotPoints, Points};

    let axis_labels: Vec<String> = labels
        .iter()
        .map(|label| truncate_label(label, 14))
        .collect();
    let hover_labels: Vec<String> = labels.to_vec();
    let hover_series: Vec<(String, Vec<(usize, f64, usize)>)> = series.to_vec();
    let session_count = labels.len();

    // Duration aggregates plot in milliseconds; count plots raw.
    let y_scale = if agg.is_duration() { 1.0 / 1_000_000.0 } else { 1.0 };
    let y_unit = if agg.is_duration() { "ms" } else { "obs" };
    ui.label(
        egui::RichText::new(format!("Y: {} ({y_unit})", agg.label()))
            .italics()
            .small(),
    );

    let plot = Plot::new("telemetry_axis_plot")
        .height(280.0)
        .legend(Legend::default())
        .allow_drag(false)
        .allow_scroll(false)
        .allow_zoom(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .x_axis_formatter(move |mark, _range| {
            let rounded = mark.value.round();
            if (mark.value - rounded).abs() > 1e-3 || rounded < 0.0 {
                return String::new();
            }
            let idx = rounded as usize;
            axis_labels.get(idx).cloned().unwrap_or_default()
        })
        .label_formatter(move |name, point| {
            let idx = point.x.round();
            if idx < 0.0 || (point.x - idx).abs() > 0.5 {
                return String::new();
            }
            let idx = idx as usize;
            if idx >= session_count {
                return String::new();
            }
            let label = hover_labels.get(idx).cloned().unwrap_or_default();
            if name.is_empty() {
                return label;
            }
            match hover_series
                .iter()
                .find(|(series_name, _)| series_name == name)
                .and_then(|(_, points)| points.iter().find(|(x, _, _)| *x == idx))
            {
                Some((_, value, count)) => {
                    format!("{name}\n{label}\n{}", fmt_axis_value(agg, *value, *count))
                }
                None => format!("{name}\n{label}"),
            }
        });

    plot.show(ui, |plot_ui| {
        for (idx, (name, points)) in series.iter().enumerate() {
            let color = series_color(idx);
            let shape = SERIES_SHAPES[idx % SERIES_SHAPES.len()];
            let plot_points: Vec<[f64; 2]> = points
                .iter()
                .map(|(x, value, _)| [*x as f64, *value * y_scale])
                .collect();
            plot_ui.line(
                Line::new(name.clone(), PlotPoints::from(plot_points.clone()))
                    .color(color)
                    .width(1.5),
            );
            plot_ui.points(
                Points::new(name.clone(), PlotPoints::from(plot_points))
                    .color(color)
                    .shape(shape)
                    .filled(true)
                    .radius(4.0),
            );
        }
    });
}

#[cfg(not(feature = "plots"))]
fn show_axis_plot(
    ui: &mut egui::Ui,
    _agg: AxisAgg,
    _labels: &[String],
    _series: &[(String, Vec<(usize, f64, usize)>)],
) {
    ui.label(
        egui::RichText::new(
            "Chart requires the `plots` feature — rebuild with \
             `--features telemetry,plots`. The values table below has the same numbers.",
        )
        .italics()
        .small(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    use ed25519_dalek::SigningKey;
    use triblespace::core::repo::pile::Pile;
    use triblespace::core::repo::Repository;
    use triblespace::macros::entity;

    /// Bench-style label attributes a viewer must NOT know about;
    /// discovery has to surface them generically. Minted with
    /// `trible genid`.
    mod fixture {
        use triblespace::macros::attributes;

        attributes! {
            "6D222735450D3DB38112408DD94D6B2E" as pub engine_label:
                triblespace::core::inline::encodings::shortstring::ShortString;
            "4CD283B919CF91F20E333F065BCEAE8B" as pub commit_label:
                triblespace::core::inline::encodings::shortstring::ShortString;
        }
    }

    /// An attribute with NO metadata declaration in the pile — the
    /// heuristic (value validates as non-empty ShortString) must offer it.
    mod fixture_undeclared {
        use triblespace::macros::attributes;

        attributes! {
            "954403FAF210741DD66B9E174A067438" as pub adhoc_note:
                triblespace::core::inline::encodings::shortstring::ShortString;
        }
    }

    /// An attribute DECLARED as U256BE whose value bytes would pass the
    /// ShortString heuristic — the declaration must veto it.
    mod fixture_weird {
        use triblespace::macros::attributes;

        attributes! {
            "FA5A543A23000FAA72441E6E41C63C5E" as pub weird:
                triblespace::core::inline::encodings::iu256::U256BE;
        }
    }

    fn session_id_at(prefix: u32, salt: u8) -> Id {
        let mut raw = [0u8; 16];
        raw[0..4].copy_from_slice(&prefix.to_be_bytes());
        raw[4] = salt;
        raw[15] = 0x5A;
        Id::new(raw).expect("nonzero fixture id")
    }

    fn session_data(prefix: u32, salt: u8) -> AxisSessionData {
        AxisSessionData::new(session_id_at(prefix, salt))
    }

    #[test]
    fn sessions_order_chronologically() {
        let mut sessions = vec![
            session_data(3000, 3),
            session_data(1000, 1),
            session_data(2000, 2),
        ];
        order_sessions(&mut sessions, 5000);
        let prefixes: Vec<u32> = sessions
            .iter()
            .map(|s| ufoid_millis_prefix(&s.id))
            .collect();
        assert_eq!(prefixes, vec![1000, 2000, 3000]);
    }

    #[test]
    fn sessions_order_across_rollover() {
        // One session minted just before the 32-bit prefix rolled
        // over, one just after: the pre-rollover one is older.
        let mut sessions = vec![session_data(0x10, 2), session_data(0xFFFF_FFF0, 1)];
        order_sessions(&mut sessions, 0x20);
        let prefixes: Vec<u32> = sessions
            .iter()
            .map(|s| ufoid_millis_prefix(&s.id))
            .collect();
        assert_eq!(prefixes, vec![0xFFFF_FFF0, 0x10]);
    }

    #[test]
    fn aggregate_math() {
        let obs = [300_000u64, 100_000, 200_000];
        assert_eq!(aggregate(&obs, AxisAgg::Min, 0), Some((100_000.0, 3)));
        assert_eq!(aggregate(&obs, AxisAgg::Max, 0), Some((300_000.0, 3)));
        assert_eq!(aggregate(&obs, AxisAgg::Total, 0), Some((600_000.0, 3)));
        assert_eq!(aggregate(&obs, AxisAgg::Count, 0), Some((3.0, 3)));
        // Nearest-rank p50: sorted [100k, 200k, 300k] -> rank 2.
        assert_eq!(aggregate(&obs, AxisAgg::P50, 0), Some((200_000.0, 3)));
        // Even count: nearest-rank stays on the lower-middle element.
        let even = [100_000u64, 200_000, 300_000, 400_000];
        assert_eq!(aggregate(&even, AxisAgg::P50, 0), Some((200_000.0, 4)));
        assert_eq!(aggregate(&[], AxisAgg::P50, 0), None);
    }

    #[test]
    fn aggregate_floor_rejection() {
        let obs = [5_000u64, 100_000, 200_000, 300_000];
        // 20µs floor rejects the 5µs observation everywhere, count included.
        let floor = 20_000;
        assert_eq!(aggregate(&obs, AxisAgg::Min, floor), Some((100_000.0, 3)));
        assert_eq!(aggregate(&obs, AxisAgg::Count, floor), Some((3.0, 3)));
        assert_eq!(aggregate(&obs, AxisAgg::P50, floor), Some((200_000.0, 3)));
        // A floor above every observation empties the cell.
        assert_eq!(aggregate(&obs, AxisAgg::Min, 1_000_000), None);
    }

    #[test]
    fn series_extraction_skips_sessions_without_observations() {
        let mut a = session_data(1000, 1);
        a.observations
            .insert("query/total".to_owned(), vec![100_000, 300_000]);
        let b = session_data(2000, 2); // no observations at all
        let mut c = session_data(3000, 3);
        c.observations.insert("query/total".to_owned(), vec![50_000]);

        let mut sessions = vec![a, b, c];
        order_sessions(&mut sessions, 5000);

        let points = series_points(&sessions, "query/total", AxisAgg::Max, 0);
        assert_eq!(points, vec![(0, 300_000.0, 2), (2, 50_000.0, 1)]);

        // The floor can empty a cell, producing a fresh gap.
        let points = series_points(&sessions, "query/total", AxisAgg::Max, 60_000);
        assert_eq!(points, vec![(0, 300_000.0, 2)]);
    }

    #[test]
    fn discovery_is_generic_and_schema_aware() {
        let session = session_id_at(1000, 1);
        let session_entity = ExclusiveId::force_ref(&session);

        let mut space = TribleSet::new();
        space += entity! { session_entity @
            metadata::tag: t::kind_session,
            t::category: "session",
            t::begin_ns: 0u64,
            fixture::engine_label: "baseline",
            fixture_undeclared::adhoc_note: "warm-run",
            fixture_weird::weird: Inline::<U256BE>::new([0x41; 32]),
        };

        let engine_id = fixture::engine_label.id();
        let weird_id = fixture_weird::weird.id();
        let engine_entity = ExclusiveId::force_ref(&engine_id);
        let weird_entity = ExclusiveId::force_ref(&weird_id);
        let mut meta = TribleSet::new();
        meta += entity! { engine_entity @
            metadata::value_encoding: <ShortString as MetaDescribe>::id(),
        };
        meta += entity! { weird_entity @
            metadata::value_encoding: <U256BE as MetaDescribe>::id(),
        };

        let discovered: HashMap<Id, String> =
            discover_session_attrs(&space, &meta, &session).into_iter().collect();

        // Declared ShortString: offered with its value.
        assert_eq!(discovered.get(&engine_id).map(String::as_str), Some("baseline"));
        // Undeclared but ShortString-shaped: offered via the heuristic.
        let adhoc_id = fixture_undeclared::adhoc_note.id();
        assert_eq!(discovered.get(&adhoc_id).map(String::as_str), Some("warm-run"));
        // Canonical ShortString attribute of the schema itself: offered.
        let category_id = t::category.id();
        assert_eq!(discovered.get(&category_id).map(String::as_str), Some("session"));
        // Declared non-ShortString whose bytes would pass the heuristic: vetoed.
        assert!(!discovered.contains_key(&weird_id));
        // Numeric/id-valued attributes self-exclude (leading NUL byte).
        let begin_id = t::begin_ns.id();
        assert!(!discovered.contains_key(&begin_id));
        let tag_id = metadata::tag.id();
        assert!(!discovered.contains_key(&tag_id));
    }

    /// Builds a synthetic telemetry pile with 3 sessions × 2 span names
    /// × several observations each, through the same triblespace
    /// version and canonical schema ids the emitter uses.
    fn build_fixture_pile(path: &Path) -> (Id, [Id; 3]) {
        std::fs::File::create(path).expect("create fixture pile");
        let mut pile = Pile::open(path).expect("open fixture pile");
        pile.refresh().expect("refresh fixture pile");

        // Commit metadata: the canonical telemetry protocol description
        // plus the bench-local attribute descriptions (as Fragments, so
        // display-name blobs persist) plus explicit value_encoding facts
        // for the declared labels.
        let mut meta = t::build_telemetry_metadata();
        meta += fixture::describe();
        let engine_id = fixture::engine_label.id();
        let commit_id = fixture::commit_label.id();
        let engine_entity = ExclusiveId::force_ref(&engine_id);
        let commit_entity = ExclusiveId::force_ref(&commit_id);
        meta += entity! { engine_entity @
            metadata::value_encoding: <ShortString as MetaDescribe>::id(),
        };
        meta += entity! { commit_entity @
            metadata::value_encoding: <ShortString as MetaDescribe>::id(),
        };

        // Deterministic throwaway key — this pile is synthetic test data.
        let mut repo = Repository::new(pile, SigningKey::from_bytes(&[7u8; 32]), meta)
            .expect("create fixture repository");
        let branch_id = *repo
            .create_branch("telemetry-axis-fixture", None)
            .expect("create fixture branch");

        let runs: [(u32, u8, &str, &str, &str, &[(&str, &[u64])]); 3] = [
            (
                1000,
                1,
                "run-a",
                "baseline",
                "aaaa111",
                &[
                    ("query/total", &[100_000u64, 300_000, 200_000] as &[u64]),
                    ("load/total", &[50_000u64] as &[u64]),
                ],
            ),
            (
                2000,
                2,
                "run-b",
                "candidate",
                "bbbb222",
                &[
                    ("query/total", &[150_000u64, 250_000] as &[u64]),
                    ("load/total", &[60_000u64, 40_000] as &[u64]),
                ],
            ),
            (
                3000,
                3,
                "run-c",
                "candidate",
                "cccc333",
                &[("query/total", &[110_000u64] as &[u64])],
            ),
        ];

        let mut session_ids = [session_id_at(0, 9); 3];
        for (slot, (prefix, salt, title, engine, commit, spans)) in runs.iter().enumerate() {
            let session_id = session_id_at(*prefix, *salt);
            session_ids[slot] = session_id;
            let session_entity = ExclusiveId::force_ref(&session_id);

            let mut ws = repo.pull(branch_id).expect("pull fixture branch");
            let mut facts = TribleSet::new();
            facts += entity! { session_entity @
                metadata::tag: t::kind_session,
                t::category: "session",
                t::name: ws.put((*title).to_owned()),
                t::begin_ns: 0u64,
                fixture::engine_label: *engine,
                fixture::commit_label: *commit,
            };
            if *salt == 2 {
                facts += entity! { session_entity @
                    fixture_undeclared::adhoc_note: "warm-run",
                };
            }

            let mut begin = 1_000u64;
            for (name, durations) in spans.iter() {
                for duration in durations.iter() {
                    let span_id = *triblespace::core::id::ufoid();
                    let span_entity = ExclusiveId::force_ref(&span_id);
                    facts += entity! { span_entity @
                        metadata::tag: t::kind_span,
                        t::session: session_id,
                        t::category: "query",
                        t::name: ws.put((*name).to_owned()),
                        t::begin_ns: begin,
                    };
                    facts += entity! { span_entity @
                        t::end_ns: begin + duration,
                        t::duration_ns: *duration,
                    };
                    begin += duration + 1_000;
                }
            }

            ws.commit(facts, "fixture session");
            repo.push(&mut ws).expect("push fixture session");
        }

        repo.close().expect("close fixture repo");
        (branch_id, session_ids)
    }

    #[test]
    fn axis_pivot_over_synthetic_pile() {
        let dir = std::env::temp_dir().join(format!(
            "gorbie_axis_fixture_{pid}",
            pid = std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        // GORBIE_AXIS_FIXTURE_OUT overrides the pile location and keeps
        // it (for opening the viewer against the fixture manually).
        let (pile_path, keep) = match std::env::var("GORBIE_AXIS_FIXTURE_OUT") {
            Ok(out) => (PathBuf::from(out), true),
            Err(_) => (dir.join("axis_fixture.pile"), false),
        };
        let _ = std::fs::remove_file(&pile_path);
        let (branch_id, [a, b, c]) = build_fixture_pile(&pile_path);
        if keep {
            eprintln!(
                "fixture pile kept: {} branch {branch_id:x}",
                pile_path.display()
            );
        }

        let mut cache = RepoCache::default();
        let index = load_axis(&mut cache, pile_path.clone(), branch_id, None)
            .expect("load axis index");
        let snapshot = index.snapshot(5000);

        // Session ordering: chronological by begin (ufoid prefix).
        let ids: Vec<Id> = snapshot.sessions.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![a, b, c]);
        let titles: Vec<Option<&str>> = snapshot
            .sessions
            .iter()
            .map(|s| s.title.as_deref())
            .collect();
        assert_eq!(titles, vec![Some("run-a"), Some("run-b"), Some("run-c")]);

        // Span names across sessions.
        assert_eq!(
            snapshot.span_names,
            vec!["load/total".to_owned(), "query/total".to_owned()]
        );

        // Generic attribute discovery: bench labels appear with display
        // names resolved from the pile's own metadata.
        let engine_id = fixture::engine_label.id();
        let engines: Vec<Option<&str>> = snapshot
            .sessions
            .iter()
            .map(|s| s.attrs.get(&engine_id).map(String::as_str))
            .collect();
        assert_eq!(
            engines,
            vec![Some("baseline"), Some("candidate"), Some("candidate")]
        );
        let by_name: HashMap<&str, Id> = snapshot
            .label_attrs
            .iter()
            .map(|(id, name)| (name.as_str(), *id))
            .collect();
        assert_eq!(by_name.get("engine_label"), Some(&engine_id));
        assert_eq!(by_name.get("commit_label"), Some(&fixture::commit_label.id()));
        // The undeclared attribute (no metadata in the pile) is offered
        // heuristically; only run-b carries it.
        let adhoc_id = fixture_undeclared::adhoc_note.id();
        assert!(snapshot.label_attrs.iter().any(|(id, _)| *id == adhoc_id));
        assert_eq!(
            snapshot.sessions[1].attrs.get(&adhoc_id).map(String::as_str),
            Some("warm-run")
        );
        assert!(!snapshot.sessions[0].attrs.contains_key(&adhoc_id));

        // Aggregates over the pivot (render-time, from raw observations).
        let obs_a = &snapshot.sessions[0].observations["query/total"];
        assert_eq!(aggregate(obs_a, AxisAgg::P50, 0), Some((200_000.0, 3)));
        assert_eq!(aggregate(obs_a, AxisAgg::Min, 0), Some((100_000.0, 3)));
        assert_eq!(aggregate(obs_a, AxisAgg::Max, 0), Some((300_000.0, 3)));
        assert_eq!(aggregate(obs_a, AxisAgg::Total, 0), Some((600_000.0, 3)));
        assert_eq!(aggregate(obs_a, AxisAgg::Min, 150_000), Some((200_000.0, 2)));

        let series = series_points(&snapshot.sessions, "load/total", AxisAgg::Total, 0);
        assert_eq!(series, vec![(0, 50_000.0, 1), (1, 100_000.0, 2)]);

        // Incremental reload with an up-to-date index is a no-op.
        let head = index.head;
        let index = load_axis(&mut cache, pile_path.clone(), branch_id, Some(index))
            .expect("incremental reload");
        assert_eq!(index.head, head);
        assert_eq!(index.snapshot(5000).sessions.len(), 3);

        drop(cache);
        if !keep {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
