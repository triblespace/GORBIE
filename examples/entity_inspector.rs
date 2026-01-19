#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.33"
//! eframe = "0.33"
//! triblespace = { version = "0.7.0", features = ["wasm"] }
//! ```

use std::collections::{HashMap, HashSet, VecDeque};

use eframe::egui;
use egui::{pos2, vec2, Align2, Rect, Response, Sense, Stroke, TextStyle, Ui};
use triblespace::core::blob::schemas::wasmcode::WasmCode;
use triblespace::core::blob::BlobCache;
use triblespace::core::examples::literature;
use triblespace::core::id::ExclusiveId;
use triblespace::core::id::Id;
use triblespace::core::metadata::ConstMetadata;
use triblespace::core::query::ContainsConstraint;
use triblespace::core::query::TriblePattern;
use triblespace::core::repo::memoryrepo::MemoryRepo;
use triblespace::core::repo::BlobStore;
use triblespace::core::repo::BlobStorePut;
use triblespace::core::value::schemas::hash::Blake3;
use triblespace::core::value::schemas::hash::Handle;
use triblespace::core::value::schemas::UnknownValue;
use triblespace::core::value::Value;
use triblespace::core::value_formatter::WasmFormatterLimits;
use triblespace::core::value_formatter::WasmValueFormatter;
use triblespace::prelude::blobschemas::LongString;
use triblespace::prelude::valueschemas::{GenId, ShortString, R256};
use triblespace::prelude::{and, entity, find, pattern, TribleSet};

use GORBIE::prelude::*;

mod demo {
    use triblespace::prelude::*;

