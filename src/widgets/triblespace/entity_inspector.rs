use std::collections::{HashMap, HashSet, VecDeque};
#[cfg(feature = "cubecl")]
use std::sync::mpsc;
use std::sync::Arc;
#[cfg(feature = "cubecl")]
use std::sync::Mutex;
#[cfg(feature = "cubecl")]
use std::time::Instant;

use eframe::egui;
use eframe::egui::{pos2, vec2, Align2, Rect, Response, Sense, Stroke, TextStyle, Ui};

#[cfg(feature = "cubecl")]
use cubecl::prelude::*;
#[cfg(feature = "cubecl")]
use cubecl::server::Handle as CubeHandle;
#[cfg(feature = "cubecl")]
use cubecl::wgpu::{WgpuDevice, WgpuRuntime};
use triblespace::core::blob::schemas::longstring::LongString;
use triblespace::core::blob::schemas::wasmcode::WasmCode;
use triblespace::core::blob::BlobCache;
use triblespace::core::id::Id;
use triblespace::core::query::TriblePattern;
use triblespace::core::repo::BlobStoreGet;
use triblespace::core::value::schemas::hash::{Blake3, Handle};
use triblespace::core::value::schemas::UnknownValue;
use triblespace::core::value::Value;
use triblespace::core::value_formatter::{WasmLimits, WasmValueFormatter};
use triblespace::prelude::valueschemas::GenId;
use triblespace::prelude::{find, pattern, TribleSet, TribleSetFingerprint, View};

use crate::themes;

fn hex_prefix(bytes: impl AsRef<[u8]>, prefix_len: usize) -> String {
    let bytes = bytes.as_ref();
    let prefix_len = prefix_len.min(bytes.len());
    let mut out = String::with_capacity(prefix_len * 2);
    for byte in bytes.iter().take(prefix_len) {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

pub fn id_short(id: Id) -> String {
    let bytes: &[u8] = id.as_ref();
    hex_prefix(bytes, 4)
}

pub fn id_full(id: Id) -> String {
    let bytes: &[u8] = id.as_ref();
    hex_prefix(bytes, 16)
}

fn try_decode_genid(raw: &[u8; 32]) -> Option<Id> {
    let v = triblespace::core::value::Value::<GenId>::new(*raw);
    let parsed: Result<Id, _> = v.try_from_value();
    parsed.ok()
}

fn paint_hatching(painter: &egui::Painter, rect: Rect, color: egui::Color32) {
    let spacing = 8.0;
    let stroke = Stroke::new(1.0, color);

    let h = rect.height();
    let mut x = rect.left() - h;
    while x < rect.right() + h {
        painter.line_segment([pos2(x, rect.top()), pos2(x + h, rect.bottom())], stroke);
        x += spacing;
    }
}

#[derive(Clone, Debug)]
struct EntityRow {
    attr_id: Id,
    attr: String,
    value: String,
    target: Option<Id>,
    hatched: bool,
}

#[derive(Clone, Debug)]
struct EntityNode {
    id: Id,
    title: String,
    rows: Vec<EntityRow>,
}

#[derive(Clone, Debug)]
struct EntityEdge {
    from_entity: usize,
    from_row: usize,
    to_entity: usize,
    attr_id: Id,
}

#[derive(Debug)]
struct EntityGraph {
    nodes: Vec<EntityNode>,
    edges: Vec<EntityEdge>,
    id_to_index: HashMap<Id, usize>,
}

#[derive(Clone, Debug)]
struct AttrInfo {
    label: String,
    schema: Option<Id>,
    formatter: Option<Value<Handle<Blake3, WasmCode>>>,
}

fn build_attr_info<B>(
    metadata: &TribleSet,
    name_cache: &BlobCache<B, Blake3, LongString, View<str>>,
) -> HashMap<Id, AttrInfo>
where
    B: BlobStoreGet<Blake3>,
{
    let mut labels = HashMap::<Id, String>::new();
    for (attr, name_handle) in find!(
        (attr: Id, name_handle: Value<Handle<Blake3, LongString>>),
        pattern!(metadata, [{ ?attr @ triblespace::core::metadata::name: ?name_handle }])
    ) {
        if let Ok(name) = name_cache.get(name_handle) {
            labels.insert(attr, name.as_ref().to_string());
        }
    }
    for (usage, attr, name_handle) in find!(
        (usage: Id, attr: Id, name_handle: Value<Handle<Blake3, LongString>>),
        pattern!(metadata, [
            { ?usage @ triblespace::core::metadata::attribute: ?attr },
            { ?usage @ triblespace::core::metadata::tag: triblespace::core::metadata::KIND_ATTRIBUTE_USAGE },
            { ?usage @ triblespace::core::metadata::name: ?name_handle },
        ])
    ) {
        let _ = usage;
        if labels.contains_key(&attr) {
            continue;
        }
        if let Ok(name) = name_cache.get(name_handle) {
            labels.insert(attr, name.as_ref().to_string());
        }
    }

    let mut schema_by_attr = HashMap::<Id, Id>::new();
    for (attr, schema) in find!(
        (attr: Id, schema: Id),
        pattern!(metadata, [{ ?attr @ triblespace::core::metadata::value_schema: ?schema }])
    ) {
        schema_by_attr.insert(attr, schema);
    }

    let mut formatter_by_schema = HashMap::<Id, Value<Handle<Blake3, WasmCode>>>::new();
    for (schema, formatter) in find!(
        (schema: Id, formatter: Value<Handle<Blake3, WasmCode>>),
        pattern!(metadata, [{ ?schema @ triblespace::core::metadata::value_formatter: ?formatter }])
    ) {
        formatter_by_schema.insert(schema, formatter);
    }

    let mut out = HashMap::<Id, AttrInfo>::new();
    for (attr, schema) in schema_by_attr {
        let label = labels
            .remove(&attr)
            .unwrap_or_else(|| format!("attr:{}", id_short(attr)));
        let formatter = formatter_by_schema.get(&schema).copied();
        out.insert(
            attr,
            AttrInfo {
                label,
                schema: Some(schema),
                formatter,
            },
        );
    }

    for (attr, label) in labels {
        out.entry(attr).or_insert(AttrInfo {
            label,
            schema: None,
            formatter: None,
        });
    }

    out
}

fn build_entity_graph<B>(
    data: &TribleSet,
    metadata: &TribleSet,
    name_cache: &BlobCache<B, Blake3, LongString, View<str>>,
    formatter_cache: &BlobCache<B, Blake3, WasmCode, WasmValueFormatter>,
) -> EntityGraph
where
    B: BlobStoreGet<Blake3>,
{
    let attr_info = build_attr_info(metadata, name_cache);
    let limits = WasmLimits::default();

    let schema_genid = GenId::id();
    let mut entity_ids = HashSet::<Id>::new();
    let mut tribles = Vec::<(Id, Id, [u8; 32])>::new();

    for (e, a, v) in find!((e: Id, a: Id, v: Value<UnknownValue>), data.pattern(e, a, v)) {
        entity_ids.insert(e);
        if let Some(info) = attr_info.get(&a) {
            if info.schema == Some(schema_genid) {
                if let Some(target) = try_decode_genid(&v.raw) {
                    entity_ids.insert(target);
                }
            }
        }

        tribles.push((e, a, v.raw));
    }

    let mut entities: Vec<Id> = entity_ids.into_iter().collect();
    entities.sort_by(|a, b| {
        let a: &[u8] = a.as_ref();
        let b: &[u8] = b.as_ref();
        a.cmp(b)
    });

    let mut id_to_index = HashMap::with_capacity(entities.len());
    for (idx, id) in entities.iter().copied().enumerate() {
        id_to_index.insert(id, idx);
    }

    let mut raw_rows = vec![Vec::<EntityRow>::new(); entities.len()];
    for (e, attr, raw) in tribles {
        let Some(entity_index) = id_to_index.get(&e).copied() else {
            continue;
        };

        let info = attr_info.get(&attr);
        let attr_text = info
            .map(|info| info.label.clone())
            .unwrap_or_else(|| format!("attr:{}", id_short(attr)));

        let (value_text, target, hatched) = match info {
            Some(info) if info.schema == Some(schema_genid) => {
                if let Some(target) = try_decode_genid(&raw) {
                    (format!("id:{}", id_short(target)), Some(target), false)
                } else {
                    (format!("id:0x{}", hex_prefix(raw, 6)), None, false)
                }
            }
            Some(info) => match info
                .formatter
                .and_then(|handle| formatter_cache.get(handle).ok())
            {
                Some(formatter) => match formatter.format_value_with_limits(&raw, limits) {
                    Ok(text) => (text, None, false),
                    Err(_) => (format!("0x{}", hex_prefix(raw, 6)), None, true),
                },
                None => (format!("0x{}", hex_prefix(raw, 6)), None, true),
            },
            None => (format!("0x{}", hex_prefix(raw, 6)), None, true),
        };

        raw_rows[entity_index].push(EntityRow {
            attr_id: attr,
            attr: attr_text,
            value: value_text,
            target,
            hatched,
        });
    }

    let mut titles = HashMap::<Id, String>::new();
    for (entity_idx, entity_id) in entities.iter().copied().enumerate() {
        for row in &raw_rows[entity_idx] {
            if row.attr == "name" && !row.hatched {
                titles.insert(entity_id, row.value.clone());
                break;
            }
        }
    }

    let mut nodes = Vec::with_capacity(entities.len());
    for (idx, id) in entities.iter().copied().enumerate() {
        let title = titles
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("id:{}", id_short(id)));

        let mut rows = raw_rows[idx].clone();
        rows.sort_by(|a, b| {
            a.attr
                .cmp(&b.attr)
                .then_with(|| a.hatched.cmp(&b.hatched))
                .then_with(|| a.value.cmp(&b.value))
        });

        nodes.push(EntityNode { id, title, rows });
    }

    let mut edges = Vec::new();
    for (from_entity, node) in nodes.iter().enumerate() {
        for (from_row, row) in node.rows.iter().enumerate() {
            let Some(target) = row.target else {
                continue;
            };
            let Some(&to_entity) = id_to_index.get(&target) else {
                continue;
            };
            edges.push(EntityEdge {
                from_entity,
                from_row,
                to_entity,
                attr_id: row.attr_id,
            });
        }
    }

    EntityGraph {
        nodes,
        edges,
        id_to_index,
    }
}

#[derive(Clone)]
struct EntityGraphCache {
    data_fingerprint: TribleSetFingerprint,
    metadata_fingerprint: TribleSetFingerprint,
    graph: Option<Arc<EntityGraph>>,
}

impl Default for EntityGraphCache {
    fn default() -> Self {
        Self {
            data_fingerprint: TribleSetFingerprint::EMPTY,
            metadata_fingerprint: TribleSetFingerprint::EMPTY,
            graph: None,
        }
    }
}

fn cached_entity_graph<B>(
    ui: &mut Ui,
    cache_id: egui::Id,
    data: &TribleSet,
    metadata: &TribleSet,
    name_cache: &BlobCache<B, Blake3, LongString, View<str>>,
    formatter_cache: &BlobCache<B, Blake3, WasmCode, WasmValueFormatter>,
) -> Arc<EntityGraph>
where
    B: BlobStoreGet<Blake3>,
{
    let data_fingerprint = data.fingerprint();
    let metadata_fingerprint = metadata.fingerprint();
    ui.data_mut(|memory| {
        let cache = memory.get_temp_mut_or_default::<EntityGraphCache>(cache_id);
        let needs_rebuild = cache.graph.is_none()
            || cache.data_fingerprint != data_fingerprint
            || cache.metadata_fingerprint != metadata_fingerprint;
        if needs_rebuild {
            cache.graph = Some(Arc::new(build_entity_graph(
                data,
                metadata,
                name_cache,
                formatter_cache,
            )));
            cache.data_fingerprint = data_fingerprint;
            cache.metadata_fingerprint = metadata_fingerprint;
        }
        cache
            .graph
            .as_ref()
            .expect("entity graph cache missing")
            .clone()
    })
}

#[derive(Clone, Debug)]
struct ComponentLayout {
    column_free: Vec<Vec<(f32, f32)>>,
}

#[derive(Clone, Debug)]
struct GraphLayout {
    canvas_size: egui::Vec2,
    column_count: usize,
    column_gap: f32,
    tile_padding: f32,
    header_height: f32,
    text_row_height: f32,
    tile_rects: Vec<Rect>,
    components: Vec<ComponentLayout>,
    node_component: Vec<usize>,
    node_column: Vec<usize>,
}

fn build_adjacency(graph: &EntityGraph) -> Vec<Vec<usize>> {
    let mut adj = vec![Vec::<usize>::new(); graph.nodes.len()];
    for edge in &graph.edges {
        adj[edge.from_entity].push(edge.to_entity);
        adj[edge.to_entity].push(edge.from_entity);
    }
    adj
}

fn connected_components(adjacency: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut visited = vec![false; adjacency.len()];
    let mut components = Vec::new();

    for start in 0..adjacency.len() {
        if visited[start] {
            continue;
        }

        visited[start] = true;
        let mut queue = VecDeque::new();
        queue.push_back(start);

        let mut component = vec![start];
        while let Some(node) = queue.pop_front() {
            for &next in &adjacency[node] {
                if !visited[next] {
                    visited[next] = true;
                    queue.push_back(next);
                    component.push(next);
                }
            }
        }

        component.sort_unstable();
        components.push(component);
    }

    components.sort_by_key(|component| component[0]);
    components
}

fn compute_graph_layout(
    ui: &Ui,
    graph: &EntityGraph,
    forced_columns: usize,
    order: &[usize],
) -> GraphLayout {
    let column_gap = 48.0;
    let outer_x_pad = column_gap;
    let min_tile_width = 160.0;
    let desired_tile_width = 220.0;
    let max_tile_width = 260.0;

    let usable_width = (ui.available_width() - outer_x_pad * 2.0).max(min_tile_width);
    let mut column_count = if forced_columns == 0 {
        ((usable_width + column_gap) / (desired_tile_width + column_gap)).floor() as usize
    } else {
        forced_columns
    };
    column_count = column_count.max(1).min(graph.nodes.len().max(1));

    let tile_width = loop {
        if column_count == 1 {
            break ui.available_width().clamp(180.0, 520.0);
        }

        let raw = (usable_width - column_gap * ((column_count - 1) as f32)) / column_count as f32;
        if raw >= min_tile_width {
            break raw.clamp(min_tile_width, max_tile_width);
        }

        if forced_columns != 0 {
            break raw.max(1.0);
        }

        column_count = column_count.saturating_sub(1).max(1);
    };

    let tile_padding = 8.0;
    let title_font = TextStyle::Monospace.resolve(ui.style());
    let row_font = TextStyle::Small.resolve(ui.style());
    let header_height = ui.fonts_mut(|fonts| fonts.row_height(&title_font)).ceil() + 6.0;
    let text_row_height = ui.fonts_mut(|fonts| fonts.row_height(&row_font)).ceil() + 4.0;

    let mut tile_heights = vec![0.0f32; graph.nodes.len()];
    for (idx, node) in graph.nodes.iter().enumerate() {
        let rows = node.rows.len().max(1);
        tile_heights[idx] = tile_padding * 2.0 + header_height + text_row_height * rows as f32;
    }

    let row_gap = 24.0;
    let top_pad = row_gap;
    let bottom_pad = row_gap;
    let clearance = 4.0;

    let node_count = graph.nodes.len();
    let mut tile_rects = vec![Rect::NOTHING; node_count];
    let node_component = vec![0usize; node_count];
    let mut node_column = vec![usize::MAX; node_count];

    let mut column_bottoms = vec![top_pad; column_count];
    let mut column_nodes = vec![Vec::<usize>::new(); column_count];

    // Baseline placement: pack lexicographically by entity id, with a simple "masonry" heuristic
    // (always place the next tile in the currently-shortest column).
    for &node_idx in order {
        let (col, y) = column_bottoms
            .iter()
            .copied()
            .enumerate()
            .min_by(|(a_idx, a_y), (b_idx, b_y)| {
                a_y.partial_cmp(b_y)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a_idx.cmp(b_idx))
            })
            .unwrap_or((0, top_pad));

        let x = outer_x_pad + col as f32 * (tile_width + column_gap);
        let rect = Rect::from_min_size(pos2(x, y), vec2(tile_width, tile_heights[node_idx]));
        tile_rects[node_idx] = rect;
        node_column[node_idx] = col;
        column_nodes[col].push(node_idx);
        column_bottoms[col] = y + tile_heights[node_idx] + row_gap;
    }

    for bottom in &mut column_bottoms {
        if *bottom > top_pad {
            *bottom -= row_gap;
        }
    }

    let content_height = column_bottoms.into_iter().fold(top_pad, f32::max);
    let canvas_height = (content_height + bottom_pad).max(top_pad + bottom_pad);

    let canvas_width = outer_x_pad * 2.0
        + tile_width * column_count as f32
        + column_gap * (column_count.saturating_sub(1) as f32);

    let mut column_free = Vec::with_capacity(column_count);
    for nodes in &column_nodes {
        let mut intervals = Vec::new();
        let mut cursor = 0.0f32;

        for &node_idx in nodes {
            let rect = tile_rects[node_idx];
            let top = (rect.top() - clearance).max(0.0);
            if top > cursor {
                intervals.push((cursor, top));
            }
            cursor = (rect.bottom() + clearance).max(cursor);
        }

        if cursor < canvas_height {
            intervals.push((cursor, canvas_height));
        }
        if intervals.is_empty() {
            intervals.push((0.0, canvas_height));
        }
        column_free.push(intervals);
    }

    let component_layout = ComponentLayout { column_free };

    GraphLayout {
        canvas_size: vec2(canvas_width, canvas_height),
        column_count,
        column_gap,
        tile_padding,
        header_height,
        text_row_height,
        tile_rects,
        components: vec![component_layout],
        node_component,
        node_column,
    }
}

