use std::cell::RefCell;
use std::fmt::Write;
use std::sync::LazyLock;

use eframe::egui;

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
/// Sets page width to the available UI width, zero margins,
/// body text size from the UI style, and IosevkaGorbie as the default font.
/// Intended to be called inside a grid cell (the grid handles padding).
pub fn typst_with_preamble(ui: &mut egui::Ui, content: &str) {
    let width = ui.available_width();
    let body_size = ui.style()
        .text_styles
        .get(&egui::TextStyle::Body)
        .map_or(15.0, |d| d.size);
    let palette = ral_preamble(ui);

    let source = format!(
        "{palette}\
         #set page(width: {width}pt, height: auto, margin: 0pt)\n\
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
/// Automatically sets page width to the grid content width,
/// IosevkaGorbie as the body font, and the current body text size.
/// Uses a full-width grid cell for padding.
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
            $ctx.grid(|g| g.full(|ctx| {
                $crate::widgets::typst_widget::typst_with_preamble(ctx, &text);
            }));
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

    // Render all pages (typically just one for inline/display math).
    for page in doc.pages.iter() {
        let (shapes, size) = painter::render_frame_to_shapes(
            &page.frame,
            &mut state.glyph_cache,
            text_color,
            pixels_per_point,
        );

        let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            let offset = rect.min.to_vec2();
            for mut shape in shapes {
                shape.translate(offset);
                ui.painter().add(shape);
            }
        }
    }
}
