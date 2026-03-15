use std::cell::RefCell;
use std::fmt::Write;
use std::ops::Range;
use std::sync::LazyLock;

use eframe::egui;
use typst::syntax::{Source, Span, SyntaxKind};

use crate::themes::colorhash::RAL_CATEGORICAL;
use crate::themes::ral::RAL_COLORS;
use super::typst_render::outline::GlyphCache;
use super::typst_render::painter;
use super::typst_render::world::GorbieWorld;

/// Persistent state for Typst compilation and rendering.
///
/// Holds a `GorbieWorld` (benefits from comemo's incremental memoization)
/// and a `GlyphCache` (tessellated glyph meshes, never invalidated).
struct TypstState {
    world: GorbieWorld,
    glyph_cache: GlyphCache,
}

impl TypstState {
    fn new() -> Self {
        Self {
            world: GorbieWorld::new(),
            glyph_cache: GlyphCache::new(),
        }
    }
}

thread_local! {
    static TYPST_STATE: RefCell<TypstState> = RefCell::new(TypstState::new());
}

/// Render a full Typst document string into the UI.
///
/// The source is compiled as-is — callers are responsible for any
/// `#set page(...)` / `#set text(...)` preamble they want.
pub fn typst(ui: &mut egui::Ui, source: &str) {
    TYPST_STATE.with(|state| {
        let mut state = state.borrow_mut();
        render_typst(ui, &mut state, source, 0);
    });
}

/// Render an inline math expression: `$<expr>$`.
pub fn typst_math_inline(ui: &mut egui::Ui, expr: &str) {
    let fg = typst_rgb(ui.visuals().text_color());
    let size = ui.style()
        .text_styles
        .get(&egui::TextStyle::Body)
        .map_or(15.0, |d| d.size);
    let preamble = format!(
        "#set page(width: auto, height: auto, margin: 0pt)\n\
         #set text(size: {size}pt, fill: {fg})\n"
    );
    let source = format!("{preamble}${expr}$");
    TYPST_STATE.with(|state| {
        let mut state = state.borrow_mut();
        render_typst(ui, &mut state, &source, preamble.len());
    });
}

/// Render a display-mode math expression (centered block):
/// `$ <expr> $` (with spaces for display mode in Typst).
pub fn typst_math_display(ui: &mut egui::Ui, expr: &str) {
    let fg = typst_rgb(ui.visuals().text_color());
    let size = ui.style()
        .text_styles
        .get(&egui::TextStyle::Body)
        .map_or(15.0, |d| d.size);
    let preamble = format!(
        "#set page(width: auto, height: auto, margin: 0pt)\n\
         #set text(size: {size}pt, fill: {fg})\n"
    );
    let source = format!("{preamble}$ {expr} $");
    TYPST_STATE.with(|state| {
        let mut state = state.borrow_mut();
        render_typst(ui, &mut state, &source, preamble.len());
    });
}

/// Returns a closure suitable for use as a `render_math_fn` in gorbie-commonmark.
///
/// The closure renders `$...$` (inline) or `$$...$$` (display) math
/// using Typst compilation + direct vector painting.
pub fn typst_math_fn() -> impl Fn(&mut egui::Ui, &str, bool) {
    |ui: &mut egui::Ui, expr: &str, display: bool| {
        if display {
            typst_math_display(ui, expr);
        } else {
            typst_math_inline(ui, expr);
        }
    }
}

/// Render Typst content with a GORBIE-appropriate preamble automatically injected.
///
/// Sets page width to the available UI width with GRID_EDGE_PAD margins,
/// body text size from the UI style, and IosevkaGorbie as the default font.
/// The Typst page margin replaces the grid edge padding so the interactive
/// area extends to the card edges (better for drag-selection).
pub fn typst_with_preamble(ui: &mut egui::Ui, content: &str) {
    let width = ui.available_width();
    let pad = crate::card_ctx::GRID_EDGE_PAD;
    let body_size = ui.style()
        .text_styles
        .get(&egui::TextStyle::Body)
        .map_or(15.0, |d| d.size);
    let palette = ral_preamble(ui);

    let preamble = format!(
        "{palette}\
         #set page(width: {width}pt, height: auto, margin: (x: {pad}pt, top: {pad}pt, bottom: {pad}pt))\n\
         #set text(size: {body_size}pt, font: \"IosevkaGorbie\", fill: ral-fg)\n\
         #set table(stroke: ral-fg)\n\
         #set line(stroke: ral-fg)\n\
         #set rect(stroke: ral-fg)\n\
         #set circle(stroke: ral-fg)\n\
         #set ellipse(stroke: ral-fg)\n\
         #set polygon(stroke: ral-fg)\n\
         #set path(stroke: ral-fg)\n"
    );
    let source = format!("{preamble}{content}");
    TYPST_STATE.with(|state| {
        let mut state = state.borrow_mut();
        render_typst(ui, &mut state, &source, preamble.len());
    });
}

