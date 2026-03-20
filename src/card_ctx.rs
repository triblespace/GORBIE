use std::hash::Hash;
use std::ops::{Deref, DerefMut};

use eframe::egui;

use crate::state;

/// Number of columns in the notebook grid.
pub const GRID_COLUMNS: u32 = 12;

/// Gutter width between grid columns, in pixels.
pub const GRID_GUTTER: f32 = 12.0;

/// Vertical module height for the modular grid, in pixels.
///
/// Row tops snap to multiples of this value (relative to the grid origin),
/// producing uniform row gaps and field-aligned content. Matches the
/// horizontal gutter so the grid is truly modular (square gutters).
pub const GRID_ROW_MODULE: f32 = 12.0;

/// Edge padding of the grid, in pixels.
///
/// The grid has one gutter-width of padding on each side, so the content
/// area is `768 - 2 * GRID_EDGE_PAD = 744px`.
pub const GRID_EDGE_PAD: f32 = GRID_GUTTER;

/// Width of a single grid column, in pixels.
///
/// Derived from the notebook column width (768px), edge padding, grid
/// column count, and gutter:
/// `(768 - 2 * 12 - (GRID_COLUMNS - 1) * GRID_GUTTER) / GRID_COLUMNS = 51`.
pub const GRID_COL_WIDTH: f32 = 51.0;

/// Pixel width of a span of `n` grid columns (including internal gutters).
pub const fn span_width(n: u32) -> f32 {
    n as f32 * GRID_COL_WIDTH + n.saturating_sub(1) as f32 * GRID_GUTTER
}

/// Card-scoped context that threads the state store through all nested layouts.
///
/// `CardCtx` wraps an `egui::Ui` and provides layout methods that pass
/// `&mut CardCtx` (rather than raw `&mut egui::Ui`) to closures, so
/// [`StateId::read`](crate::state::StateId::read) and
/// [`StateId::read_mut`](crate::state::StateId::read_mut) work at any nesting
/// depth.
///
/// All layout methods shadow their `egui::Ui` counterparts. Because owned
/// methods take priority over `Deref` methods, calling `ctx.horizontal(…)`
/// always invokes the GORBIE version automatically.
pub struct CardCtx<'a> {
    ui: &'a mut egui::Ui,
    store: &'a state::StateStore,
}

impl<'a> CardCtx<'a> {
    pub(crate) fn new(ui: &'a mut egui::Ui, store: &'a state::StateStore) -> Self {
        Self { ui, store }
    }

    pub fn store(&self) -> &state::StateStore {
        self.store
    }

    /// Access the underlying `egui::Ui` directly.
    ///
    /// Prefer the `CardCtx` layout methods — this escape hatch is for
    /// egui APIs that `CardCtx` does not yet wrap.
    pub fn ui_mut(&mut self) -> &mut egui::Ui {
        self.ui
    }

    // ── Grid-aware widgets ──────────────────────────────────────────

    /// Add a GORBIE button.
    ///
    /// This shadows `egui::Ui::button` via method resolution, so
    /// `ctx.button("text")` always produces a GORBIE button.
    pub fn button(&mut self, text: impl Into<egui::WidgetText>) -> egui::Response {
        self.ui.add(crate::widgets::Button::new(text))
    }

