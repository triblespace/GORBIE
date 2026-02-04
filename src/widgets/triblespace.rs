use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use triblespace::core::blob::schemas::simplearchive::{SimpleArchive, UnarchiveError};
use triblespace::core::patch::{IdentitySchema, PATCH};
use triblespace::core::repo::{
    BlobStore, BlobStoreGet, CommitSelector, Workspace, WorkspaceCheckoutError,
};
use triblespace::core::value::schemas::hash::{Blake3, Handle};
use triblespace::core::value::{RawValue, Value, VALUE_LEN};

pub mod commit_history;
pub mod entity_inspector;
pub mod pile_repo;
#[cfg(feature = "gloss")]
pub mod pile_overview;

pub use commit_history::CommitHistoryResponse;
pub use commit_history::CommitHistoryState;
pub use commit_history::CommitHistoryWidget;
pub use entity_inspector::id_full;
pub use entity_inspector::id_short;
pub use entity_inspector::EntityInspectorResponse;
pub use entity_inspector::EntityInspectorStats;
pub use entity_inspector::EntityInspectorWidget;
pub use entity_inspector::EntityOrder;
pub use pile_repo::PileRepoResponse;
pub use pile_repo::PileRepoState;
pub use pile_repo::PileRepoWidget;
#[cfg(feature = "gloss")]
pub use pile_overview::PileOverviewData;
#[cfg(feature = "gloss")]
pub use pile_overview::PileOverviewPalette;
#[cfg(feature = "gloss")]
pub use pile_overview::PileOverviewResponse;
#[cfg(feature = "gloss")]
pub use pile_overview::PileOverviewState;
#[cfg(feature = "gloss")]
pub use pile_overview::PileOverviewTuning;
#[cfg(feature = "gloss")]
pub use pile_overview::PileOverviewWidget;

type CommitHandle = Value<Handle<Blake3, SimpleArchive>>;
type CommitSet = PATCH<VALUE_LEN, IdentitySchema, ()>;

/// Metadata used to render a commit card.
#[derive(Clone, Debug)]
pub struct CommitInfo {
    pub parents: Vec<RawValue>,
    pub summary: String,
    pub message: Option<String>,
    pub author: Option<RawValue>,
    pub timestamp_ms: Option<u64>,
}

/// Commit DAG data for the commit graph widget.
#[derive(Clone, Debug)]
pub struct CommitGraph {
    pub order: Vec<RawValue>,
    pub commits: HashMap<RawValue, CommitInfo>,
    pub truncated: bool,
}

/// A labeled head reference used to seed commit lanes.
#[derive(Clone, Debug)]
pub struct CommitHead {
    pub label: String,
    pub commit: RawValue,
}

impl CommitHead {
    pub fn new(label: impl Into<String>, commit: RawValue) -> Self {
        Self {
            label: label.into(),
            commit,
        }
    }
}

/// Selection produced by the commit graph widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitSelection {
    None,
    Single(RawValue),
    Range { start: RawValue, end: RawValue },
}

impl Default for CommitSelection {
    fn default() -> Self {
        Self::None
    }
}

impl CommitSelection {
    pub fn label(self) -> Option<String> {
        match self {
            CommitSelection::None => None,
            CommitSelection::Single(commit) => {
                Some(format!("Selected commit: {}", hex_prefix(commit, 8)))
            }
            CommitSelection::Range { start, end } => Some(format!(
                "Selected range: {}..{}",
                hex_prefix(start, 8),
                hex_prefix(end, 8)
            )),
        }
    }
}

/// Mutable selection state for the commit graph widget.
#[derive(Clone, Debug, Default)]
pub struct CommitSelectionState {
    anchor: Option<RawValue>,
    focus: Option<RawValue>,
}

impl CommitSelectionState {
    pub fn selection(&self) -> CommitSelection {
        match (self.anchor, self.focus) {
            (Some(anchor), Some(focus)) if anchor == focus => CommitSelection::Single(anchor),
            (Some(anchor), Some(focus)) => CommitSelection::Range {
                start: anchor,
                end: focus,
            },
            (Some(anchor), None) => CommitSelection::Single(anchor),
            _ => CommitSelection::None,
        }
    }

    pub fn set(&mut self, selection: CommitSelection) {
        match selection {
            CommitSelection::None => self.clear(),
            CommitSelection::Single(commit) => {
                self.anchor = Some(commit);
                self.focus = Some(commit);
            }
            CommitSelection::Range { start, end } => {
                self.anchor = Some(start);
                self.focus = Some(end);
            }
        }
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.focus = None;
    }
}

/// Output of the commit graph widget.
pub struct CommitGraphResponse {
    pub response: egui::Response,
    pub selection: CommitSelection,
    pub selection_changed: bool,
}