/// Render a Typst document with GORBIE grid defaults.
///
/// Automatically sets page width to the available width with edge padding
/// as Typst page margins, IosevkaGorbie as the body font, and the current
/// body text size. The widget fills the full card width so drag-selection
/// extends to the card edges.
///
/// ```ignore
/// typst!(ctx, "= Hello World\nThis is *Typst* in GORBIE.");
/// typst!(ctx, "$ {} $", some_math_expr);
/// ```
#[macro_export]
macro_rules! typst {
    ($ctx:expr, $fmt:expr $(, $args:expr)*) => {
        {
            let text = format!($fmt $(, $args)*);
            $crate::widgets::typst_widget::typst_with_preamble($ctx, &text);
        }
    };
}

/// Format a `Color32` as a Typst `rgb("#RRGGBB")` literal.
fn typst_rgb(c: egui::Color32) -> String {
    format!("rgb(\"#{:02X}{:02X}{:02X}\")", c.r(), c.g(), c.b())
}

/// RAL color names for the categorical palette, matching `RAL_CATEGORICAL` order.
const RAL_CAT_NAMES: &[&str] = &[
    "yellow", "orange", "pink", "red", "violet", "blue",
    "sky", "water", "lime", "mint", "green", "teal",
];

/// The static part of the GORBIE Typst preamble: grid constants, full 272-color
/// RAL lookup dictionary, and semantic named aliases. Generated once, reused.
static RAL_TYPST_STATIC: LazyLock<String> = LazyLock::new(|| {
    use crate::card_ctx::{GRID_COL_WIDTH, GRID_GUTTER, GRID_COLUMNS, GRID_ROW_MODULE};

    let mut s = String::with_capacity(16384);

    // Grid system: matches the Rust-side 12-column layout exactly.
    let _ = writeln!(s, "#let grid-col = {GRID_COL_WIDTH}pt");
    let _ = writeln!(s, "#let grid-gutter = {GRID_GUTTER}pt");
    let _ = writeln!(s, "#let grid-columns = {GRID_COLUMNS}");
    let _ = writeln!(s, "#let grid-row = {GRID_ROW_MODULE}pt");
    let _ = writeln!(s, "#let grid-span(n) = n * {GRID_COL_WIDTH}pt + (n - 1) * {GRID_GUTTER}pt");

    // Full RAL dictionary: ral(1003) → rgb("#F9A800")
    let _ = writeln!(s, "#let ral-table = (");
    for &(code, _, color) in RAL_COLORS {
        let _ = writeln!(
            s,
            "  \"{code}\": rgb(\"#{:02X}{:02X}{:02X}\"),",
            color.r(),
            color.g(),
            color.b()
        );
    }
    let _ = writeln!(s, ")");
    let _ = writeln!(s, "#let ral(num) = ral-table.at(str(num))");
    // Semantic aliases from the dictionary
    let _ = writeln!(s, "#let ral-accent = ral(2009)");
    for (&code, &name) in RAL_CATEGORICAL.iter().zip(RAL_CAT_NAMES) {
        let _ = writeln!(s, "#let ral-{name} = ral({code})");
    }
    s
});