    // A tiny synthetic schema so we can render human-friendly rows and references.
    attributes! {
        "B603E10B4BBF45B7A1BA0B7D9FA2D001" as pub name: valueschemas::ShortString;
        "B603E10B4BBF45B7A1BA0B7D9FA2D002" as pub isa: valueschemas::GenId;
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

#[derive(Clone, Debug)]
struct EntityGraph {
    nodes: Vec<EntityNode>,
    edges: Vec<EntityEdge>,
    id_to_index: HashMap<Id, usize>,
}

fn build_demo_space() -> (TribleSet, MemoryRepo, Id) {
    let mut kb = TribleSet::new();
    let mut storage = MemoryRepo::default();

    let name = demo::name.id();
    let isa = demo::isa.id();
    let lit_title = literature::title.id();
    let lit_author = literature::author.id();
    let lit_firstname = literature::firstname.id();
    let lit_lastname = literature::lastname.id();
    let lit_quote = literature::quote.id();
    let lit_page_count = literature::page_count.id();

    let schema_genid = GenId::id();
    let schema_shortstring = ShortString::id();
    let schema_handle = Handle::<Blake3, LongString>::id();
    let schema_r256 = R256::id();
    for (attr, shortname, schema) in [
        (name, "name", schema_shortstring),
        (isa, "isa", schema_genid),
        (lit_title, "title", schema_shortstring),
        (lit_author, "author", schema_genid),
        (lit_firstname, "firstname", schema_shortstring),
        (lit_lastname, "lastname", schema_shortstring),
        (lit_quote, "quote", schema_handle),
        (lit_page_count, "page_count", schema_r256),
    ] {
        kb += entity! { ExclusiveId::force_ref(&attr) @
            triblespace::core::metadata::shortname: shortname,
            triblespace::core::metadata::value_schema: schema,
        };
    }

    kb += GenId::describe(&mut storage).expect("genid metadata");
    kb += Handle::<Blake3, LongString>::describe(&mut storage).expect("handle metadata");
    kb += R256::describe(&mut storage).expect("r256 metadata");
    kb += ShortString::describe(&mut storage).expect("shortstring metadata");

    fn demo_id(seed: u16) -> Id {
        let mut raw = [0u8; 16];
        raw[14..16].copy_from_slice(&seed.to_be_bytes());
        Id::new(raw).expect("demo ids are non-zero")
    }

    let e_author_kind = demo_id(0xC001);
    let e_book_kind = demo_id(0xC002);
    kb += entity! { ExclusiveId::force_ref(&e_author_kind) @ demo::name: "Author" };
    kb += entity! { ExclusiveId::force_ref(&e_book_kind) @ demo::name: "Book" };

    let authors = [
        ("Frank", "Herbert"),
        ("Isaac", "Asimov"),
        ("Mary", "Shelley"),
        ("Jane", "Austen"),
        ("Herman", "Melville"),
        ("Homer", ""),
        ("William", "Shakespeare"),
        ("Jules", "Verne"),
        ("George", "Orwell"),
        ("Virginia", "Woolf"),
        ("Fyodor", "Dostoevsky"),
        ("Leo", "Tolstoy"),
        ("Miguel", "Cervantes"),
        ("Franz", "Kafka"),
        ("Mark", "Twain"),
        ("Oscar", "Wilde"),
    ];

    let mut author_ids = Vec::with_capacity(authors.len());
    for (idx, (first, last)) in authors.iter().enumerate() {
        let id = demo_id(0xA000 + idx as u16);
        author_ids.push(id);
        let full_name = if last.is_empty() {
            (*first).to_string()
        } else {
            format!("{first} {last}")
        };
        let mut author = entity! { ExclusiveId::force_ref(&id) @
            demo::name: full_name,
            demo::isa: e_author_kind,
            literature::firstname: *first,
        };
        if !last.is_empty() {
            author += entity! { ExclusiveId::force_ref(&id) @
                literature::lastname: *last,
            };
        }
        kb += author;
    }

    let books = [
        (
            "Dune",
            0,
            "Deep in the human unconscious is a need for a logical universe.",
            412,
        ),
        (
            "Dune Messiah",
            0,
            "He shall know your ways as if born to them.",
            256,
        ),
        (
            "Foundation",
            1,
            "Violence is the last refuge of the incompetent.",
            255,
        ),
        ("I, Robot", 1, "A robot may not injure a human being.", 224),
        (
            "Frankenstein",
            2,
            "Beware; for I am fearless, and therefore powerful.",
            280,
        ),
        (
            "The Last Man",
            2,
            "My imagination was the only reality.",
            360,
        ),
        (
            "Pride and Prejudice",
            3,
            "It is a truth universally acknowledged.",
            279,
        ),
        (
            "Sense and Sensibility",
            3,
            "What do you know of my heart?",
            240,
        ),
        ("Moby Dick", 4, "Call me Ishmael.", 635),
        ("Billy Budd", 4, "The sea had jeered at it all.", 192),
        (
            "Odyssey",
            5,
            "Tell me, O Muse, of the man of many ways.",
            500,
        ),
        ("Iliad", 5, "Sing, goddess, the anger of Achilles.", 480),
        (
            "Hamlet",
            6,
            "To be, or not to be, that is the question.",
            200,
        ),
        (
            "The Tempest",
            6,
            "We are such stuff as dreams are made on.",
            200,
        ),
        ("Twenty Thousand Leagues", 7, "The sea is everything.", 300),
        (
            "Journey to the Center",
            7,
            "Science, my boy, is made up of mistakes.",
            300,
        ),
        ("1984", 8, "Big Brother is watching you.", 328),
        (
            "Animal Farm",
            8,
            "All animals are equal, but some are more equal.",
            112,
        ),
        (
            "Mrs Dalloway",
            9,
            "Mrs. Dalloway said she would buy the flowers herself.",
            296,
        ),
        ("To the Lighthouse", 9, "Nothing was simply one thing.", 209),
        (
            "Crime and Punishment",
            10,
            "The darker the night, the brighter the stars.",
            671,
        ),
        ("The Idiot", 10, "Beauty will save the world.", 656),
        (
            "War and Peace",
            11,
            "Well, Prince, so Genoa and Lucca are now just family estates.",
            1225,
        ),
        ("Anna Karenina", 11, "All happy families are alike.", 864),
        (
            "Don Quixote",
            12,
            "The truth may be stretched, but cannot be broken.",
            863,
        ),
        (
            "Metamorphosis",
            13,
            "When Gregor Samsa awoke, he found himself changed.",
            201,
        ),
        ("The Trial", 13, "Someone must have slandered Josef K.", 255),
        (
            "Tom Sawyer",
            14,
            "Tom appeared on the sidewalk with a bucket of whitewash.",
            274,
        ),
        (
            "Huckleberry Finn",
            14,
            "You do not know about me without you have read a book.",
            366,
        ),
        (
            "Dorian Gray",
            15,
            "The only way to get rid of a temptation is to yield to it.",
            254,
        ),
        (
            "Earnest",
            15,
            "The truth is rarely pure and never simple.",
            180,
        ),
    ];

    for (idx, (title, author_idx, quote, pages)) in books.iter().enumerate() {
        let id = demo_id(0xB000 + idx as u16);
        let author_id = author_ids.get(*author_idx).copied().expect("author index");
        let quote_handle = storage.put::<LongString, _>(*quote).expect("quote handle");

        kb += entity! { ExclusiveId::force_ref(&id) @
            demo::name: *title,
            demo::isa: e_book_kind,
            literature::title: *title,
            literature::author: author_id,
            literature::quote: quote_handle,
            literature::page_count: *pages as i128,
        };
    }

    (kb, storage, demo_id(0xB000))
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
    linear_total: f32,
    linear_avg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntityOrder {
    Id,
    CuthillMckee,
    Barycentric,
    BarycentricSwap,
}

#[derive(Clone, Debug)]
struct RoutedEdge {
    points: Vec<egui::Pos2>,
    length: f32,
    turns: usize,
    span_cols: usize,
    start_underline: Option<(egui::Pos2, egui::Pos2)>,
    attr_id: Id,
    go_left: bool,
    used_fallback_track: bool,
}

#[derive(Clone, Debug)]
struct EdgeRender {
    points: Vec<egui::Pos2>,
    line_color: egui::Color32,
    start_underline: Option<(egui::Pos2, egui::Pos2)>,
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

fn entity_order(graph: &EntityGraph, order: EntityOrder, barycentric_passes: usize) -> Vec<usize> {
    match order {
        EntityOrder::Id => (0..graph.nodes.len()).collect(),
        EntityOrder::CuthillMckee => cuthill_mckee_order(graph),
        EntityOrder::Barycentric => barycentric_order(graph, barycentric_passes),
        EntityOrder::BarycentricSwap => barycentric_swap_order(graph, barycentric_passes),
    }
}

fn cuthill_mckee_order(graph: &EntityGraph) -> Vec<usize> {
    let mut adjacency = build_adjacency(graph);
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
        neighbors.dedup();
    }

    let degrees: Vec<usize> = adjacency.iter().map(|neighbors| neighbors.len()).collect();
    let mut seeds: Vec<usize> = (0..graph.nodes.len()).collect();
    seeds.sort_by_key(|&idx| (degrees[idx], graph.nodes[idx].id));

    let mut visited = vec![false; graph.nodes.len()];
    let mut order = Vec::with_capacity(graph.nodes.len());
    let mut queue = VecDeque::new();

    for start in seeds {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        queue.push_back(start);
        while let Some(node) = queue.pop_front() {
            order.push(node);
            let mut neighbors = adjacency[node].clone();
            neighbors.sort_by_key(|&idx| (degrees[idx], graph.nodes[idx].id));
            for next in neighbors {
                if !visited[next] {
                    visited[next] = true;
                    queue.push_back(next);
                }
            }
        }
    }

    order
}

fn barycentric_order(graph: &EntityGraph, passes: usize) -> Vec<usize> {
    let adjacency = build_adjacency(graph);
    let mut order: Vec<usize> = (0..graph.nodes.len()).collect();
    let mut best = order.clone();
    let mut best_cost = order_linear_cost(graph, &best);

    let passes = passes.max(1);
    for pass in 0..passes {
        let reverse_ties = pass % 2 == 1;
        order = barycentric_pass(graph, &adjacency, order, reverse_ties);
        let cost = order_linear_cost(graph, &order);
        if cost < best_cost {
            best_cost = cost;
            best.clone_from(&order);
        }
    }

    best
}

fn barycentric_pass(
    graph: &EntityGraph,
    adjacency: &[Vec<usize>],
    mut order: Vec<usize>,
    reverse_ties: bool,
) -> Vec<usize> {
    let mut positions = vec![0usize; graph.nodes.len()];
    let mut positions_f32 = vec![0.0f32; graph.nodes.len()];
    let mut scores = vec![0.0f32; graph.nodes.len()];

    for _ in 0..8 {
        for (pos, &idx) in order.iter().enumerate() {
            positions[idx] = pos;
            positions_f32[idx] = pos as f32;
        }

        for (idx, neighbors) in adjacency.iter().enumerate() {
            if neighbors.is_empty() {
                scores[idx] = positions_f32[idx];
                continue;
            }
            let sum = neighbors
                .iter()
                .map(|&neighbor| positions_f32[neighbor])
                .sum::<f32>();
            scores[idx] = sum / neighbors.len() as f32;
        }

        let mut next = order.clone();
        next.sort_by(|a, b| {
            scores[*a]
                .total_cmp(&scores[*b])
                .then_with(|| {
                    let left = positions[*a];
                    let right = positions[*b];
                    if reverse_ties {
                        right.cmp(&left)
                    } else {
                        left.cmp(&right)
                    }
                })
                .then_with(|| graph.nodes[*a].id.cmp(&graph.nodes[*b].id))
        });
        if next == order {
            break;
        }
        order = next;
    }

    order
}

fn order_linear_cost(graph: &EntityGraph, order: &[usize]) -> i64 {
    let mut positions = vec![0usize; graph.nodes.len()];
    for (pos, &idx) in order.iter().enumerate() {
        positions[idx] = pos;
    }

    let mut total = 0i64;
    for edge in &graph.edges {
        let from = positions[edge.from_entity] as i64;
        let to = positions[edge.to_entity] as i64;
        total += (from - to).abs();
    }

    total
}

fn barycentric_swap_order(graph: &EntityGraph, passes: usize) -> Vec<usize> {
    let mut order = barycentric_order(graph, passes);
    let adjacency = build_adjacency(graph);
    local_swap_refine(&mut order, &adjacency);
    order
}

fn local_swap_refine(order: &mut [usize], adjacency: &[Vec<usize>]) {
    let mut positions = vec![0usize; order.len()];
    for (pos, &idx) in order.iter().enumerate() {
        positions[idx] = pos;
    }

    for _ in 0..8 {
        let mut changed = false;
        for pos in 0..order.len().saturating_sub(1) {
            let left = order[pos];
            let right = order[pos + 1];
            let mut delta = 0i32;

            for &neighbor in &adjacency[left] {
                let old = (positions[left] as i32 - positions[neighbor] as i32).abs();
                let new = (positions[right] as i32 - positions[neighbor] as i32).abs();
                delta += new - old;
            }
            for &neighbor in &adjacency[right] {
                let old = (positions[right] as i32 - positions[neighbor] as i32).abs();
                let new = (positions[left] as i32 - positions[neighbor] as i32).abs();
                delta += new - old;
            }

            if delta < 0 {
                order.swap(pos, pos + 1);
                positions.swap(left, right);
                changed = true;
            }
        }

        if !changed {
            break;
        }
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

fn compute_inspector(
    ui: &Ui,
    graph: &EntityGraph,
    forced_columns: usize,
    order: EntityOrder,
    barycentric_passes: usize,
) -> (GraphLayout, Vec<RoutedEdge>, GraphStats) {
    let order = entity_order(graph, order, barycentric_passes);
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
) -> GraphStats {
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
) {
    let selected_index = graph.id_to_index.get(selected_id).copied();

    let desired_width = ui.available_width();
    let (outer_rect, _resp) =
        ui.allocate_exact_size(vec2(desired_width, layout.canvas_size.y), Sense::hover());
    if !ui.is_rect_visible(outer_rect) {
        return;
    }

    let offset_x = ((desired_width - layout.canvas_size.x).max(0.0)) * 0.5;
    let origin = pos2(outer_rect.left() + offset_x, outer_rect.top());
    let origin_vec = origin.to_vec2();

    let painter = ui.painter().with_clip_rect(outer_rect);
    let line_width: f32 = 2.5;
    // Line palette (RAL classic).
    let line_palette = [
        GORBIE::themes::ral(1003),
        GORBIE::themes::ral(2010),
        GORBIE::themes::ral(3001),
        GORBIE::themes::ral(4008),
        GORBIE::themes::ral(5005),
        GORBIE::themes::ral(6032),
        GORBIE::themes::ral(3014),
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
        });
    }

    let hovered_edge = ui
        .input(|input| input.pointer.hover_pos())
        .filter(|pos| outer_rect.contains(*pos))
        .and_then(|pos| {
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
        });

    let mut end_dots = Vec::new();
    let mut end_dots_hover = Vec::new();
    let mut start_underlines = Vec::new();
    let mut start_underlines_hover = Vec::new();
    let fade_bg = ui.visuals().window_fill;
    if let Some(hovered) = hovered_edge {
        for (idx, render) in edge_renders.iter().enumerate() {
            if idx == hovered {
                continue;
            }
            let line_color = GORBIE::themes::blend(render.line_color, fade_bg, 0.5);
            let line_stroke = Stroke::new(line_width, line_color);
            paint_subway_edge(&painter, &render.points, line_stroke);
            if let Some((a, b)) = render.start_underline {
                start_underlines.push((a, b, line_color));
            }
            if let Some(end) = render.points.last().copied() {
                end_dots.push((end, line_color));
            }
        }
        if let Some(render) = edge_renders.get(hovered) {
            let line_stroke = Stroke::new(line_width, render.line_color);
            paint_subway_edge(&painter, &render.points, line_stroke);
            if let Some((a, b)) = render.start_underline {
                start_underlines_hover.push((a, b, render.line_color));
            }
            if let Some(end) = render.points.last().copied() {
                end_dots_hover.push((end, render.line_color));
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

    for (idx, node) in graph.nodes.iter().enumerate() {
        let local = layout.tile_rects[idx];
        if !local.is_positive() {
            continue;
        }
        let rect = local.translate(origin_vec);
        let is_selected = selected_index == Some(idx);
        let _ = paint_entity_table(ui, rect, node, is_selected, selected_id, &layout);
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
}

#[derive(Debug)]
struct InspectorState {
    selected: Id,
    columns: usize,
    order: EntityOrder,
    barycentric_passes: usize,
}

impl Default for InspectorState {
    fn default() -> Self {
        use triblespace::macros::id_hex;
        Self {
            selected: id_hex!("11111111111111111111111111111111"),
            columns: 0,
            order: EntityOrder::Id,
            barycentric_passes: 4,
        }
    }
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;
    nb.view(move |ui| {
        with_padding(ui, padding, |ui| {
            md!(
                ui,
                "# Hi Triblespace entity inspector (prototype)\n\nTables-first tiled layout, with orthogonal “subway” routing through gutters.\n\nClick a table to select."
            );
        });
    });

    let (space, mut storage, default_selected) = build_demo_space();
    let reader = storage.reader().expect("demo blob store reader");
    let formatter_cache: BlobCache<_, Blake3, WasmCode, WasmValueFormatter> =
        BlobCache::new(reader);
    let limits = WasmFormatterLimits::default();
    let space = std::sync::Arc::new(space);
    let graph = std::sync::Arc::new(build_entity_graph(&space, &formatter_cache, limits));

    let inspector = nb.state(
        "inspector",
        InspectorState {
            selected: default_selected,
            columns: 0,
            order: EntityOrder::Id,
            barycentric_passes: 4,
        },
        move |ui, state| {
            with_padding(ui, padding, |ui| {
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
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("ORDER").monospace().strong());
                    ui.add(
                        widgets::ChoiceToggle::new(&mut state.order)
                            .choice(EntityOrder::Id, "ID")
                            .choice(EntityOrder::CuthillMckee, "CM")
                            .choice(EntityOrder::Barycentric, "BC")
                            .choice(EntityOrder::BarycentricSwap, "BS")
                            .small(),
                    );
                    if matches!(
                        state.order,
                        EntityOrder::Barycentric | EntityOrder::BarycentricSwap
                    ) {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("BC PASSES").monospace().weak());
                        let constrain = |_: usize, next: usize| next.clamp(1, 32);
                        ui.add(
                            widgets::NumberField::new(&mut state.barycentric_passes)
                                .speed(0.25)
                                .constrain_value(&constrain),
                        );
                    }
                });
                ui.add_space(8.0);

                let (layout, routed_edges, stats) = compute_inspector(
                    ui,
                    &graph,
                    state.columns,
                    state.order,
                    state.barycentric_passes,
                );

                let metrics = format!(
                    "_{} nodes, {} edges ({} components), {} columns._\n\
_Canvas: {:.0}×{:.0}px • Tiles: {:.0}%._\n\
_Order: {:.0} 1D dist total • {:.1} avg._\n\
_Wire: {:.0}px total • {:.0}px avg (max {:.0}px)._\n\
_Routing: {:.1} turns avg (max {}) • span {:.1} cols (max {}) • {} left • {} fallback._",
                    stats.nodes,
                    stats.edges,
                    stats.connected_components,
                    stats.columns,
                    stats.canvas_width,
                    stats.canvas_height,
                    stats.tile_coverage * 100.0,
                    stats.linear_total,
                    stats.linear_avg,
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
                ui.add_space(8.0);

                paint_entity_inspector(ui, &graph, &mut state.selected, &layout, &routed_edges);
            });
        },
    );

    nb.view(move |ui| {
        with_padding(ui, padding, |ui| {
            let selected = inspector
                .read(ui.store())
                .expect("inspector state missing")
                .selected;
            md!(ui, "Selected entity: `{}`", id_short(selected));
        });
    });
}
