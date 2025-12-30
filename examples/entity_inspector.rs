#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! eframe = "0.32"
//! triblespace = { path = "../../triblespace-rs", features = ["wasm"] }
//! ```

use std::collections::{HashMap, HashSet, VecDeque};

use eframe::egui;
use egui::{pos2, vec2, Align2, Rect, Response, Sense, Stroke, TextStyle, Ui};
use triblespace::core::blob::schemas::wasmcode::WasmCode;
use triblespace::core::blob::BlobCache;
use triblespace::core::id::Id;
use triblespace::core::metadata::ConstMetadata;
use triblespace::core::query::ContainsConstraint;
use triblespace::core::query::TriblePattern;
use triblespace::core::repo::memoryrepo::MemoryRepo;
use triblespace::core::repo::BlobStore;
use triblespace::core::trible::Trible;
use triblespace::core::value::schemas::hash::Blake3;
use triblespace::core::value::schemas::hash::Handle;
use triblespace::core::value::schemas::UnknownValue;
use triblespace::core::value::Value;
use triblespace::core::value_formatter::WasmFormatterLimits;
use triblespace::core::value_formatter::WasmValueFormatter;
use triblespace::prelude::valueschemas::{GenId, ShortString};
use triblespace::prelude::{and, find, pattern, TribleSet};

use GORBIE::prelude::*;

mod demo {
    use triblespace::prelude::*;