/// Generate a Typst preamble with GORBIE grid constants and RAL color palette.
///
/// Injects:
/// - `grid-col`, `grid-gutter`, `grid-columns`, `grid-row` — grid constants
/// - `grid-span(n)` — width of `n` grid columns including inner gutters
/// - `ral(num)` — lookup any of the 272 RAL Classic colors by number
/// - `ral-fg` / `ral-bg` — current theme foreground and background
/// - `ral-accent` — RAL 2009 Traffic orange
/// - The 12 categorical colors (`ral-yellow`, `ral-blue`, …)
///
/// Included automatically by [`typst_with_preamble`] and the [`typst!`] macro.
/// Call manually when building custom source strings that need palette access.
pub fn ral_preamble(ui: &egui::Ui) -> String {
    let fg = ui.visuals().text_color();
    let bg = ui.visuals().panel_fill;

    let mut s = RAL_TYPST_STATIC.clone();
    let _ = writeln!(s, "#let ral-fg = {}", typst_rgb(fg));
    let _ = writeln!(s, "#let ral-bg = {}", typst_rgb(bg));
    s
}

/// Persistent selection state for a single Typst render area.
///
/// Stores raw frame-relative positions from the drag gesture.
/// The actual glyph range is computed via a 2D→1D hybrid:
/// build a bounding rect from anchor↔cursor, find all glyphs
/// whose rects overlap it, then take the min/max flat indices.
#[derive(Clone, Default)]
struct TypstSelection {
    /// Frame-relative position where the drag started.
    anchor: Option<egui::Pos2>,
    /// Frame-relative position at the current pointer.
    cursor: Option<egui::Pos2>,
    /// Exact glyph range override (set by double-click, bypasses 2D rect).
    glyph_override: Option<std::ops::RangeInclusive<usize>>,
}

impl TypstSelection {
    /// The 2D selection rectangle, if both anchor and cursor are set.
    fn sel_rect(&self) -> Option<egui::Rect> {
        Some(egui::Rect::from_two_pos(self.anchor?, self.cursor?))
    }

    /// Compute the flat glyph index range from the 2D selection rect,
    /// or return the exact override if set by double-click.
    fn range(&self, chars: &[painter::PositionedChar]) -> Option<std::ops::RangeInclusive<usize>> {
        if let Some(ref r) = self.glyph_override {
            return Some(r.clone());
        }

        let sel_rect = self.sel_rect()?;

        let mut lo = usize::MAX;
        let mut hi = 0usize;
        for (i, ch) in chars.iter().enumerate() {
            if sel_rect.intersects(ch.rect) {
                lo = lo.min(i);
                hi = hi.max(i);
            }
        }
        if lo <= hi { Some(lo..=hi) } else { None }
    }
}

// ── Double-click helpers ─────────────────────────────────────────────

/// Find the glyph whose center is nearest to `pos`.
fn nearest_glyph(chars: &[painter::PositionedChar], pos: egui::Pos2) -> Option<usize> {
    chars.iter().enumerate().min_by(|(_, a), (_, b)| {
        let da = a.rect.center().distance_sq(pos);
        let db = b.rect.center().distance_sq(pos);
        da.partial_cmp(&db).unwrap()
    }).map(|(i, _)| i)
}

