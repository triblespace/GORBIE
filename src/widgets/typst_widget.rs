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
        render_typst(ui, &mut state, source);
    });
}

/// Render an inline math expression: `$<expr>$`.
pub fn typst_math_inline(ui: &mut egui::Ui, expr: &str) {
    let fg = typst_rgb(ui.visuals().text_color());
    let size = ui.style()
        .text_styles
        .get(&egui::TextStyle::Body)
        .map_or(15.0, |d| d.size);
    let source = format!(
        "#set page(width: auto, height: auto, margin: 0pt)\n\
         #set text(size: {size}pt, fill: {fg})\n\
         ${expr}$"
    );
    TYPST_STATE.with(|state| {
        let mut state = state.borrow_mut();
        render_typst(ui, &mut state, &source);
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
    let source = format!(
        "#set page(width: auto, height: auto, margin: 0pt)\n\
         #set text(size: {size}pt, fill: {fg})\n\
         $ {expr} $"
    );
    TYPST_STATE.with(|state| {
        let mut state = state.borrow_mut();
        render_typst(ui, &mut state, &source);
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

    let source = format!(
        "{palette}\
         #set page(width: {width}pt, height: auto, margin: (x: {pad}pt, top: 0pt, bottom: {pad}pt))\n\
         #set text(size: {body_size}pt, font: \"IosevkaGorbie\", fill: ral-fg)\n\
         #set table(stroke: ral-fg)\n\
         #set line(stroke: ral-fg)\n\
         #set rect(stroke: ral-fg)\n\
         #set circle(stroke: ral-fg)\n\
         #set ellipse(stroke: ral-fg)\n\
         #set polygon(stroke: ral-fg)\n\
         #set path(stroke: ral-fg)\n\
         {content}"
    );
    TYPST_STATE.with(|state| {
        let mut state = state.borrow_mut();
        render_typst(ui, &mut state, &source);
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

/// The static part of the RAL Typst preamble: full 272-color lookup dictionary
/// plus semantic named aliases. Generated once, reused across all renders.
static RAL_TYPST_STATIC: LazyLock<String> = LazyLock::new(|| {
    let mut s = String::with_capacity(16384);
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

/// Generate a Typst preamble that defines the GORBIE RAL color palette.
///
/// Injects:
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
}

impl TypstSelection {
    /// The 2D selection rectangle, if both anchor and cursor are set.
    fn sel_rect(&self) -> Option<egui::Rect> {
        Some(egui::Rect::from_two_pos(self.anchor?, self.cursor?))
    }

    /// Compute the flat glyph index range from the 2D selection rect.
    fn range(&self, chars: &[painter::PositionedChar]) -> Option<std::ops::RangeInclusive<usize>> {
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

// ── Structural container resolution ──────────────────────────────────

/// Structural context of a glyph in the Typst source tree.
#[derive(Clone, Debug, Default)]
struct SourceContainer {
    /// Byte range of the innermost content block `[...]` (table cell).
    cell_range: Option<Range<usize>>,
    /// Byte range of the `#table(...)` function call.
    table_range: Option<Range<usize>>,
}

/// Walk up the syntax tree from a span to find table/cell containers.
fn resolve_container(source: &Source, span: Span) -> SourceContainer {
    let Some(node) = source.find(span) else {
        return SourceContainer::default();
    };

    let mut cell_range = None;
    let mut table_range = None;
    let mut current = &node;

    loop {
        match current.kind() {
            SyntaxKind::ContentBlock if cell_range.is_none() => {
                // A `[...]` block. It's a table cell if its parent is Args.
                if let Some(parent) = current.parent() {
                    if parent.kind() == SyntaxKind::Args {
                        cell_range = Some(current.range());
                    }
                }
            }
            SyntaxKind::FuncCall => {
                // Check if the first child is an Ident "table".
                for child in current.children() {
                    if child.kind() == SyntaxKind::Ident
                        && child.text().as_str() == "table"
                    {
                        table_range = Some(current.range());
                        break;
                    }
                    // Only check the callee (first meaningful child).
                    if !child.kind().is_trivia() {
                        break;
                    }
                }
                if table_range.is_some() {
                    break;
                }
            }
            _ => {}
        }
        match current.parent() {
            Some(p) => current = p,
            None => break,
        }
    }

    SourceContainer { cell_range, table_range }
}

/// Resolve containers for all positioned chars, returning a parallel vec.
fn resolve_all_containers(
    source: &Source,
    chars: &[painter::PositionedChar],
) -> Vec<SourceContainer> {
    chars.iter().map(|ch| resolve_container(source, ch.span.0)).collect()
}

/// Selection tier determines highlight style and copy behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionTier {
    /// Individual glyph selection (default, or within a single cell).
    Glyphs,
    /// Selection spans multiple cells — highlight whole cells, copy `[...]`.
    Cells,
    /// Selection includes table borders — highlight whole table, copy `#table(...)`.
    Table,
}

/// Determine the selection tier from the containers of selected glyphs.
///
/// Tier is based on how many table cells are covered:
/// - 0-1 cells (or not in a table): `Glyphs`
/// - 2+ cells but not all: `Cells`
/// - All cells of the table: `Table`
fn compute_tier(
    containers: &[SourceContainer],
    selected_indices: &std::ops::RangeInclusive<usize>,
) -> SelectionTier {
    // Collect selected cell ranges and the table range.
    let mut selected_cells: Vec<Range<usize>> = Vec::new();
    let mut table_range: Option<Range<usize>> = None;

    for i in selected_indices.clone() {
        if let Some(c) = containers.get(i) {
            if let Some(ref tr) = c.table_range {
                table_range = Some(tr.clone());
            }
            if let Some(ref cr) = c.cell_range {
                if !selected_cells.contains(cr) {
                    selected_cells.push(cr.clone());
                }
            }
        }
    }

    let Some(ref table_range) = table_range else {
        return SelectionTier::Glyphs;
    };

    if selected_cells.len() <= 1 {
        return SelectionTier::Glyphs;
    }

    // Count total cells in this table to distinguish Cells vs Table.
    let mut all_cells: Vec<Range<usize>> = Vec::new();
    for c in containers {
        if c.table_range.as_ref() == Some(table_range) {
            if let Some(ref cr) = c.cell_range {
                if !all_cells.contains(cr) {
                    all_cells.push(cr.clone());
                }
            }
        }
    }

    if selected_cells.len() >= all_cells.len() {
        SelectionTier::Table
    } else {
        SelectionTier::Cells
    }
}

// ── Table grid computation for visual cell highlights ─────────────────

struct TableGrid {
    /// Rects to paint as selection highlights.
    highlight_rects: Vec<egui::Rect>,
}

/// Build visual cell rects for table selection highlights.
///
/// Computes the table's row/column structure from cell glyph positions,
/// then returns the grid-aligned rects for selected cells (Cells tier)
/// or the full table rect including borders (Table tier).
fn build_table_grid(
    containers: &[SourceContainer],
    chars: &[painter::PositionedChar],
    spans: &[painter::PositionedSpan],
    source: &Source,
    selected: &std::ops::RangeInclusive<usize>,
) -> TableGrid {
    // Find the table range.
    let table_range = match containers.iter().find_map(|c| c.table_range.clone()) {
        Some(tr) => tr,
        None => return TableGrid { highlight_rects: Vec::new() },
    };

    // Collect selected cell ranges.
    let mut selected_cells: Vec<Range<usize>> = Vec::new();
    for i in selected.clone() {
        if let Some(c) = containers.get(i) {
            if c.table_range.as_ref() == Some(&table_range) {
                if let Some(ref cr) = c.cell_range {
                    if !selected_cells.contains(cr) {
                        selected_cells.push(cr.clone());
                    }
                }
            }
        }
    }

    // Collect ALL cells in this table with their glyph bounding rects.
    let mut all_cells: Vec<(Range<usize>, egui::Rect)> = Vec::new();
    for (i, c) in containers.iter().enumerate() {
        if c.table_range.as_ref() != Some(&table_range) {
            continue;
        }
        if let Some(ref cr) = c.cell_range {
            if let Some(ch) = chars.get(i) {
                if let Some(entry) = all_cells.iter_mut().find(|(r, _)| r == cr) {
                    entry.1 = entry.1.union(ch.rect);
                } else {
                    all_cells.push((cr.clone(), ch.rect));
                }
            }
        }
    }

    if all_cells.is_empty() {
        return TableGrid { highlight_rects: Vec::new() };
    }

    let all_selected = selected_cells.len() >= all_cells.len();

    if all_selected {
        // Table tier: full table rect including border shapes.
        let mut table_rect = egui::Rect::NOTHING;
        for (_, r) in &all_cells {
            table_rect = table_rect.union(*r);
        }
        for ps in spans {
            if let Some(r) = source.range(ps.span) {
                if r.start >= table_range.start && r.end <= table_range.end {
                    table_rect = table_rect.union(ps.rect);
                }
            }
        }
        return TableGrid {
            highlight_rects: if table_rect.is_positive() {
                vec![table_rect]
            } else {
                Vec::new()
            },
        };
    }

    // Cells tier: extract actual grid lines from table border shapes.
    // Vertical borders (tall, narrow) give column boundaries.
    // Horizontal borders (wide, short) give row boundaries.
    let mut grid_xs: Vec<f32> = Vec::new(); // vertical line x positions
    let mut grid_ys: Vec<f32> = Vec::new(); // horizontal line y positions
    let mut table_rect = egui::Rect::NOTHING;

    for ps in spans {
        if let Some(r) = source.range(ps.span) {
            if r.start >= table_range.start && r.end <= table_range.end {
                table_rect = table_rect.union(ps.rect);
                let w = ps.rect.width();
                let h = ps.rect.height();
                if h > w * 3.0 && w < 4.0 {
                    // Vertical border → column boundary.
                    let x = ps.rect.center().x;
                    if !grid_xs.iter().any(|&gx| (gx - x).abs() < 1.0) {
                        grid_xs.push(x);
                    }
                } else if w > h * 3.0 && h < 4.0 {
                    // Horizontal border → row boundary.
                    let y = ps.rect.center().y;
                    if !grid_ys.iter().any(|&gy| (gy - y).abs() < 1.0) {
                        grid_ys.push(y);
                    }
                }
            }
        }
    }
    for (_, r) in &all_cells {
        table_rect = table_rect.union(*r);
    }

    grid_xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    grid_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // Build column intervals from consecutive vertical lines.
    // The intervals are: [table_left, x0], [x0, x1], ..., [xN, table_right].
    let mut col_intervals: Vec<(f32, f32)> = Vec::new();
    {
        let mut prev = table_rect.left();
        for &x in &grid_xs {
            col_intervals.push((prev, x));
            prev = x;
        }
        col_intervals.push((prev, table_rect.right()));
    }

    let mut row_intervals: Vec<(f32, f32)> = Vec::new();
    {
        let mut prev = table_rect.top();
        for &y in &grid_ys {
            row_intervals.push((prev, y));
            prev = y;
        }
        row_intervals.push((prev, table_rect.bottom()));
    }

    // For each selected cell, find which grid cell contains its content.
    let mut rects = Vec::new();
    for cell_range in &selected_cells {
        let cell_entry = all_cells.iter().find(|(r, _)| r == cell_range);
        let Some((_, cell_rect)) = cell_entry else { continue };
        let center = cell_rect.center();

        // Find the column interval containing the center.
        let col = col_intervals.iter()
            .find(|(l, r)| center.x >= *l && center.x <= *r);
        let row = row_intervals.iter()
            .find(|(t, b)| center.y >= *t && center.y <= *b);

        if let (Some(&(left, right)), Some(&(top, bottom))) = (col, row) {
            rects.push(egui::Rect::from_min_max(
                egui::pos2(left, top),
                egui::pos2(right, bottom),
            ));
        }
    }

    TableGrid { highlight_rects: rects }
}

// ── Copy helpers ──────────────────────────────────────────────────────

/// Expand a source byte range to include enclosing `$...$` pairs (both
/// sides required) and heading markers (`=`, `==`, …).
///
/// Structural delimiters (`[]`, `()`, `{}`) are NOT expanded here —
/// those are handled by the tiered selection system.
fn expand_markup(src: &str, start: usize, end: usize) -> String {
    let bytes = src.as_bytes();
    let mut lo = start;
    let mut hi = end;

    // Probe left past whitespace for $ or heading =.
    let mut li = lo;
    while li > 0 && matches!(bytes[li - 1], b' ' | b'\n' | b'\r') {
        li -= 1;
    }
    let left_dollar = li > 0 && bytes[li - 1] == b'$';

    // Probe right past whitespace for $.
    let mut ri = hi;
    while ri < bytes.len() && matches!(bytes[ri], b' ' | b'\n' | b'\r') {
        ri += 1;
    }
    let right_dollar = ri < bytes.len() && bytes[ri] == b'$';

    // Include $ only if both delimiters are reachable.
    if left_dollar && right_dollar {
        lo = li - 1;
        hi = ri + 1;
    } else if !left_dollar && li > 0 && bytes[li - 1] == b'=' {
        // Heading markers (=, ==, ===…)
        let mut h = li - 1;
        while h > 0 && bytes[h - 1] == b'=' { h -= 1; }
        if h == 0 || bytes[h - 1] == b'\n' {
            lo = h;
        }
    }

    src[lo..hi].to_string()
}

/// Rendered unicode text for the selected glyph range.
fn rendered_text(
    range: &std::ops::RangeInclusive<usize>,
    chars: &[painter::PositionedChar],
) -> String {
    range
        .clone()
        .filter_map(|i| chars.get(i).map(|c| c.text.as_str()))
        .collect()
}

/// Produce copy text for the current selection tier.
fn copy_selection(
    tier: SelectionTier,
    source: &Source,
    range: &std::ops::RangeInclusive<usize>,
    chars: &[painter::PositionedChar],
    containers: &[SourceContainer],
) -> String {
    match tier {
        SelectionTier::Table => {
            // Full #table(...) source.
            let table_range = containers.iter().find_map(|c| c.table_range.clone());
            if let Some(tr) = table_range {
                // Include the `#` hash before the FuncCall node.
                let start = if tr.start > 0
                    && source.text().as_bytes()[tr.start - 1] == b'#'
                { tr.start - 1 } else { tr.start };
                source.text()[start..tr.end].to_string()
            } else {
                rendered_text(range, chars)
            }
        }
        SelectionTier::Cells => {
            // [cell1], [cell2], … for each selected cell.
            let mut cells: Vec<Range<usize>> = Vec::new();
            for i in range.clone() {
                if let Some(c) = containers.get(i) {
                    if let Some(ref cr) = c.cell_range {
                        if !cells.contains(cr) {
                            cells.push(cr.clone());
                        }
                    }
                }
            }
            if cells.is_empty() {
                rendered_text(range, chars)
            } else {
                cells.iter()
                    .map(|cr| &source.text()[cr.clone()])
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        }
        SelectionTier::Glyphs => {
            // Resolve glyph spans. If all share one span (plain text),
            // use rendered unicode for character-level precision.
            // Otherwise use source markup (formulas, headings).
            let mut min_byte = usize::MAX;
            let mut max_byte = 0usize;
            let mut any_resolved = false;
            let mut single_span = true;
            let mut first_range: Option<Range<usize>> = None;

            for i in range.clone() {
                if let Some(ch) = chars.get(i) {
                    if let Some(r) = source.range(ch.span.0) {
                        min_byte = min_byte.min(r.start);
                        max_byte = max_byte.max(r.end);
                        any_resolved = true;
                        match &first_range {
                            None => first_range = Some(r),
                            Some(first) if *first != r => single_span = false,
                            _ => {}
                        }
                    }
                }
            }

            if single_span {
                return rendered_text(range, chars);
            }
            if any_resolved && min_byte < max_byte {
                expand_markup(source.text(), min_byte, max_byte)
            } else {
                rendered_text(range, chars)
            }
        }
    }
}

fn render_typst(ui: &mut egui::Ui, state: &mut TypstState, source: &str) {
    state.world.set_source(source.to_string());

    let doc = match state.world.compile() {
        Ok(doc) => doc,
        Err(err) => {
            ui.colored_label(egui::Color32::RED, format!("Typst error: {err}"));
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

        if has_text {
            // Show text cursor when hovering over text content.
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }

            if response.drag_started() {
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

            // Clear selection on click without drag.
            if response.clicked() && !response.dragged() {
                sel = TypstSelection::default();
            }
        }

        // ── Tier-aware selection ──────────────────────────────────
        let glyph_range = sel.range(chars);
        let source = state.world.main_source();

        // Resolve structural containers and compute tier lazily.
        let (tier, containers) = if let Some(ref range) = glyph_range {
            let containers = resolve_all_containers(source, chars);
            let tier = compute_tier(&containers, range);
            (tier, containers)
        } else {
            (SelectionTier::Glyphs, Vec::new())
        };

        // ── Paint selection highlights (behind text) ───────────────
        if let Some(ref range) = glyph_range {
            let highlight_color = ui.visuals().selection.bg_fill;
            match tier {
                SelectionTier::Glyphs => {
                    for i in range.clone() {
                        if let Some(ch) = chars.get(i) {
                            let r = ch.rect.translate(offset);
                            ui.painter().rect_filled(r, 0.0, highlight_color);
                        }
                    }
                }
                SelectionTier::Cells | SelectionTier::Table => {
                    let grid = build_table_grid(
                        &containers, chars, &text_layout.spans,
                        source, range,
                    );
                    for r in &grid.highlight_rects {
                        ui.painter().rect_filled(
                            r.translate(offset), 0.0, highlight_color,
                        );
                    }
                }
            }
        }

        // ── Paint text shapes ──────────────────────────────────────
        for mut shape in shapes {
            shape.translate(offset);
            ui.painter().add(shape);
        }

        // ── Copy to clipboard ──────────────────────────────────────
        if let Some(range) = glyph_range {
            // Request focus so we receive keyboard events.
            response.request_focus();

            let wants_copy = ui.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Copy))
                    || (i.modifiers.command && i.key_pressed(egui::Key::C))
            });

            if wants_copy {
                let text = copy_selection(tier, source, &range, chars, &containers);
                ui.ctx().copy_text(text);
            }
        }

        ui.data_mut(|d| d.insert_temp(sel_id, sel));
    }
}