    /// GORBIE-styled toggle button (checkbox-like, latching on/off).
    pub fn toggle(&mut self, on: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response {
        self.ui.add(crate::widgets::Button::new(text).on(on))
    }

    /// GORBIE-styled slider.
    pub fn slider<Num: egui::emath::Numeric>(
        &mut self,
        value: &mut Num,
        range: std::ops::RangeInclusive<Num>,
    ) -> egui::Response {
        self.ui.add(crate::widgets::Slider::new(value, range))
    }

    /// GORBIE-styled number field (LCD-style drag/edit).
    pub fn number<Num: egui::emath::Numeric>(
        &mut self,
        value: &mut Num,
    ) -> egui::Response {
        self.ui.add(crate::widgets::NumberField::new(value))
    }

    /// GORBIE-styled single-line text field (LCD-style).
    pub fn text_field(&mut self, text: &mut dyn egui::TextBuffer) -> egui::Response {
        self.ui.add(crate::widgets::TextField::singleline(text))
    }

    /// GORBIE-styled progress bar.
    pub fn progress(&mut self, fraction: f32) -> egui::Response {
        self.ui.add(crate::widgets::ProgressBar::new(fraction))
    }

    /// Render markdown with GORBIE styling and syntax themes.
    pub fn markdown(&mut self, text: &str) {
        crate::widgets::markdown(self.ui, text);
    }

    /// Render a full Typst document string with GORBIE styling (RAL colors,
    /// IosevkaGorbie font, page width matching the card, blue links).
    #[cfg(feature = "typst")]
    pub fn typst(&mut self, source: &str) {
        crate::widgets::typst_widget::typst_with_preamble(self.ui, source);
    }

    /// Render an inline math expression via Typst: `$<expr>$`.
    #[cfg(feature = "typst")]
    pub fn typst_math_inline(&mut self, expr: &str) {
        crate::widgets::typst_widget::typst_math_inline(self.ui, expr);
    }

    /// Render a display-mode math expression via Typst.
    #[cfg(feature = "typst")]
    pub fn typst_math_display(&mut self, expr: &str) {
        crate::widgets::typst_widget::typst_math_display(self.ui, expr);
    }

    // ── Layout wrappers ──────────────────────────────────────────────

    /// Apply padding around the contents while preserving `CardCtx`.
    ///
    /// This is the `CardCtx`-aware equivalent of [`cards::with_padding`].
    pub fn with_padding<R>(
        &mut self,
        padding: impl Into<egui::Margin>,
        add_contents: impl FnOnce(&mut CardCtx<'_>) -> R,
    ) -> egui::InnerResponse<R> {
        let store = self.store;
        egui::Frame::new()
            .inner_margin(padding)
            .show(self.ui, |ui| {
                ui.set_width(ui.available_width());
                let mut ctx = CardCtx::new(ui, store);
                add_contents(&mut ctx)
            })
    }

    pub fn horizontal<R>(
        &mut self,
        add_contents: impl FnOnce(&mut CardCtx<'_>) -> R,
    ) -> egui::InnerResponse<R> {
        let store = self.store;
        self.ui.horizontal(|ui| {
            let mut ctx = CardCtx::new(ui, store);
            add_contents(&mut ctx)
        })
    }

    pub fn horizontal_wrapped<R>(
        &mut self,
        add_contents: impl FnOnce(&mut CardCtx<'_>) -> R,
    ) -> egui::InnerResponse<R> {
        let store = self.store;
        self.ui.horizontal_wrapped(|ui| {
            let mut ctx = CardCtx::new(ui, store);
            add_contents(&mut ctx)
        })
    }

    pub fn vertical<R>(
        &mut self,
        add_contents: impl FnOnce(&mut CardCtx<'_>) -> R,
    ) -> egui::InnerResponse<R> {
        let store = self.store;
        self.ui.vertical(|ui| {
            let mut ctx = CardCtx::new(ui, store);
            add_contents(&mut ctx)
        })
    }

    pub fn with_layout<R>(
        &mut self,
        layout: egui::Layout,
        add_contents: impl FnOnce(&mut CardCtx<'_>) -> R,
    ) -> egui::InnerResponse<R> {
        let store = self.store;
        self.ui.with_layout(layout, |ui| {
            let mut ctx = CardCtx::new(ui, store);
            add_contents(&mut ctx)
        })
    }

    pub fn push_id<R>(
        &mut self,
        id_salt: impl Hash,
        add_contents: impl FnOnce(&mut CardCtx<'_>) -> R,
    ) -> egui::InnerResponse<R> {
        let store = self.store;
        self.ui.push_id(id_salt, |ui| {
            let mut ctx = CardCtx::new(ui, store);
            add_contents(&mut ctx)
        })
    }

    pub fn collapsing<R>(
        &mut self,
        heading: impl Into<egui::WidgetText>,
        add_contents: impl FnOnce(&mut CardCtx<'_>) -> R,
    ) -> egui::CollapsingResponse<R> {
        let store = self.store;
        self.ui.collapsing(heading, |ui| {
            let mut ctx = CardCtx::new(ui, store);
            add_contents(&mut ctx)
        })
    }

    pub fn scope<R>(
        &mut self,
        add_contents: impl FnOnce(&mut CardCtx<'_>) -> R,
    ) -> egui::InnerResponse<R> {
        let store = self.store;
        self.ui.scope(|ui| {
            let mut ctx = CardCtx::new(ui, store);
            add_contents(&mut ctx)
        })
    }

    pub fn indent<R>(
        &mut self,
        id_salt: impl Hash,
        add_contents: impl FnOnce(&mut CardCtx<'_>) -> R,
    ) -> egui::InnerResponse<R> {
        let store = self.store;
        self.ui.indent(id_salt, |ui| {
            let mut ctx = CardCtx::new(ui, store);
            add_contents(&mut ctx)
        })
    }

    pub fn group<R>(
        &mut self,
        add_contents: impl FnOnce(&mut CardCtx<'_>) -> R,
    ) -> egui::InnerResponse<R> {
        let store = self.store;
        self.ui.group(|ui| {
            let mut ctx = CardCtx::new(ui, store);
            add_contents(&mut ctx)
        })
    }

    // ── Grid layout ──────────────────────────────────────────────────

    /// Place elements on the 12-column grid.
    ///
    /// The closure receives a [`Grid`] with [`place`](Grid::place) and
    /// [`skip`](Grid::skip) methods. Elements flow left-to-right and
    /// wrap to the next row automatically when spans would exceed 12.
    ///
    /// Pixel widths are derived entirely from constants — same spans
    /// produce identical widths in every card (coordination-free).
    ///
    /// ```ignore
    /// ctx.grid(|g| {
    ///     g.place(12, |ctx| { ctx.heading("Title"); });     // full width
    ///     g.place(3, |ctx| { ctx.paragraph("text"); });     // 3 cols
    ///     g.place(9, |ctx| { ctx.image(path); });           // fills row
    ///     g.place(6, |ctx| { chart(ctx); });                // next row
    ///     g.skip(6);                                        // furniture
    /// });
    /// ```
    pub fn grid(&mut self, build: impl FnOnce(&mut Grid<'_, '_>)) {
        let store = self.store;
        let left = self.ui.cursor().min.x + GRID_EDGE_PAD;
        let top = self.ui.cursor().min.y + GRID_EDGE_PAD;
        let mut g = Grid {
            ui: self.ui,
            store,
            left,
            grid_top: top,
            cursor: 0,
            row_top: top,
            row_max_bottom: top,
        };
        build(&mut g);
        // Advance the parent Ui past all rows we painted.
        g.finish();
    }
}

/// Response from [`CardCtx::float`].
pub struct FloatResponse {
    /// `true` if the user clicked the drag handle to dismiss the float.
    pub closed: bool,
}

impl<'a> CardCtx<'a> {
    /// Spawn a floating card at the current mouse position (on first show).
    ///
    /// The float renders as a GORBIE card with drag handle and shadow, hovering
    /// above the notebook. Clicking the drag handle dismisses it (returns
    /// `closed = true`). Dragging the handle repositions it.
    ///
    /// Use [`push_id`](Self::push_id) to give each float a unique identity
    /// when creating multiple floats in a loop.
    ///
    /// ```ignore
    /// for page in &state.pages {
    ///     ctx.push_id(page.id, |ctx| {
    ///         let resp = ctx.float(|ctx| {
    ///             ctx.markdown(&page.content);
    ///         });
    ///         if resp.closed {
    ///             close_page(page.id);
    ///         }
    ///     });
    /// }
    /// ```
    #[track_caller]
    pub fn float(
        &mut self,
        add_contents: impl FnOnce(&mut CardCtx<'_>),
    ) -> FloatResponse {
        let float_id = self.ui.id().with("gorbie_float");
        let initial_pos = self.ui.ctx().input(|i| {
            i.pointer.hover_pos().unwrap_or(egui::pos2(100.0, 100.0))
        });

        let card_width = crate::NOTEBOOK_COLUMN_WIDTH;
        let store = self.store;
        let mut add_contents = Some(add_contents);

        let resp = crate::floating::show_floating_card(
            self.ui.ctx(),
            float_id,
            initial_pos,
            card_width,
            0.0,
            store,
            "Close",
            &mut |ctx| {
                if let Some(f) = add_contents.take() {
                    f(ctx);
                }
            },
        );

        FloatResponse { closed: resp.handle_clicked }
    }
}

impl<'a> state::StateAccess for CardCtx<'a> {
    fn store(&self) -> &state::StateStore {
        self.store
    }
}

impl<'a> Deref for CardCtx<'a> {
    type Target = egui::Ui;

    fn deref(&self) -> &Self::Target {
        self.ui
    }
}

impl<'a> DerefMut for CardCtx<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ui
    }
}

// ── Grid ─────────────────────────────────────────────────────────────

/// Flat grid layout for a card.
///
/// Created by [`CardCtx::grid`]. Call [`place`](Self::place) to add
/// content spanning grid columns, [`skip`](Self::skip) to add furniture.
///
/// Elements flow left-to-right. When a placement would exceed 12
/// columns, the current row is finalized and a new one begins.
///
/// Each cell is rendered immediately into a precisely positioned rect —
/// no closure buffering, no lifetime constraints beyond the current call.
pub struct Grid<'ui, 'store> {
    ui: &'ui mut egui::Ui,
    store: &'store state::StateStore,
    /// Left edge of the grid (pixel x).
    left: f32,
    /// Top of the entire grid (pixel y), used as origin for vertical snapping.
    grid_top: f32,
    /// Current column within the row (0..GRID_COLUMNS).
    cursor: u32,
    /// Top of the current row (pixel y).
    row_top: f32,
    /// Tallest cell bottom in the current row.
    row_max_bottom: f32,
}

impl<'ui, 'store> Grid<'ui, 'store> {
    // ── Named spans ────────────────────────────────────────────────
    //
    // These are the clean divisions of the 12-column grid.
    // Use these instead of raw column counts to stay on the grid.

    /// Full-width cell (12 columns, 744px).
    pub fn full(&mut self, f: impl FnOnce(&mut CardCtx<'_>)) { self.place(12, f); }

    /// Three-quarter cell (9 columns, 555px).
    pub fn three_quarters(&mut self, f: impl FnOnce(&mut CardCtx<'_>)) { self.place(9, f); }

    /// Two-thirds cell (8 columns, 492px).
    pub fn two_thirds(&mut self, f: impl FnOnce(&mut CardCtx<'_>)) { self.place(8, f); }

    /// Half-width cell (6 columns, 366px).
    pub fn half(&mut self, f: impl FnOnce(&mut CardCtx<'_>)) { self.place(6, f); }

    /// One-third cell (4 columns, 240px).
    pub fn third(&mut self, f: impl FnOnce(&mut CardCtx<'_>)) { self.place(4, f); }

    /// Quarter-width cell (3 columns, 177px).
    pub fn quarter(&mut self, f: impl FnOnce(&mut CardCtx<'_>)) { self.place(3, f); }

    // ── Named skips ─────────────────────────────────────────────────

    /// Skip a half-width gap (6 columns).
    pub fn skip_half(&mut self) { self.skip(6); }

    /// Skip a third-width gap (4 columns).
    pub fn skip_third(&mut self) { self.skip(4); }

    /// Skip a quarter-width gap (3 columns).
    pub fn skip_quarter(&mut self) { self.skip(3); }

    // ── Low-level ──────────────────────────────────────────────────

    /// Place content spanning an arbitrary number of grid columns.
    ///
    /// Prefer the named methods ([`full`](Self::full), [`half`](Self::half),
    /// [`third`](Self::third), [`quarter`](Self::quarter),
    /// [`two_thirds`](Self::two_thirds), [`three_quarters`](Self::three_quarters))
    /// to stay on the grid. This escape hatch exists for unusual layouts.
    pub fn place(
        &mut self,
        span: u32,
        add_contents: impl FnOnce(&mut CardCtx<'_>),
    ) {
        assert!(
            span > 0 && span <= GRID_COLUMNS,
            "span must be 1..={GRID_COLUMNS}, got {span}"
        );

        // Advance to new row if the previous row was completed or
        // this span doesn't fit.
        if self.needs_advance(span) {
            self.new_row();
        }

        let x = self.left + col_x(self.cursor);
        let width = span_width(span);

        let cell_rect = egui::Rect::from_min_size(
            egui::pos2(x, self.row_top),
            egui::vec2(width, f32::MAX),
        );

        let store = self.store;
        // Use new_child (not scope_builder) so cells don't advance the
        // parent cursor — finish() handles that in one shot.
        let mut child = self.ui.new_child(
            egui::UiBuilder::new().max_rect(cell_rect),
        );
        child.set_width(width);
        let mut ctx = CardCtx::new(&mut child, store);
        add_contents(&mut ctx);

        let used_bottom = child.min_rect().bottom();
        if used_bottom > self.row_max_bottom {
            self.row_max_bottom = used_bottom;
        }

        self.cursor += span;
        if self.cursor >= GRID_COLUMNS {
            self.cursor = 0;
        }
    }

    /// Skip columns (typographic furniture / blank space).
    ///
    /// Common patterns: `skip_quarter()`, `skip_third()`, `skip_half()`.
    pub fn skip(&mut self, span: u32) {
        assert!(
            span > 0 && span <= GRID_COLUMNS,
            "skip must be 1..={GRID_COLUMNS}, got {span}"
        );

        if self.needs_advance(span) {
            self.new_row();
        }

        self.cursor += span;
        if self.cursor >= GRID_COLUMNS {
            self.cursor = 0;
        }
    }

    /// Check if we need to advance to a new row before placing `span` columns.
    fn needs_advance(&self, span: u32) -> bool {
        // Previous row completed (cursor reset to 0) with content above.
        let pending_complete = self.cursor == 0 && self.row_max_bottom > self.row_top;
        // Span doesn't fit current row.
        let overflow = self.cursor > 0 && self.cursor + span > GRID_COLUMNS;
        pending_complete || overflow
    }

    /// Start a new row, snapping the y position to the next vertical module.
    fn new_row(&mut self) {
        let raw = self.row_max_bottom;
        // Snap to the vertical grid so rows are field-aligned.
        let rel = raw - self.grid_top;
        self.row_top = self.grid_top + (rel / GRID_ROW_MODULE).ceil() * GRID_ROW_MODULE;
        self.row_max_bottom = self.row_top;
        self.cursor = 0;
    }

    /// Advance the parent Ui's cursor past all grid content.
    fn finish(&mut self) {
        // Don't snap the last row — just add bottom padding.
        // Inter-row snapping is handled by new_row().
        let total_height = (self.row_max_bottom + GRID_EDGE_PAD - self.ui.cursor().min.y).max(0.0);
        if total_height > 0.0 {
            self.ui.allocate_space(egui::vec2(
                2.0 * GRID_EDGE_PAD + span_width(GRID_COLUMNS),
                total_height,
            ));
        }
    }
}

/// Pixel x-offset of grid column `col` from the left edge.
const fn col_x(col: u32) -> f32 {
    col as f32 * (GRID_COL_WIDTH + GRID_GUTTER)
}