/// Double-click glyph range: walk up the AST and stop at the
/// opacity transition boundary (transparent↔opaque).
///
/// - Opaque leaf (math): walks through opaque ancestors, stops when
///   hitting transparent → selects the whole formula/equation.
/// - Transparent leaf in opaque parent (table cell): walks through
///   transparent nodes, stops at first opaque → selects the cell.
/// - All transparent (plain text): falls back to word boundaries.
fn double_click_range(
    source: &typst::syntax::Source,
    chars: &[painter::PositionedChar],
    idx: usize,
) -> (usize, usize) {
    if let Some(node) = source.find(chars[idx].span.0) {
        let mut current = &node;
        loop {
            let opaque = !is_transparent(current.kind());
            match current.parent() {
                Some(parent) => {
                    let parent_opaque = !is_transparent(parent.kind());
                    // Opacity transition — select whichever side is opaque.
                    if opaque != parent_opaque {
                        let target = if opaque { current.range() } else { parent.range() };
                        let (mut lo, mut hi) = (usize::MAX, 0usize);
                        for (i, ch) in chars.iter().enumerate() {
                            if let Some(r) = source.range(ch.span.0) {
                                if r.start >= target.start && r.end <= target.end {
                                    lo = lo.min(i);
                                    hi = hi.max(i);
                                }
                            }
                        }
                        if lo <= hi { return (lo, hi); }
                        break;
                    }
                    current = parent;
                }
                None => break,
            }
        }
    }

    // All transparent — select paragraph via AST sibling walk.
    // Walk up until the parent is Markup, then expand siblings
    // until hitting a paragraph boundary.
    if let Some(node) = source.find(chars[idx].span.0) {
        let mut current = &node;
        loop {
            match current.parent() {
                Some(parent) if parent.kind() == SyntaxKind::Markup => {
                    // current is a direct child of Markup.
                    let child_idx = current.index();
                    let children: Vec<_> = parent.children().collect();
                    let is_boundary = |k: SyntaxKind| matches!(
                        k,
                        SyntaxKind::Parbreak | SyntaxKind::Heading
                            | SyntaxKind::ListItem | SyntaxKind::Equation
                    );
                    let mut start = child_idx;
                    while start > 0 && !is_boundary(children[start - 1].kind()) {
                        start -= 1;
                    }
                    let mut end = child_idx;
                    while end + 1 < children.len() && !is_boundary(children[end + 1].kind()) {
                        end += 1;
                    }
                    let byte_start = children[start].range().start;
                    let byte_end = children[end].range().end;
                    let (mut lo, mut hi) = (usize::MAX, 0usize);
                    for (i, ch) in chars.iter().enumerate() {
                        if let Some(r) = source.range(ch.span.0) {
                            if r.start >= byte_start && r.end <= byte_end {
                                lo = lo.min(i);
                                hi = hi.max(i);
                            }
                        }
                    }
                    if lo <= hi { return (lo, hi); }
                    break;
                }
                Some(parent) => current = parent,
                None => break,
            }
        }
    }
    (idx, idx)
}

// ── AST-based selection ──────────────────────────────────────────────

/// Result of walking the syntax tree to determine selection granularity.
///
/// The algorithm finds the lowest common ancestor (LCA) of the min and
/// max selected glyphs, then selects the range of LCA children between
/// them. If all non-trivia children are covered, the selection collapses
/// to the LCA node itself.
struct AstSelection {
    /// Source byte ranges of the selected units, with transparency flag.
    /// `true` = transparent (allows partial selection).
    /// `false` = opaque (any glyph → full node).
    units: Vec<(Range<usize>, bool)>,
    /// True when all selected glyphs map to a single leaf node.
    /// In this case, use rendered unicode for character-level precision.
    single_leaf: bool,
}

/// Collect the ancestor chain from root to the node at `span`.
/// Returns `(byte_range, child_index_in_parent)` pairs, root-first.
fn ancestor_path(source: &Source, span: Span) -> Vec<(Range<usize>, usize)> {
    let Some(node) = source.find(span) else { return Vec::new() };
    let mut path = Vec::new();
    let mut current = &node;
    loop {
        path.push((current.range(), current.index()));
        match current.parent() {
            Some(p) => current = p,
            None => break,
        }
    }
    path.reverse();
    path
}

/// Whether a syntax node allows partial (character-level) selection.
///
/// Transparent nodes are inline text formatting — the user can select
/// individual characters within them. Opaque nodes (math, tables,
/// function calls, etc.) promote to full-node selection when any
/// glyph inside is touched.
fn is_transparent(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Text
            | SyntaxKind::Space
            | SyntaxKind::Markup
            | SyntaxKind::Strong
            | SyntaxKind::Emph
            | SyntaxKind::SmartQuote
    )
}

/// Get children byte ranges and kinds of the node at `lca_range`.
/// Navigates from `anchor_span` upward to find the LCA node.
fn lca_children(
    source: &Source,
    anchor_span: Span,
    lca_range: &Range<usize>,
) -> Vec<(Range<usize>, SyntaxKind)> {
    let Some(node) = source.find(anchor_span) else { return Vec::new() };
    let mut current = &node;
    loop {
        if current.range() == *lca_range {
            return current.children().map(|c| (c.range(), c.kind())).collect();
        }
        match current.parent() {
            Some(p) => current = p,
            None => return Vec::new(),
        }
    }
}