#[derive(Clone, Debug, Default)]
pub struct EntityInspectorStats {
    pub nodes: usize,
    pub edges: usize,
    pub connected_components: usize,
    pub columns: usize,
    pub canvas_width: f32,
    pub canvas_height: f32,
    pub tile_coverage: f32,
    pub total_edge_len: f32,
    pub avg_edge_len: f32,
    pub max_edge_len: f32,
    pub avg_turns: f32,
    pub max_turns: usize,
    pub avg_span_cols: f32,
    pub max_span_cols: usize,
    pub left_edges: usize,
    pub fallback_tracks: usize,
    pub linear_total: f32,
    pub linear_avg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityOrder {
    Id,
    #[cfg(feature = "cubecl")]
    Anneal,
}

pub struct EntityInspectorResponse {
    pub response: egui::Response,
    pub stats: EntityInspectorStats,
    pub selection_changed: bool,
}

#[must_use = "Use `EntityInspectorWidget::show(ui)` to render this widget."]
pub struct EntityInspectorWidget<'a, B>
where
    B: BlobStoreGet<Blake3>,
{
    data: &'a TribleSet,
    metadata: &'a TribleSet,
    name_cache: &'a BlobCache<B, Blake3, LongString, View<str>>,
    formatter_cache: &'a BlobCache<B, Blake3, WasmCode, WasmValueFormatter>,
    selection: &'a mut Id,
    columns: usize,
    order: EntityOrder,
    cache_id: Option<egui::Id>,
}

impl<'a, B> EntityInspectorWidget<'a, B>
where
    B: BlobStoreGet<Blake3>,
{
    pub fn new(
        data: &'a TribleSet,
        metadata: &'a TribleSet,
        name_cache: &'a BlobCache<B, Blake3, LongString, View<str>>,
        formatter_cache: &'a BlobCache<B, Blake3, WasmCode, WasmValueFormatter>,
        selection: &'a mut Id,
    ) -> Self {
        #[cfg(feature = "cubecl")]
        let order = EntityOrder::Anneal;
        #[cfg(not(feature = "cubecl"))]
        let order = EntityOrder::Id;
        Self {
            data,
            metadata,
            name_cache,
            formatter_cache,
            selection,
            columns: 0,
            order,
            cache_id: None,
        }
    }

    pub fn columns(mut self, columns: usize) -> Self {
        self.columns = columns;
        self
    }

    pub fn order(mut self, order: EntityOrder) -> Self {
        self.order = order;
        self
    }

    pub fn cache_id(mut self, cache_id: egui::Id) -> Self {
        self.cache_id = Some(cache_id);
        self
    }

    pub fn show(self, ui: &mut Ui) -> EntityInspectorResponse {
        let data_fingerprint = self.data.fingerprint();
        let metadata_fingerprint = self.metadata.fingerprint();
        let cache_id = self.cache_id.unwrap_or_else(|| {
            ui.id()
                .with("entity_inspector_graph")
                .with(data_fingerprint)
                .with(metadata_fingerprint)
        });
        let graph = {
            #[cfg(feature = "telemetry")]
            let _graph_span = tracing::info_span!("entity_inspector_graph").entered();
            cached_entity_graph(
                ui,
                cache_id,
                self.data,
                self.metadata,
                self.name_cache,
                self.formatter_cache,
            )
        };
        let selection_before = *self.selection;
        if let Some(first) = graph.nodes.first().map(|node| node.id) {
            if !graph.id_to_index.contains_key(self.selection) {
                *self.selection = first;
            }
        }
        let (layout, routed_edges, stats) = {
            #[cfg(feature = "telemetry")]
            let _layout_span = tracing::info_span!("entity_inspector_layout").entered();
            compute_inspector(ui, cache_id, graph.as_ref(), self.columns, self.order)
        };
        let response = {
            #[cfg(feature = "telemetry")]
            let _paint_span = tracing::info_span!("entity_inspector_paint").entered();
            paint_entity_inspector(ui, graph.as_ref(), self.selection, &layout, &routed_edges)
        };
        let selection_changed = *self.selection != selection_before;
        EntityInspectorResponse {
            response,
            stats,
            selection_changed,
        }
    }
}

#[derive(Clone, Debug)]
struct RoutedEdge {
    points: Vec<egui::Pos2>,
    length: f32,
    turns: usize,
    span_cols: usize,
    start_underline: Option<(egui::Pos2, egui::Pos2)>,
    attr_id: Id,
    from_entity: usize,
    to_entity: usize,
    go_left: bool,
    used_fallback_track: bool,
}

#[derive(Clone, Debug)]
struct EdgeRender {
    points: Vec<egui::Pos2>,
    line_color: egui::Color32,
    start_underline: Option<(egui::Pos2, egui::Pos2)>,
    from_entity: usize,
    to_entity: usize,
}

#[derive(Clone, Debug)]
struct EdgeDraft {
    edge: EntityEdge,
    source_rect: Rect,
    go_left: bool,
    start: egui::Pos2,
    end: egui::Pos2,
    min_col: usize,
    max_col: usize,
    start_boundary: i32,
    end_boundary: i32,
    start_gutter_center_x: f32,
    end_gutter_center_x: f32,
    component: usize,
}

type BundleKey = (Id, usize);

fn attribute_palette_index(attr: Id, palette_len: usize) -> usize {
    if palette_len == 0 {
        return 0;
    }
    let raw: [u8; 16] = *attr.as_ref();
    let val = u128::from_le_bytes(raw);
    let hash = (val ^ (val >> 64)) as u64;
    (hash as usize) % palette_len
}

fn entity_order(
    ui: &mut Ui,
    cache_id: egui::Id,
    graph: &EntityGraph,
    order: EntityOrder,
) -> Vec<usize> {
    #[cfg(not(feature = "cubecl"))]
    let _ = (ui, cache_id);
    match order {
        EntityOrder::Id => (0..graph.nodes.len()).collect(),
        #[cfg(feature = "cubecl")]
        EntityOrder::Anneal => {
            gpu_sa_order(ui, cache_id, graph).unwrap_or_else(|| (0..graph.nodes.len()).collect())
        }
    }
}

#[cfg(feature = "cubecl")]
#[derive(Clone)]
struct GpuSaProblem {
    node_count: usize,
    edges: Vec<(usize, usize)>,
    edges_flat: Vec<u32>,
    adj_offsets: Vec<u32>,
    adj_list: Vec<u32>,
}

#[cfg(feature = "cubecl")]
impl GpuSaProblem {
    fn from_graph(graph: &EntityGraph) -> Self {
        let node_count = graph.nodes.len();
        let edges: Vec<(usize, usize)> = graph
            .edges
            .iter()
            .map(|edge| (edge.from_entity, edge.to_entity))
            .collect();
        let mut edges_flat = Vec::with_capacity(edges.len() * 2);
        for (u, v) in &edges {
            edges_flat.push(*u as u32);
            edges_flat.push(*v as u32);
        }

        let mut degrees = vec![0usize; node_count];
        for &(u, v) in &edges {
            degrees[u] += 1;
            degrees[v] += 1;
        }
        let mut adj_offsets = Vec::with_capacity(node_count + 1);
        adj_offsets.push(0u32);
        for i in 0..node_count {
            adj_offsets.push(adj_offsets[i] + degrees[i] as u32);
        }
        let total_adj = *adj_offsets.last().unwrap_or(&0) as usize;
        let mut adj_list = vec![0u32; total_adj];
        let mut cursor: Vec<usize> = adj_offsets.iter().map(|&offset| offset as usize).collect();
        for &(u, v) in &edges {
            let idx = cursor[u];
            adj_list[idx] = v as u32;
            cursor[u] += 1;
            let idx = cursor[v];
            adj_list[idx] = u as u32;
            cursor[v] += 1;
        }

        Self {
            node_count,
            edges,
            edges_flat,
            adj_offsets,
            adj_list,
        }
    }
}

#[cfg(feature = "cubecl")]
#[derive(Clone, Default)]
struct GpuSaCache {
    graph_ptr: usize,
    order: Option<Vec<usize>>,
    best_cost: Option<u32>,
    receiver: Option<Arc<Mutex<mpsc::Receiver<GpuSaUpdate>>>>,
    skipped: bool,
}

#[cfg(feature = "cubecl")]
#[derive(Clone, Debug)]
struct GpuSaUpdate {
    order: Vec<usize>,
}

#[cfg(feature = "cubecl")]
const SA_DEFAULT_STEPS: u32 = 1000;
#[cfg(feature = "cubecl")]
const SA_MIN_STEPS: u32 = 1;
#[cfg(feature = "cubecl")]
const SA_MAX_STEPS: u32 = 20_000;
#[cfg(feature = "cubecl")]
const SA_TARGET_MS: u32 = 60;
#[cfg(feature = "cubecl")]
const SA_MAX_TOTAL_NODES: usize = 200_000;
#[cfg(feature = "cubecl")]
const SA_MAX_BATCH_SIZE: usize = 256;
#[cfg(feature = "cubecl")]
const SA_DEFAULT_TARGET_ACCEPTANCE: f32 = 0.3;
#[cfg(feature = "cubecl")]
const SA_DEFAULT_COOLING_ADJUST: f32 = 0.002;
#[cfg(feature = "cubecl")]
const SA_RESEED_PLATEAU_STEPS: u32 = 12_000;
#[cfg(feature = "cubecl")]
const SA_STOP_AFTER_PLATEAU_MS: u64 = 10_000;
#[cfg(feature = "cubecl")]
const SA_FLOOR_FRACTION: f32 = 0.25;
#[cfg(feature = "cubecl")]
const SEED_MIX: u32 = 0x9E37_79B9;
#[cfg(feature = "cubecl")]
const LCG_A: u32 = 1_664_525;
#[cfg(feature = "cubecl")]
const LCG_C: u32 = 1_013_904_223;
#[cfg(feature = "cubecl")]
const INV_U32_MAX_PLUS1: f32 = 1.0 / 4_294_967_296.0;
#[cfg(feature = "cubecl")]
const MIN_ANNEAL_TEMP: f32 = 0.001;

#[cfg(feature = "cubecl")]
#[derive(Clone, Copy, Debug)]
struct LcgRng {
    state: u64,
}

#[cfg(feature = "cubecl")]
impl LcgRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        (self.state >> 32) as u32
    }

    fn gen_range(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        (self.next_u32() as usize) % upper
    }
}