/// Commit graph widget that renders a commit DAG and updates selection state.
#[must_use = "Use `CommitGraphWidget::show(ui)` to render this widget."]
pub struct CommitGraphWidget<'a> {
    graph: &'a CommitGraph,
    heads: &'a [CommitHead],
    selection: &'a mut CommitSelectionState,
    card_width: f32,
}

impl<'a> CommitGraphWidget<'a> {
    pub fn new(
        graph: &'a CommitGraph,
        heads: &'a [CommitHead],
        selection: &'a mut CommitSelectionState,
    ) -> Self {
        Self {
            graph,
            heads,
            selection,
            card_width: 240.0,
        }
    }

    pub fn card_width(mut self, card_width: f32) -> Self {
        self.card_width = card_width.max(120.0);
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> CommitGraphResponse {
        let graph = self.graph;
        let heads = self.heads;
        let selection = self.selection;
        let card_width = self.card_width;

        let output = egui::ScrollArea::horizontal()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let mut head_labels: HashMap<RawValue, Vec<String>> = HashMap::new();
                let mut head_order = Vec::new();
                let mut seen_heads = HashSet::new();

                let mut sorted_heads: Vec<&CommitHead> = heads.iter().collect();
                sorted_heads.sort_by(|a, b| a.label.cmp(&b.label));
                for head in sorted_heads {
                    head_labels
                        .entry(head.commit)
                        .or_default()
                        .push(head.label.clone());
                    if seen_heads.insert(head.commit) {
                        head_order.push(head.commit);
                    }
                }

                let layout = layout_commit_graph(graph, &head_order);
                let line_height = ui.text_style_height(&egui::TextStyle::Small);
                let card_padding = egui::vec2(6.0, 3.0);
                let card_height = (line_height * 2.0 + card_padding.y * 2.0).max(20.0);
                let row_height = (card_height + 6.0).max(18.0);
                let lane_width = 18.0;
                let node_radius = 4.0;
                let label_gap = 8.0;

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
                let width =
                    left_padding + lanes_width + label_width + label_gap + card_width + 12.0;
                let height = top_padding + (rows as f32 * row_height) + 4.0;

                let head_set: HashSet<RawValue> = head_labels.keys().copied().collect();

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
                let selection_stroke = ui.visuals().selection.stroke;
                let selection_fill = ui.visuals().selection.bg_fill;
                let boundary_stroke =
                    egui::Stroke::new(selection_stroke.width.max(1.5), selection_stroke.color);
                let font_id = egui::TextStyle::Small.resolve(ui.style());
                let now_ms = now_ms();

                let mut cards = Vec::with_capacity(graph.order.len());
                for commit in &graph.order {
                    let Some(&(lane, row)) = layout.positions.get(commit) else {
                        continue;
                    };
                    let Some(info) = graph.commits.get(commit) else {
                        continue;
                    };
                    let pos =
                        origin + egui::vec2(lane as f32 * lane_width, row as f32 * row_height);
                    let card_rect = egui::Rect::from_min_size(
                        egui::pos2(card_origin_x, pos.y - card_height * 0.5),
                        egui::vec2(card_width, card_height),
                    );
                    cards.push(CommitCard {
                        commit: *commit,
                        info,
                        lane,
                        pos,
                        card_rect,
                    });
                }

                let selection_before = selection.selection();
                let mut clicked_commit = None;
                for card in &cards {
                    let response = ui.interact(
                        card.card_rect,
                        ui.id().with((card.commit, "card")),
                        egui::Sense::click(),
                    );
                    if response.clicked() {
                        clicked_commit = Some(card.commit);
                    }
                    if response.hovered() {
                        response.on_hover_text(commit_tooltip(now_ms, card.commit, card.info));
                    }
                }

                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    selection.clear();
                }

                if let Some(commit) = clicked_commit {
                    let shift = ui.input(|input| input.modifiers.shift);
                    if shift {
                        if selection.anchor.is_none() {
                            selection.anchor = Some(commit);
                        }
                        selection.focus = Some(commit);
                    } else {
                        selection.anchor = Some(commit);
                        selection.focus = Some(commit);
                    }
                }

                let selection_value = selection.selection();
                let selection_changed = selection_value != selection_before;
                if selection_changed {
                    ui.ctx().request_repaint();
                }
                let selected_set = selected_commits(graph, selection_value);
                let boundary_start = match selection_value {
                    CommitSelection::Range { start, .. } => Some(start),
                    _ => None,
                };

                for (commit, info) in &graph.commits {
                    let Some(&(lane, row)) = layout.positions.get(commit) else {
                        continue;
                    };
                    let start =
                        origin + egui::vec2(lane as f32 * lane_width, row as f32 * row_height);
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

                for card in &cards {
                    let is_head = head_set.contains(&card.commit);
                    let is_selected = selected_set.contains(&card.commit);
                    let is_boundary = boundary_start == Some(card.commit);
                    let color = lane_colors[card.lane % lane_colors.len()];

                    let node_stroke = if is_selected {
                        selection_stroke
                    } else if is_boundary {
                        boundary_stroke
                    } else if is_head {
                        egui::Stroke::new(1.5, color)
                    } else {
                        egui::Stroke::new(1.0, color)
                    };
                    let node_fill = if is_selected {
                        selection_fill
                    } else if is_head {
                        color
                    } else {
                        base_fill
                    };

                    painter.circle_filled(card.pos, node_radius, node_fill);
                    painter.circle_stroke(card.pos, node_radius, node_stroke);

                    let card_fill = if is_selected {
                        selection_fill
                    } else {
                        base_fill
                    };
                    let card_stroke = if is_selected {
                        selection_stroke
                    } else if is_boundary {
                        boundary_stroke
                    } else {
                        base_stroke
                    };
                    painter.rect_filled(card.card_rect, 6.0, card_fill);
                    painter.rect_stroke(card.card_rect, 6.0, card_stroke, egui::StrokeKind::Inside);

                    let summary = truncate_to_width(
                        ui,
                        &card.info.summary,
                        card_width - card_padding.x * 2.0,
                        &egui::TextStyle::Small,
                    );
                    let detail_line = commit_detail_line(now_ms, card.info);
                    let detail = truncate_to_width(
                        ui,
                        &detail_line,
                        card_width - card_padding.x * 2.0,
                        &egui::TextStyle::Small,
                    );
                    let text_pos = card.card_rect.min + card_padding;
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
                }

                for head in &head_order {
                    let Some(&(lane, row)) = layout.positions.get(head) else {
                        continue;
                    };
                    let Some(labels) = head_labels.get(head) else {
                        continue;
                    };
                    let label = labels.join(", ");
                    let pos =
                        origin + egui::vec2(lane as f32 * lane_width, row as f32 * row_height);
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

                (selection_value, selection_changed)
            });

        let response = ui.interact(
            output.inner_rect,
            output.id.with("commit_graph"),
            egui::Sense::hover(),
        );

        CommitGraphResponse {
            response,
            selection: output.inner.0,
            selection_changed: output.inner.1,
        }
    }
}