/// Check whether all glyphs whose source spans fall within `node_range`
/// are also within the selected `glyph_range`.
///
/// Non-visual syntax (brackets, commas, keywords) produces no glyphs
/// and therefore never blocks collapse.
fn all_glyphs_covered(
    source: &Source,
    chars: &[painter::PositionedChar],
    node_range: &Range<usize>,
    glyph_range: &std::ops::RangeInclusive<usize>,
) -> bool {
    chars.iter().enumerate().all(|(i, ch)| {
        if let Some(r) = source.range(ch.span.0) {
            if r.start >= node_range.start && r.end <= node_range.end {
                glyph_range.contains(&i)
            } else {
                true
            }
        } else {
            true
        }
    })
}

/// Walk up from `start_range`, collapsing to each ancestor whose
/// glyphs are all within the selected glyph range.
/// Returns the largest range that passes the glyph-coverage check,
/// and the `SyntaxKind` of the final collapsed node.
fn collapse_upward(
    source: &Source,
    chars: &[painter::PositionedChar],
    glyph_range: &std::ops::RangeInclusive<usize>,
    anchor_span: Span,
    start_range: &Range<usize>,
    start_kind: SyntaxKind,
) -> (Range<usize>, SyntaxKind) {
    let Some(node) = source.find(anchor_span) else {
        return (start_range.clone(), start_kind);
    };
    let mut current = &node;
    let mut best = start_range.clone();
    let mut best_kind = start_kind;

    // Walk up to start_range.
    while current.range() != *start_range {
        match current.parent() {
            Some(p) => current = p,
            None => return (best, best_kind),
        }
    }

    // Try ancestors.
    while let Some(parent) = current.parent() {
        current = parent;
        if all_glyphs_covered(source, chars, &current.range(), glyph_range) {
            best = current.range();
            best_kind = current.kind();
        } else {
            break;
        }
    }

    // Include preceding `#` — Typst splits `#expr` into sibling
    // Hash + FuncCall/SetRule/LetBinding nodes, but they're one construct.
    if best.start > 0 && source.text().as_bytes()[best.start - 1] == b'#' {
        best.start -= 1;
    }

    (best, best_kind)
}

/// Determine selection units by walking the syntax tree.
///
/// 1. Map min/max glyph spans to leaf nodes
/// 2. Build ancestor paths from root to each leaf
/// 3. Find the lowest common ancestor (LCA)
/// 4. Select the range of LCA children from min-child to max-child
/// 5. Collapse upward: if all glyphs within a parent are selected,
///    expand the selection to that parent (repeating up the tree)
/// 6. Tag each unit as transparent (partial ok) or opaque (all-or-nothing)
fn ast_select(
    source: &Source,
    chars: &[painter::PositionedChar],
    range: &std::ops::RangeInclusive<usize>,
) -> AstSelection {
    let lo_span = chars[*range.start()].span.0;
    let hi_span = chars[*range.end()].span.0;

    let path_lo = ancestor_path(source, lo_span);
    let path_hi = ancestor_path(source, hi_span);

    if path_lo.is_empty() || path_hi.is_empty() {
        return AstSelection { units: Vec::new(), single_leaf: false };
    }

    // Single leaf: both endpoints map to the same node.
    let single_leaf = path_lo.last().unwrap().0 == path_hi.last().unwrap().0;

    if single_leaf {
        let leaf_range = path_lo.last().unwrap().0.clone();
        let (collapsed, kind) = collapse_upward(
            source, chars, range, lo_span, &leaf_range, SyntaxKind::Text,
        );
        let still_leaf = collapsed == leaf_range;
        return AstSelection {
            single_leaf: still_leaf,
            units: vec![(collapsed, still_leaf || is_transparent(kind))],
        };
    }

    // Find LCA depth: deepest level where both paths agree.
    let mut lca_depth = 0;
    for i in 0..path_lo.len().min(path_hi.len()) {
        if path_lo[i].0 == path_hi[i].0 {
            lca_depth = i;
        } else {
            break;
        }
    }

    let lca_range = path_lo[lca_depth].0.clone();

    // Edge case: one glyph maps directly to the LCA node.
    if lca_depth + 1 >= path_lo.len() || lca_depth + 1 >= path_hi.len() {
        let (collapsed, kind) = collapse_upward(
            source, chars, range, lo_span, &lca_range, SyntaxKind::Markup,
        );
        return AstSelection {
            units: vec![(collapsed, is_transparent(kind))],
            single_leaf: false,
        };
    }

    let lo_child = path_lo[lca_depth + 1].1.min(path_hi[lca_depth + 1].1);
    let hi_child = path_lo[lca_depth + 1].1.max(path_hi[lca_depth + 1].1);

    let children = lca_children(source, lo_span, &lca_range);
    if children.is_empty() {
        let (collapsed, kind) = collapse_upward(
            source, chars, range, lo_span, &lca_range, SyntaxKind::Markup,
        );
        return AstSelection {
            units: vec![(collapsed, is_transparent(kind))],
            single_leaf: false,
        };
    }

    // Can we collapse to the LCA (all its glyphs are selected)?
    if all_glyphs_covered(source, chars, &lca_range, range) {
        let (collapsed, kind) = collapse_upward(
            source, chars, range, lo_span, &lca_range, SyntaxKind::Markup,
        );
        AstSelection {
            units: vec![(collapsed, is_transparent(kind))],
            single_leaf: false,
        }
    } else {
        let units = children[lo_child..=hi_child]
            .iter()
            .map(|(r, kind)| (r.clone(), is_transparent(*kind)))
            .collect();
        AstSelection { units, single_leaf: false }
    }
}