#[cfg(feature = "cubecl")]
fn seed_to_u32(seed: u64) -> u32 {
    let low = seed as u32;
    let high = (seed >> 32) as u32;
    low ^ high.wrapping_mul(SEED_MIX)
}

#[cfg(feature = "cubecl")]
fn sa_batch_size(node_count: usize) -> usize {
    let node_count = node_count.max(1);
    let max_chains = SA_MAX_TOTAL_NODES / node_count;
    max_chains.clamp(1, SA_MAX_BATCH_SIZE)
}

#[cfg(feature = "cubecl")]
fn adjust_steps_per_batch(current: u32, elapsed_ms: u128, target_ms: u32) -> u32 {
    if elapsed_ms == 0 {
        return current.saturating_mul(2).clamp(SA_MIN_STEPS, SA_MAX_STEPS);
    }
    let target = target_ms.max(1) as f64;
    let elapsed = elapsed_ms as f64;
    let ratio = target / elapsed;
    if (0.9..=1.1).contains(&ratio) {
        return current.clamp(SA_MIN_STEPS, SA_MAX_STEPS);
    }
    let factor = ratio.clamp(0.5, 2.0);
    let next = (current as f64 * factor).round() as u32;
    next.clamp(SA_MIN_STEPS, SA_MAX_STEPS)
}

#[cfg(feature = "cubecl")]
fn cost_cpu(order: &[usize], problem: &GpuSaProblem) -> u32 {
    let mut positions = vec![0u32; problem.node_count.max(1)];
    for (pos, &node) in order.iter().take(problem.node_count).enumerate() {
        positions[node] = pos as u32;
    }

    let mut cost = 0u32;
    for &(u, v) in &problem.edges {
        let pu = positions[u];
        let pv = positions[v];
        cost += if pu > pv { pu - pv } else { pv - pu };
    }
    cost
}

#[cfg(feature = "cubecl")]
fn order_cost(graph: &EntityGraph, order: &[usize]) -> u32 {
    let mut positions = vec![0u32; graph.nodes.len().max(1)];
    for (pos, &node) in order.iter().take(graph.nodes.len()).enumerate() {
        positions[node] = pos as u32;
    }

    let mut cost = 0u32;
    for edge in &graph.edges {
        let pu = positions[edge.from_entity];
        let pv = positions[edge.to_entity];
        cost += if pu > pv { pu - pv } else { pv - pu };
    }
    cost
}

#[cfg(feature = "cubecl")]
fn order_cost_checked(graph: &EntityGraph, order: &[usize]) -> Option<u32> {
    let node_count = graph.nodes.len();
    if order.len() != node_count {
        return None;
    }

    let mut positions = vec![u32::MAX; node_count.max(1)];
    for (pos, &node) in order.iter().enumerate() {
        if node >= node_count {
            return None;
        }
        if positions[node] != u32::MAX {
            return None;
        }
        positions[node] = pos as u32;
    }

    let mut cost = 0u32;
    for edge in &graph.edges {
        let pu = positions[edge.from_entity];
        let pv = positions[edge.to_entity];
        cost += if pu > pv { pu - pv } else { pv - pu };
    }
    Some(cost)
}

#[cfg(feature = "cubecl")]
fn estimate_initial_temp(problem: &GpuSaProblem, seed: u64, target_acceptance: f32) -> f32 {
    let node_count = problem.node_count.max(2);
    let mut order: Vec<usize> = (0..node_count).collect();
    let base_cost = cost_cpu(&order, problem);
    let mut rng = LcgRng::new(seed ^ 0x9E37_79B9_7F4A_7C15);
    let sample_count = node_count.min(64).max(8);
    let mut sum_delta = 0u64;
    let mut count = 0u32;

    for _ in 0..sample_count {
        let i = rng.gen_range(node_count);
        let mut j = rng.gen_range(node_count - 1);
        if j >= i {
            j += 1;
        }
        order.swap(i, j);
        let candidate_cost = cost_cpu(&order, problem);
        order.swap(i, j);
        if candidate_cost > base_cost {
            sum_delta += (candidate_cost - base_cost) as u64;
            count += 1;
        }
    }

    let avg_delta = if count > 0 {
        sum_delta as f32 / count as f32
    } else {
        1.0
    };
    let target = target_acceptance.clamp(0.05, 0.95);
    let denom = -target.ln();
    let temp = if denom > 0.0 {
        avg_delta / denom
    } else {
        avg_delta
    };
    temp.clamp(0.1, 100_000.0)
}

#[cfg(feature = "cubecl")]
fn sa_seed(problem: &GpuSaProblem) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    problem.node_count.hash(&mut hasher);
    problem.edges.len().hash(&mut hasher);
    for edge in problem.edges.iter().take(1024) {
        edge.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(feature = "cubecl")]