impl<Blobs> CommitSelector<Blobs> for CommitSelection
where
    Blobs: BlobStore<Blake3>,
{
    fn select(
        self,
        ws: &mut Workspace<Blobs>,
    ) -> Result<
        CommitSet,
        WorkspaceCheckoutError<<Blobs::Reader as BlobStoreGet<Blake3>>::GetError<UnarchiveError>>,
    > {
        match self {
            CommitSelection::None => Option::<CommitHandle>::None.select(ws),
            CommitSelection::Single(commit) => commit_handle(commit).select(ws),
            CommitSelection::Range { start, end } => {
                (commit_handle(start)..commit_handle(end)).select(ws)
            }
        }
    }
}

struct CommitCard<'a> {
    commit: RawValue,
    info: &'a CommitInfo,
    lane: usize,
    pos: egui::Pos2,
    card_rect: egui::Rect,
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

fn commit_tooltip(now_ms: u64, commit: RawValue, info: &CommitInfo) -> String {
    let mut tooltip = format!("hash: {}", hex_prefix(commit, 32));
    if let Some(message) = info.message.as_deref() {
        tooltip.push_str(&format!("\nmessage: {message}"));
    }
    if let Some(author) = info.author {
        tooltip.push_str(&format!("\nauthor: {}", hex_prefix(author, 12)));
    }
    if let Some(timestamp_ms) = info.timestamp_ms {
        tooltip.push_str(&format!("\nwhen: {}", format_age(now_ms, timestamp_ms)));
    }
    tooltip
}

fn selected_commits(graph: &CommitGraph, selection: CommitSelection) -> HashSet<RawValue> {
    match selection {
        CommitSelection::None => HashSet::new(),
        CommitSelection::Single(commit) => {
            let mut set = HashSet::new();
            set.insert(commit);
            set
        }
        CommitSelection::Range { start, end } => range_commits(graph, start, end),
    }
}

fn range_commits(graph: &CommitGraph, start: RawValue, end: RawValue) -> HashSet<RawValue> {
    let mut selected = HashSet::new();
    let mut stack = vec![end];
    while let Some(commit) = stack.pop() {
        if commit == start {
            continue;
        }
        if !selected.insert(commit) {
            continue;
        }
        if let Some(info) = graph.commits.get(&commit) {
            for parent in &info.parents {
                if *parent != start {
                    stack.push(*parent);
                }
            }
        }
    }
    selected
}

fn commit_handle(raw: RawValue) -> CommitHandle {
    Value::new(raw)
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

fn hex_prefix(bytes: impl AsRef<[u8]>, prefix_len: usize) -> String {
    let bytes = bytes.as_ref();
    let prefix_len = prefix_len.min(bytes.len());
    let mut out = String::with_capacity(prefix_len * 2);
    for byte in bytes.iter().take(prefix_len) {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}