/// Render Typst compilation errors as a rustc-style diagnostic block.
fn render_typst_errors(
    ui: &mut egui::Ui,
    source: &str,
    preamble_len: usize,
    diags: &[super::typst_render::world::TypstDiag],
) {
    let pad = crate::card_ctx::GRID_EDGE_PAD;
    let frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(pad as i8, pad as i8));
    frame.show(ui, |ui| {
    render_typst_errors_inner(ui, source, preamble_len, diags);
    });
}

fn render_typst_errors_inner(
    ui: &mut egui::Ui,
    source: &str,
    preamble_len: usize,
    diags: &[super::typst_render::world::TypstDiag],
) {
    use typst::diag::Severity;

    let user_source = &source[preamble_len..];
    // Build line table for user source (byte offset → line number).
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(user_source.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let byte_to_line = |byte: usize| -> usize {
        line_starts.partition_point(|&start| start <= byte).saturating_sub(1)
    };
    let line_text = |line: usize| -> &str {
        let start = line_starts[line];
        let end = if line + 1 < line_starts.len() {
            line_starts[line + 1].saturating_sub(1) // trim \n
        } else {
            user_source.len()
        };
        &user_source[start..end]
    };

    let mono = egui::FontId::monospace(12.0);
    let error_color = egui::Color32::from_rgb(0xFF, 0x44, 0x44);
    let warning_color = egui::Color32::from_rgb(0xFF, 0xCC, 0x22);
    let hint_color = egui::Color32::from_rgb(0x55, 0xBB, 0xFF);
    let line_num_color = ui.visuals().weak_text_color();
    let source_color = ui.visuals().text_color();

    let fmt = |color: egui::Color32| egui::text::TextFormat {
        font_id: mono.clone(),
        color,
        ..Default::default()
    };

    let label_no_wrap = |ui: &mut egui::Ui, mut job: egui::text::LayoutJob| {
        job.wrap = egui::text::TextWrapping {
            max_rows: 1,
            break_anywhere: false,
            overflow_character: Some('…'),
            ..Default::default()
        };
        ui.label(job);
    };

    let prev_spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;

    // Deduplicate: keep only the first diagnostic per source line.
    let mut seen_lines = std::collections::HashSet::new();
    let diags: Vec<_> = diags.iter().filter(|d| {
        let line = d.span_range.as_ref()
            .filter(|r| r.start >= preamble_len)
            .map(|r| byte_to_line(r.start - preamble_len));
        match line {
            Some(l) => seen_lines.insert(l),
            None => true,
        }
    }).collect();

    for diag in diags {
        let color = match diag.severity {
            Severity::Error => error_color,
            Severity::Warning => warning_color,
        };
        let prefix = match diag.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };

        // Header: "error: unclosed delimiter"
        ui.add_space(4.0);
        let mut job = egui::text::LayoutJob::default();
        job.append(&format!("{prefix}: "), 0.0, fmt(color));
        job.append(&diag.message, 0.0, fmt(source_color));
        ui.label(job);

        // Source context with underline.
        if let Some(range) = &diag.span_range {
            if range.start >= preamble_len {
                let user_start = range.start - preamble_len;
                let user_end = (range.end - preamble_len).min(user_source.len());
                let err_line = byte_to_line(user_start);
                let line_num = err_line + 1;
                let gutter_width = format!("{line_num}").len().max(3);
                let text = line_text(err_line);

                // "    ┃" (empty gutter separator)
                let mut bar_job = egui::text::LayoutJob::default();
                bar_job.append(&format!("{:>gutter_width$} ┃", ""), 0.0, fmt(line_num_color));
                label_no_wrap(ui, bar_job);

                // "  5 ┃ <source line>"
                let mut line_job = egui::text::LayoutJob::default();
                line_job.append(&format!("{line_num:>gutter_width$} ┃ "), 0.0, fmt(line_num_color));
                line_job.append(text, 0.0, fmt(source_color));
                label_no_wrap(ui, line_job);

                // "    ┃     ───" underline pointing at the error span
                let line_start = line_starts[err_line];
                let col_start = user_start - line_start;
                let col_end = (user_end - line_start).max(col_start + 1);
                let underline: String = " ".repeat(col_start)
                    + &"─".repeat(col_end - col_start);
                let mut ul_job = egui::text::LayoutJob::default();
                ul_job.append(&format!("{:>gutter_width$} ┃ ", ""), 0.0, fmt(line_num_color));
                ul_job.append(&underline, 0.0, fmt(color));
                label_no_wrap(ui, ul_job);
            }
        }

        // Hints.
        for hint in &diag.hints {
            let mut hint_job = egui::text::LayoutJob::default();
            hint_job.append("  hint: ", 0.0, fmt(hint_color));
            hint_job.append(hint, 0.0, fmt(source_color));
            ui.label(hint_job);
        }
    }

    ui.spacing_mut().item_spacing.y = prev_spacing;
}