struct GpuSaConfig {
    seed: u64,
    initial_temp: f32,
    temp_floor: f32,
    cooling_adjust: f32,
    reseed_plateau_steps: u32,
}

#[cfg(feature = "cubecl")]
struct GpuSaBatch {
    best_cost: u32,
    best_index: usize,
    elapsed_ms: u128,
}

#[cfg(feature = "cubecl")]
struct GpuSaRunnerState<R: Runtime> {
    _device: R::Device,
    client: ComputeClient<R>,
    _edges_handle: CubeHandle,
    adj_offsets_handle: CubeHandle,
    adj_list_handle: CubeHandle,
    adj_list_len: usize,
    node_count: usize,
    batch_size: usize,
    best_cost: u32,
    best_version: u32,
    orders_handle: CubeHandle,
    positions_handle: CubeHandle,
    best_orders_handle: CubeHandle,
    current_costs_handle: CubeHandle,
    best_orders_costs_handle: CubeHandle,
    temp_floor_handle: CubeHandle,
    rng_states_handle: CubeHandle,
    stagnant_steps_handle: CubeHandle,
    reseeded_handle: CubeHandle,
    seed_versions_handle: CubeHandle,
}

#[cfg(feature = "cubecl")]
impl<R: Runtime> GpuSaRunnerState<R> {
    fn new(
        device: R::Device,
        problem: &GpuSaProblem,
        batch_size: usize,
        config: &GpuSaConfig,
    ) -> Result<Self, String> {
        if batch_size == 0 {
            return Err("batch size must be > 0".to_string());
        }
        if problem.node_count == 0 {
            return Err("graph must have at least one node".to_string());
        }

        let client = R::client(&device);
        let edges_handle = client.create_from_slice(u32::as_bytes(&problem.edges_flat));
        let adj_offsets_handle = client.create_from_slice(u32::as_bytes(&problem.adj_offsets));
        let adj_list_handle = client.create_from_slice(u32::as_bytes(&problem.adj_list));

        let order_len = match batch_size.checked_mul(problem.node_count) {
            Some(len) if len > 0 => len,
            _ => return Err("batch too large for graph size".to_string()),
        };
        let bytes_u32 = std::mem::size_of::<u32>();
        let bytes_f32 = std::mem::size_of::<f32>();
        let order_bytes = match order_len.checked_mul(bytes_u32) {
            Some(bytes) => bytes,
            None => return Err("order buffer too large".to_string()),
        };
        let batch_bytes_u32 = match batch_size.checked_mul(bytes_u32) {
            Some(bytes) => bytes,
            None => return Err("batch buffer too large".to_string()),
        };
        let batch_bytes_f32 = match batch_size.checked_mul(bytes_f32) {
            Some(bytes) => bytes,
            None => return Err("batch buffer too large".to_string()),
        };

        let orders_handle = client.empty(order_bytes);
        let positions_handle = client.empty(order_bytes);
        let best_orders_handle = client.empty(order_bytes);
        let current_costs_handle = client.empty(batch_bytes_u32);
        let best_orders_costs_handle = client.empty(batch_bytes_u32);
        let temp_floor_handle = client.empty(batch_bytes_f32);
        let rng_states_handle = client.empty(batch_bytes_u32);
        let stagnant_steps_handle = client.empty(batch_bytes_u32);
        let reseeded_handle = client.empty(batch_bytes_u32);
        let seed_versions_handle = client.empty(batch_bytes_u32);

        let seed32 = seed_to_u32(config.seed);
        let initial_temp = config.initial_temp.max(MIN_ANNEAL_TEMP);
        let floor_temp = config.temp_floor.max(MIN_ANNEAL_TEMP).min(initial_temp);

        unsafe {
            minla_sa_init_kernel::launch::<R>(
                &client,
                CubeCount::new_1d(batch_size as u32),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts::<u32>(&edges_handle, problem.edges_flat.len(), 1),
                ScalarArg::new(problem.node_count as u32),
                ScalarArg::new(problem.edges.len() as u32),
                ScalarArg::new(seed32),
                ScalarArg::new(initial_temp),
                ScalarArg::new(floor_temp),
                ArrayArg::from_raw_parts::<u32>(&orders_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(&positions_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(&best_orders_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(&current_costs_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&best_orders_costs_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<f32>(&temp_floor_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&rng_states_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&stagnant_steps_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&seed_versions_handle, batch_size, 1),
            )
            .expect("minla SA init kernel launch");
        }

        Ok(Self {
            _device: device,
            client,
            _edges_handle: edges_handle,
            adj_offsets_handle,
            adj_list_handle,
            adj_list_len: problem.adj_list.len(),
            node_count: problem.node_count,
            batch_size,
            best_cost: u32::MAX,
            best_version: 0,
            orders_handle,
            positions_handle,
            best_orders_handle,
            current_costs_handle,
            best_orders_costs_handle,
            temp_floor_handle,
            rng_states_handle,
            stagnant_steps_handle,
            reseeded_handle,
            seed_versions_handle,
        })
    }

    fn run_steps(&mut self, steps: u32, config: &GpuSaConfig) -> Result<GpuSaBatch, String> {
        let start = Instant::now();
        let reheat_steps = config.reseed_plateau_steps.max(1);
        let order_len = self.batch_size * self.node_count;

        unsafe {
            minla_sa_kernel::launch::<R>(
                &self.client,
                CubeCount::new_1d(self.batch_size as u32),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts::<u32>(&self.adj_offsets_handle, self.node_count + 1, 1),
                ArrayArg::from_raw_parts::<u32>(&self.adj_list_handle, self.adj_list_len, 1),
                ScalarArg::new(self.node_count as u32),
                ScalarArg::new(steps),
                ScalarArg::new(config.cooling_adjust),
                ScalarArg::new(reheat_steps),
                ArrayArg::from_raw_parts::<u32>(&self.orders_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(&self.positions_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(&self.best_orders_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(&self.current_costs_handle, self.batch_size, 1),
                ArrayArg::from_raw_parts::<f32>(&self.temp_floor_handle, self.batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&self.best_orders_costs_handle, self.batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&self.rng_states_handle, self.batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&self.stagnant_steps_handle, self.batch_size, 1),
            )
            .expect("minla SA kernel launch");
        }

        let costs_bytes = self.client.read_one(self.best_orders_costs_handle.clone());
        let costs = u32::from_bytes(&costs_bytes);
        let mut best_cost = u32::MAX;
        let mut best_index = 0usize;
        for (idx, cost) in costs.iter().enumerate() {
            if *cost < best_cost {
                best_cost = *cost;
                best_index = idx;
            }
        }

        if best_cost != u32::MAX && best_cost < self.best_cost {
            self.best_cost = best_cost;
            self.best_version = self.best_version.saturating_add(1);
        }

        let reset_cap = config.initial_temp.max(MIN_ANNEAL_TEMP);
        let reset_floor = config.temp_floor.max(MIN_ANNEAL_TEMP).min(reset_cap);
        let stale_steps = if self.batch_size > 1 && best_cost != u32::MAX && self.node_count > 0 {
            reheat_steps
        } else {
            0
        };

        unsafe {
            minla_sa_reseed_kernel::launch::<R>(
                &self.client,
                CubeCount::new_1d(self.batch_size as u32),
                CubeDim::new_1d(1),
                ScalarArg::new(self.node_count as u32),
                ScalarArg::new(best_index as u32),
                ScalarArg::new(best_cost),
                ScalarArg::new(stale_steps),
                ScalarArg::new(self.best_version),
                ScalarArg::new(reset_floor),
                ArrayArg::from_raw_parts::<u32>(&self.orders_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(&self.positions_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(&self.best_orders_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(&self.current_costs_handle, self.batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&self.best_orders_costs_handle, self.batch_size, 1),
                ArrayArg::from_raw_parts::<f32>(&self.temp_floor_handle, self.batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&self.stagnant_steps_handle, self.batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&self.reseeded_handle, self.batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&self.seed_versions_handle, self.batch_size, 1),
            )
            .expect("minla SA reseed kernel launch");
        }

        Ok(GpuSaBatch {
            best_cost,
            best_index,
            elapsed_ms: start.elapsed().as_millis(),
        })
    }

    fn read_best_order(&self, chain_index: usize) -> Vec<usize> {
        let order_stride = self.node_count * std::mem::size_of::<u32>();
        let offset_start = (chain_index * order_stride) as u64;
        let offset_end = (self.batch_size.saturating_sub(chain_index + 1) * order_stride) as u64;
        let handle = self
            .best_orders_handle
            .clone()
            .offset_start(offset_start)
            .offset_end(offset_end);
        let order_bytes = self.client.read_one(handle);
        let order = u32::from_bytes(&order_bytes);
        order
            .iter()
            .take(self.node_count)
            .map(|&val| val as usize)
            .collect()
    }
}

#[cfg(feature = "cubecl")]
fn gpu_sa_order(ui: &mut Ui, cache_id: egui::Id, graph: &EntityGraph) -> Option<Vec<usize>> {
    if graph.nodes.is_empty() {
        return Some(Vec::new());
    }

    let graph_ptr = graph as *const _ as usize;
    let mut order_out = None;
    let mut repaint = false;

    ui.data_mut(|memory| {
        let cache_id = cache_id.with("gpu_sa_order");
        let cache = memory.get_temp_mut_or_default::<GpuSaCache>(cache_id);
        if cache.graph_ptr != graph_ptr {
            cache.graph_ptr = graph_ptr;
            cache.order = None;
            cache.best_cost = None;
            cache.receiver = None;
            cache.skipped = false;
        }

        if let Some(receiver) = cache.receiver.take() {
            let mut latest = None;
            let mut disconnected = false;
            match receiver.lock() {
                Ok(guard) => loop {
                    match guard.try_recv() {
                        Ok(update) => latest = Some(update),
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                },
                Err(_) => {
                    disconnected = true;
                }
            }

            if let Some(update) = latest {
                if let Some(candidate_cost) = order_cost_checked(graph, &update.order) {
                    let improved = cache
                        .best_cost
                        .map(|best| candidate_cost < best)
                        .unwrap_or(true);
                    if improved {
                        cache.best_cost = Some(candidate_cost);
                        cache.order = Some(update.order);
                        repaint = true;
                    }
                }
            }

            if disconnected {
                cache.receiver = None;
                cache.skipped = cache.order.is_none();
            } else {
                cache.receiver = Some(receiver);
            }
        }

        if cache.order.is_none() && cache.receiver.is_none() && !cache.skipped {
            let baseline: Vec<usize> = (0..graph.nodes.len()).collect();
            cache.best_cost = Some(order_cost(graph, &baseline));
            cache.order = Some(baseline);

            if !graph.edges.is_empty() {
                let problem = GpuSaProblem::from_graph(graph);
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    gpu_sa_loop(problem, tx);
                });
                cache.receiver = Some(Arc::new(Mutex::new(rx)));
            }
        }

        order_out = cache.order.clone();
    });

    if repaint {
        ui.ctx().request_repaint();
    }

    order_out
}

#[cfg(feature = "cubecl")]
fn gpu_sa_loop(problem: GpuSaProblem, sender: mpsc::Sender<GpuSaUpdate>) {
    let batch_size = sa_batch_size(problem.node_count);
    let seed = sa_seed(&problem);
    let initial_temp = estimate_initial_temp(&problem, seed, SA_DEFAULT_TARGET_ACCEPTANCE);
    let floor_temp = (initial_temp * SA_FLOOR_FRACTION).max(MIN_ANNEAL_TEMP);
    let config = GpuSaConfig {
        seed,
        initial_temp,
        temp_floor: floor_temp,
        cooling_adjust: SA_DEFAULT_COOLING_ADJUST,
        reseed_plateau_steps: SA_RESEED_PLATEAU_STEPS,
    };

    let device = WgpuDevice::default();
    let mut runner =
        match GpuSaRunnerState::<WgpuRuntime>::new(device, &problem, batch_size, &config) {
            Ok(runner) => runner,
            Err(err) => {
                eprintln!("gpu-sa: init failed: {err}");
                return;
            }
        };

    let mut best_cost = u32::MAX;
    let mut steps = SA_DEFAULT_STEPS;
    let mut plateau_ms = 0u64;

    loop {
        let batch = match runner.run_steps(steps, &config) {
            Ok(batch) => batch,
            Err(err) => {
                eprintln!("gpu-sa: batch failed: {err}");
                break;
            }
        };

        if batch.best_cost < best_cost {
            let order = runner.read_best_order(batch.best_index);
            best_cost = batch.best_cost;
            plateau_ms = 0;
            if sender.send(GpuSaUpdate { order }).is_err() {
                break;
            }
        } else {
            let elapsed_ms = batch.elapsed_ms.min(u128::from(u64::MAX)) as u64;
            plateau_ms = plateau_ms.saturating_add(elapsed_ms);
            if plateau_ms >= SA_STOP_AFTER_PLATEAU_MS {
                break;
            }
        }

        steps = adjust_steps_per_batch(steps, batch.elapsed_ms, SA_TARGET_MS);
    }
}

#[cfg(feature = "cubecl")]
#[cube(launch)]
fn minla_sa_init_kernel(
    edges: &Array<u32>,
    node_count: u32,
    edge_count: u32,
    seed: u32,
    initial_temp: f32,
    floor_temp: f32,
    orders: &mut Array<u32>,
    positions: &mut Array<u32>,
    best_orders: &mut Array<u32>,
    current_costs: &mut Array<u32>,
    best_orders_costs: &mut Array<u32>,
    temp_floors: &mut Array<f32>,
    rng_states: &mut Array<u32>,
    stagnant_steps: &mut Array<u32>,
    seed_versions: &mut Array<u32>,
) {
    let candidate = ABSOLUTE_POS;
    let node_count = node_count as usize;
    let edge_count = edge_count as usize;
    let temp = initial_temp.max(MIN_ANNEAL_TEMP);
    let floor_temp = floor_temp.max(MIN_ANNEAL_TEMP).min(temp);

    if node_count == 0 {
        current_costs[candidate] = 0;
        best_orders_costs[candidate] = 0;
        temp_floors[candidate] = floor_temp;
        rng_states[candidate] = seed;
        stagnant_steps[candidate] = 0;
        seed_versions[candidate] = 0;
    } else {
        let mut state = seed ^ candidate as u32;
        state = state * SEED_MIX;
        let base = candidate * node_count;

        for index in 0..node_count {
            orders[base + index] = index as u32;
        }

        if node_count > 1 {
            for i in 0..node_count {
                state = state * LCG_A + LCG_C;
                let remaining = node_count - i;
                let j = (state % remaining as u32) as usize + i;
                let left = base + i;
                let right = base + j;
                let tmp = orders[left];
                orders[left] = orders[right];
                orders[right] = tmp;
            }
        }

        for pos in 0..node_count {
            let node = orders[base + pos] as usize;
            positions[base + node] = pos as u32;
        }

        let mut cost = 0u32;
        for edge in 0..edge_count {
            let edge_index = edge * 2;
            let u = edges[edge_index] as usize;
            let v = edges[edge_index + 1] as usize;
            let pu = positions[base + u];
            let pv = positions[base + v];
            let diff = if pu > pv { pu - pv } else { pv - pu };
            cost += diff;
        }

        current_costs[candidate] = cost;
        best_orders_costs[candidate] = cost;
        temp_floors[candidate] = floor_temp;
        rng_states[candidate] = state;
        stagnant_steps[candidate] = 0;
        seed_versions[candidate] = 0;

        for idx in 0..node_count {
            best_orders[base + idx] = orders[base + idx];
        }
    }
}

#[cfg(feature = "cubecl")]
#[cube(launch)]
fn minla_sa_kernel(
    adj_offsets: &Array<u32>,
    adj_list: &Array<u32>,
    node_count: u32,
    steps: u32,
    cooling_adjust: f32,
    reheat_steps: u32,
    orders: &mut Array<u32>,
    positions: &mut Array<u32>,
    best_orders: &mut Array<u32>,
    current_costs: &mut Array<u32>,
    temp_floors: &mut Array<f32>,
    best_orders_costs: &mut Array<u32>,
    rng_states: &mut Array<u32>,
    stagnant_steps: &mut Array<u32>,
) {
    let candidate = ABSOLUTE_POS;
    let node_count = node_count as usize;
    let steps = steps as usize;
    if node_count == 0 {
        current_costs[candidate] = 0;
        best_orders_costs[candidate] = 0;
    } else {
        let base = candidate * node_count;
        let mut state = rng_states[candidate];
        let mut current_cost = current_costs[candidate];
        let mut temp_floor = temp_floors[candidate].max(MIN_ANNEAL_TEMP);
        let cooling_adjust = cooling_adjust.max(0.0001).min(0.05);
        let reheat_gain = (cooling_adjust * 4.0).max(0.001).min(0.05);
        let floor_decay = (1.0 - cooling_adjust * 2.0).max(0.90).min(0.9999);
        let mut stagnant = stagnant_steps[candidate];
        let mut best_cost = best_orders_costs[candidate];
        let reheat_steps = reheat_steps.max(1);

        if node_count > 1 && steps > 0 {
            for _ in 0..steps {
                state = state * LCG_A + LCG_C;
                let i = (state % node_count as u32) as usize;
                state = state * LCG_A + LCG_C;
                let mut j = (state % (node_count as u32 - 1)) as usize;
                if j >= i {
                    j += 1;
                }

                let left = base + i;
                let right = base + j;
                let node_i = orders[left];
                let node_j = orders[right];
                orders[left] = node_j;
                orders[right] = node_i;
                positions[base + node_j as usize] = i as u32;
                positions[base + node_i as usize] = j as u32;

                let node_i_usize = node_i as usize;
                let node_j_usize = node_j as usize;
                let pos_i = i as u32;
                let pos_j = j as u32;
                let mut delta_cost: i32 = 0;

                let start_i = adj_offsets[node_i_usize] as usize;
                let end_i = adj_offsets[node_i_usize + 1] as usize;
                for idx in start_i..end_i {
                    let neighbor = adj_list[idx] as usize;
                    if neighbor != node_j_usize {
                        let pos_n = positions[base + neighbor];
                        let old = if pos_i > pos_n {
                            pos_i - pos_n
                        } else {
                            pos_n - pos_i
                        };
                        let new = if pos_j > pos_n {
                            pos_j - pos_n
                        } else {
                            pos_n - pos_j
                        };
                        delta_cost += new as i32 - old as i32;
                    }
                }

                let start_j = adj_offsets[node_j_usize] as usize;
                let end_j = adj_offsets[node_j_usize + 1] as usize;
                for idx in start_j..end_j {
                    let neighbor = adj_list[idx] as usize;
                    if neighbor != node_i_usize {
                        let pos_n = positions[base + neighbor];
                        let old = if pos_j > pos_n {
                            pos_j - pos_n
                        } else {
                            pos_n - pos_j
                        };
                        let new = if pos_i > pos_n {
                            pos_i - pos_n
                        } else {
                            pos_n - pos_i
                        };
                        delta_cost += new as i32 - old as i32;
                    }
                }

                let candidate_cost = (current_cost as i32 + delta_cost).max(0) as u32;

                let delta = candidate_cost as f32 - current_cost as f32;
                let mut accept = delta <= 0.0;
                if !accept && temp_floor > MIN_ANNEAL_TEMP {
                    let probability = (-delta / temp_floor).exp();
                    state = state * LCG_A + LCG_C;
                    let rand = state as f32 * INV_U32_MAX_PLUS1;
                    accept = rand < probability;
                }

                if accept {
                    current_cost = candidate_cost;
                    if candidate_cost < best_cost {
                        best_cost = candidate_cost;
                        stagnant = 0;
                        for idx in 0..node_count {
                            best_orders[base + idx] = orders[base + idx];
                        }
                        temp_floor = (temp_floor * floor_decay).max(MIN_ANNEAL_TEMP);
                    } else if stagnant < u32::MAX {
                        stagnant += 1;
                    }
                } else {
                    let left = base + i;
                    let right = base + j;
                    orders[left] = node_i;
                    orders[right] = node_j;
                    positions[base + node_i as usize] = i as u32;
                    positions[base + node_j as usize] = j as u32;
                    if stagnant < u32::MAX {
                        stagnant += 1;
                    }
                }
                if stagnant > 0 {
                    let ratio = stagnant as f32 / reheat_steps as f32;
                    let gain = reheat_gain * (ratio / (1.0 + ratio));
                    temp_floor = temp_floor * (1.0 + gain);
                }
            }
        }

        current_costs[candidate] = current_cost;
        temp_floors[candidate] = temp_floor;
        best_orders_costs[candidate] = best_cost;
        rng_states[candidate] = state;
        stagnant_steps[candidate] = stagnant;
    }
}

#[cfg(feature = "cubecl")]
#[cube(launch)]
fn minla_sa_reseed_kernel(
    node_count: u32,
    best_idx: u32,
    best_cost: u32,
    stale_steps: u32,
    best_version: u32,
    reset_floor: f32,
    orders: &mut Array<u32>,
    positions: &mut Array<u32>,
    best_orders: &mut Array<u32>,
    current_costs: &mut Array<u32>,
    best_orders_costs: &mut Array<u32>,
    temp_floors: &mut Array<f32>,
    stagnant_steps: &mut Array<u32>,
    reseeded_flags: &mut Array<u32>,
    seed_versions: &mut Array<u32>,
) {
    let candidate = ABSOLUTE_POS;
    let node_count = node_count as usize;
    let best_idx = best_idx as usize;
    if candidate == best_idx {
        seed_versions[candidate] = best_version;
    }
    let should_reseed = stale_steps > 0
        && candidate != best_idx
        && stagnant_steps[candidate] >= stale_steps
        && seed_versions[candidate] < best_version;
    reseeded_flags[candidate] = 0;
    if should_reseed {
        reseeded_flags[candidate] = 1;
    }
    if should_reseed {
        if node_count == 0 {
            current_costs[candidate] = best_cost;
            best_orders_costs[candidate] = best_cost;
        } else {
            let base = candidate * node_count;
            let best_base = best_idx * node_count;
            for idx in 0..node_count {
                let node = best_orders[best_base + idx];
                orders[base + idx] = node;
                best_orders[base + idx] = node;
                positions[base + node as usize] = idx as u32;
            }
            current_costs[candidate] = best_cost;
            best_orders_costs[candidate] = best_cost;
        }
        temp_floors[candidate] = reset_floor;
        stagnant_steps[candidate] = 0;
        seed_versions[candidate] = best_version;
    }
}

fn choose_track_y(gap_tracks: &[(f32, f32)], start_y: f32, end_y: f32) -> f32 {
    let mut best = (gap_tracks[0].0 + gap_tracks[0].1) * 0.5;
    let mut best_cost = f32::INFINITY;
    for (top, bottom) in gap_tracks {
        let y = (top + bottom) * 0.5;
        let cost = (start_y - y).abs() + (end_y - y).abs();
        if cost < best_cost {
            best_cost = cost;
            best = y;
        }
    }
    best
}

fn choose_track_y_monotonic(corridors: &[(f32, f32)], current_y: f32, end_y: f32) -> (f32, bool) {
    if corridors.is_empty() {
        return (current_y, true);
    }

    let going_down = end_y >= current_y;
    let mut best = None;
    let mut best_delta = f32::INFINITY;

    if going_down {
        for (top, bottom) in corridors {
            if *bottom < current_y || *top > end_y {
                continue;
            }
            let y = current_y.max(*top).min(*bottom);
            let delta = y - current_y;
            if delta < best_delta {
                best_delta = delta;
                best = Some(y);
                if delta <= f32::EPSILON {
                    break;
                }
            }
        }
    } else {
        for (top, bottom) in corridors {
            if *top > current_y || *bottom < end_y {
                continue;
            }
            let y = current_y.min(*bottom).max(*top);
            let delta = current_y - y;
            if delta < best_delta {
                best_delta = delta;
                best = Some(y);
                if delta <= f32::EPSILON {
                    break;
                }
            }
        }
    }

    if let Some(y) = best {
        return (y, false);
    }

    best = None;
    best_delta = f32::INFINITY;
    if going_down {
        for (top, bottom) in corridors {
            if *bottom < current_y {
                continue;
            }
            let y = current_y.max(*top).min(*bottom);
            let delta = (y - current_y).abs();
            if delta < best_delta {
                best_delta = delta;
                best = Some(y);
            }
        }
    } else {
        for (top, bottom) in corridors {
            if *top > current_y {
                continue;
            }
            let y = current_y.min(*bottom).max(*top);
            let delta = (current_y - y).abs();
            if delta < best_delta {
                best_delta = delta;
                best = Some(y);
            }
        }
    }

    if let Some(y) = best {
        return (y, true);
    }

    (choose_track_y(corridors, current_y, end_y), true)
}

fn intersect_intervals(a: &[(f32, f32)], b: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;

    while i < a.len() && j < b.len() {
        let start = a[i].0.max(b[j].0);
        let end = a[i].1.min(b[j].1);
        if end > start {
            out.push((start, end));
        }

        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }

    out
}

fn closest_corner_on_side(target: Rect, from: egui::Pos2, left: bool) -> egui::Pos2 {
    let top = if left {
        target.left_top()
    } else {
        target.right_top()
    };
    let bottom = if left {
        target.left_bottom()
    } else {
        target.right_bottom()
    };
    if from.distance_sq(top) <= from.distance_sq(bottom) {
        top
    } else {
        bottom
    }
}

fn choose_track_y_between_columns(
    component: &ComponentLayout,
    start_y: f32,
    end_y: f32,
    min_col: usize,
    max_col: usize,
) -> (f32, bool) {
    let Some(first) = component.column_free.get(min_col) else {
        return ((start_y + end_y) * 0.5, true);
    };

    let mut corridors = first.clone();
    for col in (min_col + 1)..=max_col {
        let Some(next) = component.column_free.get(col) else {
            break;
        };
        corridors = intersect_intervals(&corridors, next);
        if corridors.is_empty() {
            break;
        }
    }

    if corridors.is_empty() {
        return (choose_track_y(first, start_y, end_y), true);
    }

    let going_down = end_y >= start_y;
    if going_down {
        for (top, bottom) in &corridors {
            if *bottom < start_y {
                continue;
            }
            if *top > end_y {
                break;
            }
            let y = start_y.max(*top).min(end_y);
            return (y, false);
        }
    } else {
        for (top, bottom) in corridors.iter().rev() {
            if *top > start_y {
                continue;
            }
            if *bottom < end_y {
                break;
            }
            let y = start_y.min(*bottom).max(end_y);
            return (y, false);
        }
    }

    if going_down {
        for (top, bottom) in &corridors {
            if *bottom >= start_y {
                return (start_y.max(*top), true);
            }
        }
    } else {
        for (top, bottom) in corridors.iter().rev() {
            if *top <= start_y {
                return (start_y.min(*bottom), true);
            }
        }
    }

    (choose_track_y(&corridors, start_y, end_y), true)
}

fn row_line_y(layout: &GraphLayout, tile: Rect, row: usize) -> f32 {
    let row_top = tile.top()
        + layout.tile_padding
        + layout.header_height
        + row as f32 * layout.text_row_height;
    let y = row_top + layout.text_row_height - 2.0 - 1.0;
    y.max(row_top + 2.0)
}

fn row_anchor(layout: &GraphLayout, tile: Rect, row: usize, on_left: bool) -> egui::Pos2 {
    let y = row_line_y(layout, tile, row);
    let edge_inset = 0.0;
    let x = if on_left {
        tile.left() + edge_inset
    } else {
        tile.right() - edge_inset
    };
    pos2(x, y)
}

fn row_underline_segment(
    layout: &GraphLayout,
    tile: Rect,
    row: usize,
    go_left: bool,
) -> Option<(egui::Pos2, egui::Pos2)> {
    let inner = tile.shrink(layout.tile_padding);
    if !inner.is_positive() {
        return None;
    }

    let y = row_line_y(layout, tile, row);
    let key_w = (inner.width() * 0.42).clamp(56.0, 120.0);
    let divider_x = (inner.left() + key_w).min(inner.right());
    let inset = 4.0;
    let min_len = 6.0;

    let edge_inset = 0.0;
    let (start_x, end_x) = if go_left {
        let start_x = tile.left() + edge_inset;
        let mut end_x = divider_x - inset;
        if end_x < start_x + min_len {
            end_x = (start_x + min_len).min(inner.right());
        }
        (start_x, end_x)
    } else {
        let end_x = tile.right() - edge_inset;
        let mut start_x = divider_x + inset;
        if start_x > end_x - min_len {
            start_x = (end_x - min_len).max(inner.left());
        }
        (start_x, end_x)
    };

    if end_x - start_x <= 0.5 {
        return None;
    }

    Some((pos2(start_x, y), pos2(end_x, y)))
}

fn build_attribute_bundle_offsets(
    layout: &GraphLayout,
    drafts: &[EdgeDraft],
) -> HashMap<(i32, BundleKey), f32> {
    let max_offset = (layout.column_gap * 0.5 - 4.0).max(0.0);
    let mut keys_by_boundary = HashMap::<i32, Vec<BundleKey>>::new();

    for draft in drafts {
        let key = (draft.edge.attr_id, draft.edge.to_entity);
        let start_keys = keys_by_boundary.entry(draft.start_boundary).or_default();
        if !start_keys.contains(&key) {
            start_keys.push(key);
        }
        let end_keys = keys_by_boundary.entry(draft.end_boundary).or_default();
        if !end_keys.contains(&key) {
            end_keys.push(key);
        }
    }

    let mut offsets = HashMap::new();
    for (boundary, keys) in keys_by_boundary {
        let count = keys.len();
        if count == 0 {
            continue;
        }

        if max_offset <= 0.01 || count == 1 {
            offsets.insert((boundary, keys[0]), 0.0);
            continue;
        }

        let step = (max_offset * 2.0) / (count - 1) as f32;
        for (idx, key) in keys.iter().enumerate() {
            let offset = -max_offset + step * idx as f32;
            offsets.insert((boundary, *key), offset);
        }
    }

    offsets
}

fn route_edges(layout: &GraphLayout, graph: &EntityGraph) -> Vec<RoutedEdge> {
    let mut drafts = Vec::with_capacity(graph.edges.len());

    for edge in graph.edges.iter().cloned() {
        let component = layout
            .node_component
            .get(edge.from_entity)
            .copied()
            .unwrap_or(usize::MAX);
        if component == usize::MAX {
            continue;
        }
        if layout.node_component.get(edge.to_entity).copied() != Some(component) {
            continue;
        }

        let source_rect = layout.tile_rects[edge.from_entity];
        let target_rect = layout.tile_rects[edge.to_entity];
        if !source_rect.is_positive() || !target_rect.is_positive() {
            continue;
        }

        let from_col = layout
            .node_column
            .get(edge.from_entity)
            .copied()
            .unwrap_or(usize::MAX);
        let to_col = layout
            .node_column
            .get(edge.to_entity)
            .copied()
            .unwrap_or(usize::MAX);
        if from_col == usize::MAX || to_col == usize::MAX {
            continue;
        }
        let (min_col, max_col) = if from_col <= to_col {
            (from_col, to_col)
        } else {
            (to_col, from_col)
        };

        let last_col = layout.column_count.saturating_sub(1);
        let same_col = from_col == to_col;
        let go_left = if same_col {
            if last_col == 0 {
                let raw: [u8; 16] = *edge.attr_id.as_ref();
                raw[0] & 1 == 0
            } else if from_col == 0 {
                true
            } else if from_col == last_col {
                false
            } else {
                let raw: [u8; 16] = *edge.attr_id.as_ref();
                raw[0] & 1 == 0
            }
        } else {
            to_col < from_col
        };
        let start = row_anchor(layout, source_rect, edge.from_row, go_left);
        let end_on_left = if same_col { go_left } else { !go_left };
        let end = closest_corner_on_side(target_rect, start, end_on_left);

        let start_boundary = if go_left {
            from_col as i32 - 1
        } else {
            from_col as i32
        };
        let start_gutter_center_x = if go_left {
            source_rect.left() - layout.column_gap * 0.5
        } else {
            source_rect.right() + layout.column_gap * 0.5
        };

        let end_boundary = if end_on_left {
            to_col as i32 - 1
        } else {
            to_col as i32
        };
        let end_gutter_center_x = if end_on_left {
            target_rect.left() - layout.column_gap * 0.5
        } else {
            target_rect.right() + layout.column_gap * 0.5
        };

        drafts.push(EdgeDraft {
            edge,
            source_rect,
            go_left,
            start,
            end,
            min_col,
            max_col,
            start_boundary,
            end_boundary,
            start_gutter_center_x,
            end_gutter_center_x,
            component,
        });
    }

    let bundle_offsets = build_attribute_bundle_offsets(layout, &drafts);
    let mut routed = Vec::with_capacity(drafts.len());

    for draft in drafts {
        let Some(component_layout) = layout.components.get(draft.component) else {
            continue;
        };
        let bundle_key = (draft.edge.attr_id, draft.edge.to_entity);
        let start_offset = bundle_offsets
            .get(&(draft.start_boundary, bundle_key))
            .copied()
            .unwrap_or(0.0);
        let end_offset = bundle_offsets
            .get(&(draft.end_boundary, bundle_key))
            .copied()
            .unwrap_or(start_offset);
        let start_center_x = draft.start_gutter_center_x;
        let end_center_x = draft.end_gutter_center_x;
        let mut start_bundle_x = start_center_x + start_offset;
        let mut end_bundle_x = end_center_x + end_offset;

        let same_gutter = draft.start_boundary == draft.end_boundary;
        let span_cols = draft.max_col.saturating_sub(draft.min_col);
        if !same_gutter {
            if start_center_x <= end_center_x {
                start_bundle_x = start_bundle_x.clamp(start_center_x, end_center_x);
                end_bundle_x = end_bundle_x.clamp(start_bundle_x, end_center_x);
            } else {
                start_bundle_x = start_bundle_x.clamp(end_center_x, start_center_x);
                end_bundle_x = end_bundle_x.clamp(end_center_x, start_bundle_x);
            }
        }
        let mut used_fallback_track = false;
        let mut points = if same_gutter {
            let gutter_x = start_bundle_x;
            vec![
                draft.start,
                pos2(gutter_x, draft.start.y),
                pos2(gutter_x, draft.end.y),
                draft.end,
            ]
        } else if span_cols > 1 {
            let mut points = vec![draft.start, pos2(start_bundle_x, draft.start.y)];
            let step = if draft.end_boundary > draft.start_boundary {
                1
            } else {
                -1
            };
            let step_x = draft.source_rect.width() + layout.column_gap;
            let mut boundary = draft.start_boundary;
            let mut current_y = draft.start.y;
            let mut current_x = start_bundle_x;
            let end_y = draft.end.y;

            while boundary != draft.end_boundary {
                let next_boundary = boundary + step;
                let column_idx = if step > 0 {
                    next_boundary as usize
                } else {
                    boundary as usize
                };
                let next_center_x =
                    start_center_x + (next_boundary - draft.start_boundary) as f32 * step_x;
                let (track_y, fallback) = component_layout
                    .column_free
                    .get(column_idx)
                    .map(|corridors| choose_track_y_monotonic(corridors, current_y, end_y))
                    .unwrap_or((current_y, true));
                used_fallback_track |= fallback;

                let next_offset = bundle_offsets
                    .get(&(next_boundary, bundle_key))
                    .copied()
                    .unwrap_or(0.0);
                let mut next_x = next_center_x + next_offset;
                if step > 0 && next_x < current_x {
                    next_x = current_x;
                } else if step < 0 && next_x > current_x {
                    next_x = current_x;
                }

                points.push(pos2(current_x, track_y));
                points.push(pos2(next_x, track_y));

                boundary = next_boundary;
                current_y = track_y;
                current_x = next_x;
            }

            points.push(pos2(current_x, end_y));
            points.push(draft.end);
            points
        } else {
            let (track_y, used_fallback) = choose_track_y_between_columns(
                component_layout,
                draft.start.y,
                draft.end.y,
                draft.min_col,
                draft.max_col,
            );
            used_fallback_track = used_fallback;
            vec![
                draft.start,
                pos2(start_center_x, draft.start.y),
                pos2(start_center_x, track_y),
                pos2(start_bundle_x, track_y),
                pos2(end_bundle_x, track_y),
                pos2(end_center_x, track_y),
                pos2(end_center_x, draft.end.y),
                draft.end,
            ]
        };
        points.dedup_by(|a, b| a.distance_sq(*b) < 0.01);

        let mut length = 0.0f32;
        for seg in points.windows(2) {
            let a = seg[0];
            let b = seg[1];
            length += (a.x - b.x).abs() + (a.y - b.y).abs();
        }

        let mut turns = 0usize;
        let mut last_dir: Option<bool> = None;
        for seg in points.windows(2) {
            let a = seg[0];
            let b = seg[1];
            let is_horizontal = (a.y - b.y).abs() <= (a.x - b.x).abs();
            if let Some(prev) = last_dir {
                if prev != is_horizontal {
                    turns += 1;
                }
            }
            last_dir = Some(is_horizontal);
        }

        routed.push(RoutedEdge {
            points,
            length,
            turns,
            span_cols: draft.max_col.saturating_sub(draft.min_col),
            start_underline: row_underline_segment(
                layout,
                draft.source_rect,
                draft.edge.from_row,
                draft.go_left,
            ),
            attr_id: draft.edge.attr_id,
            from_entity: draft.edge.from_entity,
            to_entity: draft.edge.to_entity,
            go_left: draft.go_left,
            used_fallback_track,
        });
    }

    routed
}

fn paint_subway_edge(painter: &egui::Painter, points: &[egui::Pos2], line: Stroke) {
    if points.len() < 2 {
        return;
    }
    painter.add(egui::Shape::line(points.to_vec(), line));
}

fn distance_sq_to_segment(point: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let denom = ab.length_sq();
    if denom <= f32::EPSILON {
        return point.distance_sq(a);
    }
    let t = ((point - a).dot(ab) / denom).clamp(0.0, 1.0);
    point.distance_sq(a + ab * t)
}

fn distance_sq_to_polyline(point: egui::Pos2, points: &[egui::Pos2]) -> f32 {
    if points.len() < 2 {
        return f32::INFINITY;
    }
    let mut best = f32::INFINITY;
    for seg in points.windows(2) {
        let d2 = distance_sq_to_segment(point, seg[0], seg[1]);
        if d2 < best {
            best = d2;
        }
    }
    best
}

fn round_polyline(points: &[egui::Pos2], radius: f32, segments: usize) -> Vec<egui::Pos2> {
    if points.len() < 3 || radius <= 0.0 || segments == 0 {
        return points.to_vec();
    }

    let mut out = Vec::with_capacity(points.len() + segments * 2);
    out.push(points[0]);

    for i in 1..points.len() - 1 {
        let prev = points[i - 1];
        let curr = points[i];
        let next = points[i + 1];

        let v_in = curr - prev;
        let v_out = next - curr;
        let len_in = v_in.length();
        let len_out = v_out.length();
        if len_in <= 0.01 || len_out <= 0.01 {
            out.push(curr);
            continue;
        }

        let dir_in = v_in / len_in;
        let dir_out = v_out / len_out;
        if dir_in.dot(dir_out).abs() > 0.999 {
            out.push(curr);
            continue;
        }

        let corner_radius = radius.min(len_in * 0.5).min(len_out * 0.5);
        if corner_radius <= 0.01 {
            out.push(curr);
            continue;
        }

        let p1 = curr - dir_in * corner_radius;
        let p2 = curr + dir_out * corner_radius;
        if i == 1 {
            if out.last().is_none_or(|last| last.distance_sq(p1) > 0.01) {
                out.push(p1);
            }
        } else if let Some(last) = out.last_mut() {
            *last = p1;
        } else {
            out.push(p1);
        }

        let center = curr + (dir_out - dir_in) * corner_radius;
        let a1 = (p1 - center).angle();
        let mut a2 = (p2 - center).angle();
        let cross = dir_in.x * dir_out.y - dir_in.y * dir_out.x;
        if cross > 0.0 {
            if a2 <= a1 {
                a2 += std::f32::consts::TAU;
            }
        } else if a2 >= a1 {
            a2 -= std::f32::consts::TAU;
        }
        let step = (a2 - a1) / segments as f32;
        for s in 1..=segments {
            let angle = a1 + step * s as f32;
            out.push(center + egui::Vec2::angled(angle) * corner_radius);
        }
    }

    out.push(*points.last().unwrap());
    out
}

struct TableInteraction {
    select_target: Option<usize>,
    scroll_target: Option<usize>,
}

fn paint_entity_table(
    ui: &mut Ui,
    rect: Rect,
    node: &EntityNode,
    node_idx: usize,
    is_selected: bool,
    layout: &GraphLayout,
    graph: &EntityGraph,
) -> TableInteraction {
    let id = ui.id().with(("entity_table", node.id));
    let response = ui.interact(rect, id, Sense::click());
    let table_clicked = response.clicked();

    let visuals = ui.visuals();
    let fill = visuals.window_fill;
    let ink = visuals.widgets.noninteractive.fg_stroke.color;
    let stroke = Stroke::new(1.0, ink);
    let grid_stroke = Stroke::new(1.0, ink);
    let hatch_color = visuals.widgets.noninteractive.bg_stroke.color;

    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, fill);
    if is_selected {
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
        let inner_rect = rect.shrink(2.0);
        if inner_rect.is_positive() {
            painter.rect_stroke(inner_rect, 0.0, stroke, egui::StrokeKind::Inside);
        }
    } else {
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
    }

    let text_color = ink;
    let title_font = TextStyle::Monospace.resolve(ui.style());
    let row_font = TextStyle::Small.resolve(ui.style());

    let inner = rect.shrink(layout.tile_padding);
    let title_rect = Rect::from_min_max(
        inner.left_top(),
        pos2(inner.right(), inner.top() + layout.header_height),
    );
    painter.text(
        title_rect.left_top(),
        Align2::LEFT_TOP,
        node.title.as_str(),
        title_font,
        text_color,
    );

    let row_top = title_rect.bottom();
    let key_w = (inner.width() * 0.42).clamp(56.0, 120.0);
    let key_x = inner.left();
    let value_x = (inner.left() + key_w + 8.0).min(inner.right());

    let row_area = Rect::from_min_max(pos2(inner.left(), row_top), inner.right_bottom());
    let divider_x = (inner.left() + key_w).min(inner.right());
    if row_area.is_positive() {
        painter.vline(divider_x, row_area.y_range(), grid_stroke);
    }

    let mut scroll_target = None;
    let mut select_target = None;
    for (i, row) in node.rows.iter().enumerate() {
        let y = row_top + i as f32 * layout.text_row_height;
        let row_rect = Rect::from_min_max(
            pos2(inner.left(), y),
            pos2(
                inner.right(),
                (y + layout.text_row_height).min(inner.bottom()),
            ),
        );
        if i > 0 {
            painter.hline(row_rect.x_range(), row_rect.top(), grid_stroke);
        }

        if let Some(target) = row.target {
            if let Some(&target_idx) = graph.id_to_index.get(&target) {
                let row_id = id.with(("row", i));
                let row_response = ui
                    .interact(row_rect, row_id, Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if row_response.clicked() {
                    scroll_target = Some(target_idx);
                    select_target = Some(target_idx);
                }
            }
        }

        painter.text(
            pos2(key_x, row_rect.top() + 1.0),
            Align2::LEFT_TOP,
            row.attr.as_str(),
            row_font.clone(),
            text_color,
        );
        if row.hatched {
            let value_rect = Rect::from_min_max(
                pos2(value_x, row_rect.top()),
                pos2(inner.right(), row_rect.bottom()),
            );
            let hatch_rect = value_rect.shrink(1.0);
            if hatch_rect.is_positive() {
                paint_hatching(&painter.with_clip_rect(hatch_rect), hatch_rect, hatch_color);
            }
        } else {
            painter.text(
                pos2(value_x, row_rect.top() + 1.0),
                Align2::LEFT_TOP,
                row.value.as_str(),
                row_font.clone(),
                text_color,
            );
        }
    }

    if response.hovered() {
        let full = id_full(node.id);
        let _ = response.on_hover_text(full);
    }

    if select_target.is_none() && table_clicked {
        select_target = Some(node_idx);
    }

    TableInteraction {
        select_target,
        scroll_target,
    }
}

fn compute_inspector(
    ui: &mut Ui,
    cache_id: egui::Id,
    graph: &EntityGraph,
    forced_columns: usize,
    order: EntityOrder,
) -> (GraphLayout, Vec<RoutedEdge>, EntityInspectorStats) {
    let order = entity_order(ui, cache_id, graph, order);
    let mut positions = vec![0usize; graph.nodes.len()];
    for (pos, &idx) in order.iter().enumerate() {
        positions[idx] = pos;
    }
    let mut linear_total = 0.0f32;
    for edge in &graph.edges {
        let from = positions[edge.from_entity] as i32;
        let to = positions[edge.to_entity] as i32;
        linear_total += (from - to).abs() as f32;
    }
    let linear_avg = if graph.edges.is_empty() {
        0.0
    } else {
        linear_total / graph.edges.len() as f32
    };
    let layout = compute_graph_layout(ui, graph, forced_columns, &order);
    let routed_edges = route_edges(&layout, graph);
    let stats = compute_graph_stats(graph, &layout, &routed_edges, linear_total, linear_avg);
    (layout, routed_edges, stats)
}

fn compute_graph_stats(
    graph: &EntityGraph,
    layout: &GraphLayout,
    routed_edges: &[RoutedEdge],
    linear_total: f32,
    linear_avg: f32,
) -> EntityInspectorStats {
    let connected_components = connected_components(&build_adjacency(graph)).len();
    let edges = routed_edges.len();
    let tile_area = layout
        .tile_rects
        .iter()
        .filter_map(|rect| rect.is_positive().then(|| rect.area()))
        .sum::<f32>();
    let canvas_area = layout.canvas_size.x * layout.canvas_size.y;
    let tile_coverage = if canvas_area <= 0.0 {
        0.0
    } else {
        tile_area / canvas_area
    };

    let mut total_edge_len = 0.0f32;
    let mut max_edge_len = 0.0f32;
    let mut total_turns = 0usize;
    let mut max_turns = 0usize;
    let mut total_span_cols = 0usize;
    let mut max_span_cols = 0usize;
    let mut left_edges = 0usize;
    let mut fallback_tracks = 0usize;

    for edge in routed_edges {
        total_edge_len += edge.length;
        max_edge_len = max_edge_len.max(edge.length);
        total_turns += edge.turns;
        max_turns = max_turns.max(edge.turns);
        total_span_cols += edge.span_cols;
        max_span_cols = max_span_cols.max(edge.span_cols);
        if edge.go_left {
            left_edges += 1;
        }
        if edge.used_fallback_track {
            fallback_tracks += 1;
        }
    }

    EntityInspectorStats {
        nodes: graph.nodes.len(),
        edges,
        connected_components,
        columns: layout.column_count,
        canvas_width: layout.canvas_size.x,
        canvas_height: layout.canvas_size.y,
        tile_coverage,
        total_edge_len,
        avg_edge_len: if edges == 0 {
            0.0
        } else {
            total_edge_len / edges as f32
        },
        max_edge_len,
        avg_turns: if edges == 0 {
            0.0
        } else {
            total_turns as f32 / edges as f32
        },
        max_turns,
        avg_span_cols: if edges == 0 {
            0.0
        } else {
            total_span_cols as f32 / edges as f32
        },
        max_span_cols,
        left_edges,
        fallback_tracks,
        linear_total,
        linear_avg,
    }
}

fn paint_entity_inspector(
    ui: &mut Ui,
    graph: &EntityGraph,
    selected_id: &mut Id,
    layout: &GraphLayout,
    routed_edges: &[RoutedEdge],
) -> Response {
    let selected_index = graph.id_to_index.get(selected_id).copied();

    let desired_width = ui.available_width();
    let (outer_rect, response) =
        ui.allocate_exact_size(vec2(desired_width, layout.canvas_size.y), Sense::hover());
    if !ui.is_rect_visible(outer_rect) {
        return response;
    }

    let offset_x = ((desired_width - layout.canvas_size.x).max(0.0)) * 0.5;
    let origin = pos2(outer_rect.left() + offset_x, outer_rect.top());
    let origin_vec = origin.to_vec2();
    let tile_rects_ui: Vec<Rect> = layout
        .tile_rects
        .iter()
        .map(|rect| rect.translate(origin_vec))
        .collect();
    let pointer_pos = ui
        .input(|input| input.pointer.hover_pos())
        .filter(|pos| outer_rect.contains(*pos));
    let hovered_node = pointer_pos.and_then(|pos| {
        tile_rects_ui
            .iter()
            .enumerate()
            .find(|(_, rect)| rect.is_positive() && rect.contains(pos))
            .map(|(idx, _)| idx)
    });

    let painter = ui.painter().with_clip_rect(outer_rect);
    let line_width: f32 = 2.5;
    // Line palette (RAL classic).
    let line_palette = [
        themes::ral(1003),
        themes::ral(2010),
        themes::ral(3001),
        themes::ral(4008),
        themes::ral(5005),
        themes::ral(6032),
        themes::ral(3014),
    ];
    let end_dot_radius = line_width * 2.5;
    let hover_threshold = end_dot_radius.max(line_width * 3.0);

    let mut edge_renders = Vec::with_capacity(routed_edges.len());
    for routed in routed_edges {
        let raw = routed
            .points
            .iter()
            .copied()
            .map(|p| p + origin_vec)
            .collect::<Vec<_>>();
        let points = round_polyline(&raw, (layout.text_row_height * 0.25).clamp(3.0, 8.0), 4);
        let palette_index = attribute_palette_index(routed.attr_id, line_palette.len());
        let line_color = line_palette[palette_index];
        edge_renders.push(EdgeRender {
            points,
            line_color,
            start_underline: routed
                .start_underline
                .map(|(a, b)| (a + origin_vec, b + origin_vec)),
            from_entity: routed.from_entity,
            to_entity: routed.to_entity,
        });
    }

    let hovered_edge = if hovered_node.is_some() {
        None
    } else {
        pointer_pos.and_then(|pos| {
            let mut best = hover_threshold * hover_threshold;
            let mut hovered = None;
            for (idx, render) in edge_renders.iter().enumerate() {
                let mut dist = distance_sq_to_polyline(pos, &render.points);
                if let Some((a, b)) = render.start_underline {
                    dist = dist.min(distance_sq_to_segment(pos, a, b));
                }
                if dist <= best {
                    best = dist;
                    hovered = Some(idx);
                }
            }
            hovered
        })
    };

    let mut end_dots = Vec::new();
    let mut end_dots_hover = Vec::new();
    let mut start_underlines = Vec::new();
    let mut start_underlines_hover = Vec::new();
    let fade_bg = ui.visuals().window_fill;
    let mut active_mask = vec![false; edge_renders.len()];
    if let Some(node_idx) = hovered_node {
        for (idx, render) in edge_renders.iter().enumerate() {
            if render.from_entity == node_idx {
                active_mask[idx] = true;
            }
        }
    } else if let Some(hovered) = hovered_edge {
        active_mask[hovered] = true;
    }

    if hovered_node.is_some() || hovered_edge.is_some() {
        for (idx, render) in edge_renders.iter().enumerate() {
            let is_active = active_mask[idx];
            let line_color = if is_active {
                render.line_color
            } else {
                themes::blend(render.line_color, fade_bg, 0.5)
            };
            let line_stroke = Stroke::new(line_width, line_color);
            paint_subway_edge(&painter, &render.points, line_stroke);
            if let Some((a, b)) = render.start_underline {
                if is_active {
                    start_underlines_hover.push((a, b, line_color));
                } else {
                    start_underlines.push((a, b, line_color));
                }
            }
            if let Some(end) = render.points.last().copied() {
                if is_active {
                    end_dots_hover.push((end, line_color));
                } else {
                    end_dots.push((end, line_color));
                }
            }
        }
    } else {
        for render in &edge_renders {
            let line_stroke = Stroke::new(line_width, render.line_color);
            paint_subway_edge(&painter, &render.points, line_stroke);
            if let Some((a, b)) = render.start_underline {
                start_underlines.push((a, b, render.line_color));
            }
            if let Some(end) = render.points.last().copied() {
                end_dots.push((end, render.line_color));
            }
        }
    }

    let mut scroll_target = None;
    let mut select_target = None;
    for (idx, node) in graph.nodes.iter().enumerate() {
        let rect = tile_rects_ui[idx];
        if !rect.is_positive() {
            continue;
        }
        let is_selected = selected_index == Some(idx);
        let table = paint_entity_table(ui, rect, node, idx, is_selected, &layout, graph);
        if scroll_target.is_none() {
            scroll_target = table.scroll_target;
        }
        if select_target.is_none() {
            select_target = table.select_target;
        }
    }

    if scroll_target.is_none()
        && ui.input(|input| input.pointer.primary_clicked())
        && hovered_edge.is_some()
    {
        let shift = ui.input(|input| input.modifiers.shift);
        if let Some(edge_idx) = hovered_edge {
            if let Some(render) = edge_renders.get(edge_idx) {
                let target_idx = if shift {
                    render.from_entity
                } else {
                    render.to_entity
                };
                scroll_target = Some(target_idx);
                select_target = Some(target_idx);
            }
        }
    }

    if let Some(target_idx) = select_target {
        if let Some(node) = graph.nodes.get(target_idx) {
            *selected_id = node.id;
        }
    }

    if let Some(target_idx) = scroll_target {
        if let Some(rect) = tile_rects_ui.get(target_idx).copied() {
            if rect.is_positive() {
                ui.scroll_to_rect(rect, Some(egui::Align::Center));
            }
        }
    }

    if !start_underlines.is_empty()
        || !end_dots.is_empty()
        || !start_underlines_hover.is_empty()
        || !end_dots_hover.is_empty()
    {
        let dot_painter = ui.painter().with_clip_rect(outer_rect);
        for (a, b, color) in start_underlines {
            dot_painter.line_segment([a, b], Stroke::new(line_width, color));
        }
        for (a, b, color) in start_underlines_hover {
            dot_painter.line_segment([a, b], Stroke::new(line_width, color));
        }
        if end_dot_radius > 0.0 {
            for (center, color) in end_dots {
                dot_painter.circle_filled(center, end_dot_radius, color);
            }
            for (center, color) in end_dots_hover {
                dot_painter.circle_filled(center, end_dot_radius, color);
            }
        }
    }

    response
}