    // A tiny synthetic schema so we can render human-friendly rows and references.
    attributes! {
        "B603E10B4BBF45B7A1BA0B7D9FA2D001" as pub name: valueschemas::ShortString;
        "B603E10B4BBF45B7A1BA0B7D9FA2D002" as pub isa: valueschemas::GenId;
        "B603E10B4BBF45B7A1BA0B7D9FA2D003" as pub subject: valueschemas::GenId;
        "B603E10B4BBF45B7A1BA0B7D9FA2D004" as pub object: valueschemas::GenId;
        "B603E10B4BBF45B7A1BA0B7D9FA2D005" as pub label: valueschemas::ShortString;
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

fn id_short(id: Id) -> String {
    let bytes: &[u8] = id.as_ref();
    hex_prefix(bytes, 4)
}

fn id_full(id: Id) -> String {
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
}

#[derive(Clone, Debug)]
struct EntityGraph {
    nodes: Vec<EntityNode>,
    edges: Vec<EntityEdge>,
    id_to_index: HashMap<Id, usize>,
}

fn build_demo_space() -> (TribleSet, MemoryRepo, Id) {
    use triblespace::macros::id_hex;

    let e_sentence = id_hex!("11111111111111111111111111111111");
    let e_transition = id_hex!("22222222222222222222222222222222");
    let e_scene = id_hex!("33333333333333333333333333333333");
    let e_patient = id_hex!("44444444444444444444444444444444");
    let e_agent = id_hex!("55555555555555555555555555555555");
    let e_command = id_hex!("66666666666666666666666666666666");
    let e_subject = id_hex!("77777777777777777777777777777777");
    let e_island_left = id_hex!("88888888888888888888888888888888");
    let e_island_right = id_hex!("99999999999999999999999999999999");

    let mut kb = TribleSet::new();
    let mut storage = MemoryRepo::default();

    let name = demo::name.id();
    let label = demo::label.id();
    let isa = demo::isa.id();
    let subject = demo::subject.id();
    let object = demo::object.id();

    let schema_genid = GenId::id();
    let schema_shortstring = ShortString::id();
    let meta_shortname = triblespace::core::metadata::shortname.id();
    let meta_value_schema = triblespace::core::metadata::value_schema.id();

    for (attr, shortname, schema) in [
        (name, "name", schema_shortstring),
        (isa, "isa", schema_genid),
        (subject, "subject", schema_genid),
        (object, "object", schema_genid),
        (label, "label", schema_shortstring),
    ] {
        kb.insert(&Trible::force(
            &attr,
            &meta_shortname,
            &triblespace::core::metadata::shortname.value_from(shortname),
        ));
        kb.insert(&Trible::force(
            &attr,
            &meta_value_schema,
            &triblespace::core::metadata::value_schema.value_from(schema),
        ));
    }

    for (entity, entity_name) in [
        (e_sentence, "Sentence"),
        (e_transition, "StateTransition"),
        (e_scene, "Scene"),
        (e_patient, "Patient"),
        (e_agent, "Agent"),
        (e_command, "Command"),
        (e_subject, "Subject"),
        (e_island_left, "IslandLeft"),
        (e_island_right, "IslandRight"),
    ] {
        kb.insert(&Trible::force(
            &entity,
            &name,
            &demo::name.value_from(entity_name),
        ));
    }

    kb.insert(&Trible::force(
        &e_sentence,
        &isa,
        &demo::isa.value_from(e_transition),
    ));
    kb.insert(&Trible::force(
        &e_sentence,
        &label,
        &demo::label.value_from("soma/isExpressedBy"),
    ));

    kb.insert(&Trible::force(
        &e_transition,
        &isa,
        &demo::isa.value_from(e_scene),
    ));
    kb.insert(&Trible::force(
        &e_transition,
        &subject,
        &demo::subject.value_from(e_patient),
    ));
    kb.insert(&Trible::force(
        &e_transition,
        &object,
        &demo::object.value_from(e_agent),
    ));

    kb.insert(&Trible::force(
        &e_scene,
        &isa,
        &demo::isa.value_from(e_command),
    ));
    kb.insert(&Trible::force(
        &e_scene,
        &label,
        &demo::label.value_from("cg/sceneState"),
    ));

    kb.insert(&Trible::force(
        &e_command,
        &subject,
        &demo::subject.value_from(e_subject),
    ));
    kb.insert(&Trible::force(
        &e_command,
        &label,
        &demo::label.value_from("cg/isa"),
    ));

    kb.insert(&Trible::force(
        &e_island_left,
        &isa,
        &demo::isa.value_from(e_island_right),
    ));
    kb.insert(&Trible::force(
        &e_island_left,
        &label,
        &demo::label.value_from("unrelated"),
    ));

    kb += GenId::describe(&mut storage);
    kb += ShortString::describe(&mut storage);

    (kb, storage, e_sentence)
}

#[derive(Clone, Debug)]
struct AttrInfo {
    label: String,
    schema: Id,
    formatter: Value<Handle<Blake3, WasmCode>>,
}

fn build_attr_info(space: &TribleSet) -> HashMap<Id, AttrInfo> {
    let mut out = HashMap::<Id, AttrInfo>::new();
    for (attr, shortname, schema, formatter) in find!(
        (
            attr: Id,
            shortname: String,
            schema: Id,
            formatter: Value<Handle<Blake3, WasmCode>>
        ),
        pattern!(
            space,
            [
                {
                    ?attr @ triblespace::core::metadata::shortname: ?shortname,
                    triblespace::core::metadata::value_schema: ?schema
                },
                { ?schema @ triblespace::core::metadata::value_formatter: ?formatter }
            ]
        )
    ) {
        out.insert(
            attr,
            AttrInfo {
                label: shortname,
                schema,
                formatter,
            },
        );
    }
    out
}

fn build_entity_graph<B>(
    space: &TribleSet,
    formatter_cache: &BlobCache<B, Blake3, WasmCode, WasmValueFormatter>,
    limits: WasmFormatterLimits,
) -> EntityGraph
where
    B: triblespace::core::repo::BlobStoreGet<Blake3>,
{
    let attr_info = build_attr_info(space);

    let schema_genid = GenId::id();
    let mut entity_ids = HashSet::<Id>::new();
    let mut tribles = Vec::<(Id, Id, [u8; 32])>::new();

    for (e, a, v) in find!(
        (e: Id, a: Id, v: Value<UnknownValue>),
        and!((&attr_info).has(a), space.pattern(e, a, v))
    ) {
        entity_ids.insert(e);

        if attr_info
            .get(&a)
            .is_some_and(|info| info.schema == schema_genid)
        {
            if let Some(target) = try_decode_genid(&v.raw) {
                entity_ids.insert(target);
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

        let Some(attr_info) = attr_info.get(&attr) else {
            continue;
        };
        let attr_text = attr_info.label.clone();

        let (value_text, target, hatched) = if attr_info.schema == schema_genid {
            if let Some(target) = try_decode_genid(&raw) {
                (format!("id:{}", id_short(target)), Some(target), false)
            } else {
                (format!("id:0x{}", hex_prefix(raw, 6)), None, false)
            }
        } else {
            match formatter_cache.get(attr_info.formatter) {
                Ok(formatter) => match formatter.format_value_with_limits(&raw, limits) {
                    Ok(text) => (text, None, false),
                    Err(_) => (format!("0x{}", hex_prefix(raw, 6)), None, true),
                },
                Err(_) => (format!("0x{}", hex_prefix(raw, 6)), None, true),
            }
        };

        raw_rows[entity_index].push(EntityRow {
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
            });
        }
    }

    EntityGraph {
        nodes,
        edges,
        id_to_index,
    }
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

fn compute_graph_layout(ui: &Ui, graph: &EntityGraph, forced_columns: usize) -> GraphLayout {
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
    let header_height = ui.fonts(|fonts| fonts.row_height(&title_font)).ceil() + 6.0;
    let text_row_height = ui.fonts(|fonts| fonts.row_height(&row_font)).ceil() + 4.0;

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
    for node_idx in 0..node_count {
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
struct GraphStats {
    nodes: usize,
    edges: usize,
    connected_components: usize,
    columns: usize,
    canvas_width: f32,
    canvas_height: f32,
    tile_coverage: f32,
    total_edge_len: f32,
    avg_edge_len: f32,
    max_edge_len: f32,
    avg_turns: f32,
    max_turns: usize,
    avg_span_cols: f32,
    max_span_cols: usize,
    left_edges: usize,
    fallback_tracks: usize,
}

#[derive(Clone, Debug)]
struct RoutedEdge {
    points: Vec<egui::Pos2>,
    length: f32,
    turns: usize,
    span_cols: usize,
    go_left: bool,
    used_fallback_track: bool,
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
        (choose_track_y(first, start_y, end_y), true)
    } else {
        (choose_track_y(&corridors, start_y, end_y), false)
    }
}

fn nearest_corner(target: Rect, from: egui::Pos2) -> egui::Pos2 {
    let corners = [
        target.left_top(),
        target.right_top(),
        target.left_bottom(),
        target.right_bottom(),
    ];
    let mut best = corners[0];
    let mut best_d2 = from.distance_sq(best);
    for &corner in &corners[1..] {
        let d2 = from.distance_sq(corner);
        if d2 < best_d2 {
            best_d2 = d2;
            best = corner;
        }
    }
    best
}

fn row_anchor(layout: &GraphLayout, tile: Rect, row: usize, on_left: bool) -> egui::Pos2 {
    let y = tile.top()
        + layout.tile_padding
        + layout.header_height
        + row as f32 * layout.text_row_height
        + layout.text_row_height * 0.5;
    let x = if on_left { tile.left() } else { tile.right() };
    pos2(x, y)
}

fn allocate_gutter_lane_offset(
    lane_counters: &mut HashMap<i32, i32>,
    boundary: i32,
    lane_spacing: f32,
    max_offset: f32,
) -> f32 {
    let lane = lane_counters.entry(boundary).or_insert(0);
    let lane = std::mem::replace(lane, *lane + 1);

    let signed = if lane == 0 {
        0
    } else {
        let n = (lane + 1) / 2;
        if lane % 2 == 1 {
            n
        } else {
            -n
        }
    };

    (signed as f32 * lane_spacing).clamp(-max_offset, max_offset)
}

fn route_edges(layout: &GraphLayout, graph: &EntityGraph) -> Vec<RoutedEdge> {
    let lane_spacing = 4.0;
    let max_lane_offset = (layout.column_gap * 0.5 - 4.0).max(0.0);
    let mut gutter_lanes = HashMap::<i32, i32>::new();

    let mut routed = Vec::with_capacity(graph.edges.len());

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
        let Some(component_layout) = layout.components.get(component) else {
            continue;
        };

        let source_rect = layout.tile_rects[edge.from_entity];
        let target_rect = layout.tile_rects[edge.to_entity];
        if !source_rect.is_positive() || !target_rect.is_positive() {
            continue;
        }

        let go_left = target_rect.center().x < source_rect.center().x;
        let start = row_anchor(layout, source_rect, edge.from_row, go_left);
        let end = nearest_corner(target_rect, start);

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

        let start_boundary = if go_left {
            from_col as i32 - 1
        } else {
            from_col as i32
        };
        let start_lane = allocate_gutter_lane_offset(
            &mut gutter_lanes,
            start_boundary,
            lane_spacing,
            max_lane_offset,
        );

        let start_gutter_x = if go_left {
            source_rect.left() - layout.column_gap * 0.5
        } else {
            source_rect.right() + layout.column_gap * 0.5
        } + start_lane;

        let end_on_left = (end.x - target_rect.left()).abs() < f32::EPSILON;
        let end_on_right = (end.x - target_rect.right()).abs() < f32::EPSILON;
        let end_boundary = if end_on_left {
            to_col as i32 - 1
        } else {
            to_col as i32
        };
        let end_lane = if end_boundary == start_boundary {
            start_lane
        } else {
            allocate_gutter_lane_offset(
                &mut gutter_lanes,
                end_boundary,
                lane_spacing,
                max_lane_offset,
            )
        };

        let end_gutter_x = if end_on_left {
            target_rect.left() - layout.column_gap * 0.5
        } else if end_on_right {
            target_rect.right() + layout.column_gap * 0.5
        } else {
            target_rect.left() - layout.column_gap * 0.5
        } + end_lane;

        let (track_y, used_fallback_track) =
            choose_track_y_between_columns(component_layout, start.y, end.y, min_col, max_col);

        let mut points = if (start_gutter_x - end_gutter_x).abs() <= 0.01 {
            vec![
                start,
                pos2(start_gutter_x, start.y),
                pos2(start_gutter_x, end.y),
                end,
            ]
        } else {
            vec![
                start,
                pos2(start_gutter_x, start.y),
                pos2(start_gutter_x, track_y),
                pos2(end_gutter_x, track_y),
                pos2(end_gutter_x, end.y),
                end,
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
            span_cols: max_col.saturating_sub(min_col),
            go_left,
            used_fallback_track,
        });
    }

    routed
}

fn paint_arrow_head(painter: &egui::Painter, tip: egui::Pos2, dir: egui::Vec2, stroke: Stroke) {
    let len = dir.length();
    if len <= 0.0 {
        return;
    }
    let dir = dir / len;
    let size = 6.0;
    let back = tip - dir * size;
    let perp = vec2(-dir.y, dir.x);
    let a = back + perp * (size * 0.5);
    let b = back - perp * (size * 0.5);
    painter.add(egui::Shape::closed_line(vec![tip, a, b], stroke));
}

fn paint_entity_table(
    ui: &mut Ui,
    rect: Rect,
    node: &EntityNode,
    is_selected: bool,
    selected_id: &mut Id,
    layout: &GraphLayout,
) -> Response {
    let id = ui.id().with(("entity_table", node.id));
    let mut response = ui.interact(rect, id, Sense::click());
    if response.clicked() {
        *selected_id = node.id;
    }

    let visuals = ui.visuals();
    let fill = visuals.window_fill;
    let ink = visuals.widgets.noninteractive.fg_stroke.color;
    let outline = if is_selected {
        visuals.selection.stroke.color
    } else {
        ink
    };
    let stroke = Stroke::new(1.0, outline);
    let grid_stroke = Stroke::new(1.0, ink);
    let hatch_color = visuals.widgets.noninteractive.bg_stroke.color;

    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, fill);
    painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);

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
        response = response.on_hover_text(full);
    }

    response
}

fn draw_entity_inspector(
    ui: &mut Ui,
    graph: &EntityGraph,
    selected_id: &mut Id,
    forced_columns: usize,
) -> GraphStats {
    let selected_index = graph.id_to_index.get(selected_id).copied();
    let layout = compute_graph_layout(ui, graph, forced_columns);
    let connected_components = connected_components(&build_adjacency(graph)).len();

    let desired_width = ui.available_width();
    let (outer_rect, _resp) =
        ui.allocate_exact_size(vec2(desired_width, layout.canvas_size.y), Sense::hover());
    if !ui.is_rect_visible(outer_rect) {
        return GraphStats::default();
    }

    let offset_x = ((desired_width - layout.canvas_size.x).max(0.0)) * 0.5;
    let origin = pos2(outer_rect.left() + offset_x, outer_rect.top());
    let origin_vec = origin.to_vec2();

    let painter = ui.painter().with_clip_rect(outer_rect);
    let ink = ui.visuals().widgets.noninteractive.fg_stroke.color;
    let edge_stroke = Stroke::new(1.0, ink);

    let routed_edges = route_edges(&layout, graph);
    for routed in &routed_edges {
        let points = routed
            .points
            .iter()
            .copied()
            .map(|p| p + origin_vec)
            .collect::<Vec<_>>();
        painter.add(egui::Shape::line(points.clone(), edge_stroke));

        if points.len() >= 2 {
            let tip = *points.last().unwrap();
            let prev = points[points.len() - 2];
            paint_arrow_head(&painter, tip, tip - prev, edge_stroke);
        }
    }

    for (idx, node) in graph.nodes.iter().enumerate() {
        let local = layout.tile_rects[idx];
        if !local.is_positive() {
            continue;
        }
        let rect = local.translate(origin_vec);
        let is_selected = selected_index == Some(idx);
        let _ = paint_entity_table(ui, rect, node, is_selected, selected_id, &layout);
    }

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

    for edge in &routed_edges {
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

    GraphStats {
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
    }
}

#[derive(Debug)]
struct InspectorState {
    selected: Id,
    columns: usize,
}

impl Default for InspectorState {
    fn default() -> Self {
        use triblespace::macros::id_hex;
        Self {
            selected: id_hex!("11111111111111111111111111111111"),
            columns: 0,
        }
    }
}

fn entity_inspector(nb: &mut Notebook) {
    view!(nb, move |ui| {
        md!(
            ui,
            "# Triblespace entity inspector (prototype)\n\nTables-first tiled layout, with orthogonal “subway” routing through gutters.\n\nClick a table to select."
        );
    });

    let (space, mut storage, default_selected) = build_demo_space();
    let reader = storage.reader().expect("demo blob store reader");
    let formatter_cache: BlobCache<_, Blake3, WasmCode, WasmValueFormatter> =
        BlobCache::new(reader);
    let limits = WasmFormatterLimits::default();
    let space = std::sync::Arc::new(space);
    let graph = std::sync::Arc::new(build_entity_graph(&space, &formatter_cache, limits));

    let inspector = state!(
        nb,
        InspectorState {
            selected: default_selected,
            columns: 0,
        },
        move |ui, state| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("COLUMNS").monospace().strong());
                let max_columns = graph.nodes.len().max(1);
                let constrain = |_: usize, next: usize| next.min(max_columns);
                ui.add(
                    widgets::NumberField::new(&mut state.columns)
                        .speed(0.25)
                        .constrain_value(&constrain),
                );
                ui.label(egui::RichText::new("(0 = auto)").monospace().weak());
            });
            ui.add_space(8.0);

            let stats = draw_entity_inspector(ui, &graph, &mut state.selected, state.columns);

            let metrics = format!(
                "_{} nodes, {} edges ({} components), {} columns._\n\
_Canvas: {:.0}×{:.0}px • Tiles: {:.0}%._\n\
_Wire: {:.0}px total • {:.0}px avg (max {:.0}px)._\n\
_Routing: {:.1} turns avg (max {}) • span {:.1} cols (max {}) • {} left • {} fallback._",
                stats.nodes,
                stats.edges,
                stats.connected_components,
                stats.columns,
                stats.canvas_width,
                stats.canvas_height,
                stats.tile_coverage * 100.0,
                stats.total_edge_len,
                stats.avg_edge_len,
                stats.max_edge_len,
                stats.avg_turns,
                stats.max_turns,
                stats.avg_span_cols,
                stats.max_span_cols,
                stats.left_edges,
                stats.fallback_tracks,
            );
            widgets::markdown(ui, &metrics);
        }
    );

    view!(nb, move |ui| {
        let selected = ui
            .with_state(inspector, |_, state| state.selected)
            .expect("inspector state missing");
        md!(ui, "Selected entity: `{}`", id_short(selected));
    });
}

fn main() {
    notebook!(entity_inspector);
}