fn render_typst(ui: &mut egui::Ui, state: &mut TypstState, source: &str, preamble_len: usize) {
    state.world.set_source(source.to_string());

    let doc = match state.world.compile() {
        Ok(doc) => doc,
        Err(diags) => {
            render_typst_errors(ui, source, preamble_len, &diags);
            return;
        }
    };

    let text_color = ui.visuals().text_color();
    let pixels_per_point = ui.ctx().pixels_per_point();

    for page in doc.pages.iter() {
        let (shapes, size, text_layout) = painter::render_frame_to_shapes(
            &page.frame,
            &mut state.glyph_cache,
            text_color,
            pixels_per_point,
        );

        let (rect, response) =
            ui.allocate_exact_size(size, egui::Sense::click_and_drag());

        if !ui.is_rect_visible(rect) {
            continue;
        }

        let offset = rect.min.to_vec2();
        let chars = &text_layout.chars;
        let has_text = !chars.is_empty();

        // ── Selection state ────────────────────────────────────────
        let sel_id = response.id;
        let mut sel = ui
            .data_mut(|d| d.get_temp::<TypstSelection>(sel_id))
            .unwrap_or_default();

        // Build a frame-local rect from all glyph bounds — used to
        // extend the interactive area beyond the tight Typst frame
        // so drags near the edge don't escape to the scroll area.
        let glyphs_rect = if has_text {
            let mut r = egui::Rect::NOTHING;
            for ch in chars {
                r = r.union(ch.rect);
            }
            // Add generous margin so the user can start/end a drag
            // in the padding around the content.
            r.expand(12.0)
        } else {
            egui::Rect::NOTHING
        };

        let source = state.world.main_source();

        if has_text {
            // Show text cursor when hovering over text content.
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }

            if response.double_clicked() {
                // AST-aware selection: opaque nodes (math, tables) select
                // the whole node; transparent text falls back to word.
                if let Some(pos) = response.interact_pointer_pos() {
                    let frame_pos = pos - offset;
                    if let Some(idx) = nearest_glyph(chars, frame_pos) {
                        let (lo, hi) = double_click_range(source, chars, idx);
                        sel.glyph_override = Some(lo..=hi);
                        sel.anchor = Some(chars[lo].rect.left_center());
                        sel.cursor = Some(chars[hi].rect.right_center());
                    }
                }
            } else if response.drag_started() {
                sel.glyph_override = None;
                if let Some(pos) = response.interact_pointer_pos() {
                    let frame_pos = pos - offset;
                    sel.anchor = Some(frame_pos);
                    sel.cursor = Some(frame_pos);
                }
            } else if response.dragged() {
                if let Some(pos) = ui.ctx().pointer_latest_pos() {
                    // Clamp to glyphs rect so the drag doesn't escape
                    // into surrounding scroll areas.
                    let frame_pos = pos - offset;
                    let clamped = glyphs_rect.clamp(frame_pos);
                    sel.cursor = Some(clamped);
                }
                // Prevent scroll while selecting text.
                ui.ctx().input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
            }

            // Clear selection on single click (not double-click, not drag).
            if response.clicked() && !response.double_clicked() && !response.dragged() {
                sel = TypstSelection::default();
            }
        }

        // ── AST selection ─────────────────────────────────────────
        let glyph_range = sel.range(chars);

        let ast_sel = if let Some(ref range) = glyph_range {
            ast_select(source, chars, range)
        } else {
            AstSelection { units: Vec::new(), single_leaf: false }
        };

        // ── Compute selected glyph set (single source of truth) ────
        //
        // Both highlight and copy are derived from this set,
        // making divergence structurally impossible.
        //
        // Transparent units allow partial selection (only glyphs in
        // the drag range). Opaque units promote: if any glyph is
        // touched, all glyphs in that unit are selected.
        let selected: Vec<bool> = if let Some(ref range) = glyph_range {
            let mut sel_set = vec![false; chars.len()];
            if ast_sel.single_leaf {
                for i in range.clone() {
                    if i < sel_set.len() { sel_set[i] = true; }
                }
            } else {
                for (unit, transparent) in &ast_sel.units {
                    // Opaque: any glyph touched → select all glyphs in unit.
                    // Transparent: only glyphs in the drag range (or all if fully covered).
                    let full = !transparent
                        || all_glyphs_covered(source, chars, unit, range);
                    for (i, ch) in chars.iter().enumerate() {
                        if let Some(r) = source.range(ch.span.0) {
                            if r.start >= unit.start && r.end <= unit.end
                                && (full || range.contains(&i))
                            {
                                sel_set[i] = true;
                            }
                        }
                    }
                }
            }
            sel_set
        } else {
            Vec::new()
        };

        // ── Paint selection highlights (behind text) ───────────────
        {
            let highlight_color = ui.visuals().selection.bg_fill;
            for (i, ch) in chars.iter().enumerate() {
                if *selected.get(i).unwrap_or(&false) {
                    ui.painter().rect_filled(
                        ch.rect.translate(offset), 0.0, highlight_color,
                    );
                }
            }
        }

        // ── Paint text shapes ──────────────────────────────────────
        for mut shape in shapes {
            shape.translate(offset);
            ui.painter().add(shape);
        }

        // ── Copy to clipboard ──────────────────────────────────────
        if glyph_range.is_some() && selected.iter().any(|&s| s) {
            response.request_focus();

            let wants_copy = ui.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Copy))
                    || (i.modifiers.command && i.key_pressed(egui::Key::C))
            });

            if wants_copy {
                let text = if ast_sel.single_leaf {
                    // Single leaf: rendered unicode (character precision).
                    selected.iter().enumerate()
                        .filter(|(_, &s)| s)
                        .filter_map(|(i, _)| chars.get(i).map(|c| c.text.as_str()))
                        .collect()
                } else {
                    // Structural: per-unit source/rendered hybrid.
                    // Fully covered or opaque units → source text (preserves markup).
                    // Partially covered transparent → rendered text of selected glyphs.
                    let mut result = String::new();
                    for (unit, transparent) in &ast_sel.units {
                        if let Some(ref range) = glyph_range {
                            let use_source = !transparent
                                || all_glyphs_covered(source, chars, unit, range);
                            if use_source {
                                result.push_str(&source.text()[unit.clone()]);
                            } else {
                                for (i, ch) in chars.iter().enumerate() {
                                    if selected[i] {
                                        if let Some(r) = source.range(ch.span.0) {
                                            if r.start >= unit.start && r.end <= unit.end {
                                                result.push_str(ch.text.as_str());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    result
                };
                ui.ctx().copy_text(text);
            }
        }

        ui.data_mut(|d| d.insert_temp(sel_id, sel));
    }
}
