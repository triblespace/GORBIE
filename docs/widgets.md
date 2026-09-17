# Gorbie Widgets

## Feature flags

To keep the default build light, some widgets are feature-gated:
- `markdown`: rich Markdown rendering for `md!` and `note!` (default).
- `polars`: `GORBIE::widgets::dataframe` (Polars + GORBIE table).
- `triblespace`: Triblespace widgets under `GORBIE::widgets::triblespace`.
- `cubecl`: GPU simulated-annealing ordering for the entity inspector (use with `triblespace`).

Without `markdown`, `md!` and `note!` are unavailable. Disable defaults with
`default-features = false`.

`md!` renders Markdown inside a padded card. Use `GORBIE::widgets::markdown`
when you want inline Markdown without padding.

## Sections and grids

`ctx.section(title, |ctx| ...)` is a top-level component of a card: a
collapsible region with a coloured header bar that runs the full card width.
Put the section at the top of the card body and nest the layout inside it,
usually a grid:

```rust
nb.view(|ctx| {
    ctx.section("Parameters", |ctx| {
        ctx.grid(|g| {
            g.place(6, |ctx| { ctx.number(&mut a); });
            g.place(6, |ctx| { ctx.number(&mut b); });
        });
    });
});
```

A section may also wrap one monolithic full-width widget instead of a grid
(a map, a globe, a large table) when the content should not carry the grid's
margins.

Two rules follow from what a section and a card are:

- Never place a section inside a grid cell. A cell insets its content, so
  the header bar gets a border and a margin it is not designed to have; the
  header is meant to touch the card's edges. Grids go inside sections, not
  the other way round.
- One section per card. Every `nb.view` is a cell the reader can detach
  from the notebook and move around the workspace; several sections in one
  cell can only move together. Split a card with several sections into one
  card per section. The section titles carry their own RAL colours, so the
  split costs nothing visually.

## Text fields

`GORBIE::widgets::TextField` supports `rows()` to set the minimum visible height
for multiline inputs and `max_rows()` to cap it. When `max_rows` is set, extra
lines are clipped rather than expanding the widget.

## Headless capture

Notebooks can export cards to PNGs without opening the interactive window.
Run with `--headless` to render each card to `card_0001.png`, `card_0002.png`, ...
in `./gorbie_capture` by default. Override the output directory with `--out-dir`.
Rendering runs fully offscreen (no window is created). Use `--scale` to control the
pixels-per-point (default: 2.0).

## Physics snapshots

`widgets::PhysicsView` holds only orbit/pan/zoom state. Call
`view.show(ui, &PhysicsScene)` with owned sampled geometry. `PhysicsScene`
contains `Line3`, `Particle3`, `Label3`, `LegendEntry`, warnings and an explicit
length-unit label. `view.bounds(Bounds3 { min, max })` supplies a fixed camera
envelope; otherwise the first finite scene is fitted once. Fit/Reset never
advance the simulation. The scale bar is world-unit-aware; wireframes are
intentional x-ray overlays over depth-sorted, true-radius particle disks.

The generic viewer has no physics dependencies. Feature `rapier` adds
`widgets::physics::rapier::scene(&RigidBodySet, &ColliderSet)`; `salva` adds
`widgets::physics::salva::scene(&LiquidWorld)` and `fluid_scene(&Fluid)`.
Combine snapshots with `scene.extend(other_scene)`; different declared units
produce a warning, not a silent conversion. Unsupported Rapier shapes produce
explicit AABB approximations/warnings. Salva boundary points are not included
as fluid. Capture at your chosen real simulation timestep, outside the widget.
See `examples/physics_widgets.rs` and the README for feature commands and limits.

## Triblespace entity inspector

`GORBIE::widgets::triblespace::EntityInspectorWidget` renders an entity graph from a
data `TribleSet` and uses a metadata `TribleSet` for attribute labels and value
formatters. Pass both sets, a `BlobCache` for `UTF8String` attribute names, a
`BlobCache` for `WasmCode` value formatters, and a mutable selection `Id`. The
widget caches its internal graph and will reset the selection to the first
entity if the current `Id` is not present in the data.
Use `EntityOrder::Id` for a stable, deterministic ordering. With the `cubecl`
feature, `EntityOrder::Anneal` runs a GPU simulated annealing pass (starting
from ID order) and updates the layout as it improves. If SA is unavailable, the
widget stays on `EntityOrder::Id`.

## Dataframes (polars)

Enable the `polars` feature to access dataframe widgets.

Core widgets:
- `dataframe(ui, df) -> Result<DataFrame, String>`: interactive SQL + sortable table. Uses
  per-card temp state and returns the query result (or the query error).
- `dataframe_summary(ui, df)`: per-column nulls + quick stats table.
- `data_summary_tiny(ui, active_df, total_df)`: compact “rows × columns” line. Pass the same
  dataframe twice for the unfiltered view.
- `data_export_tiny(ui, df)`: copy/save CSV for a dataframe (typically the active filtered view).

Response:
- Store the `Result` in your own state if you want other cards to react to the current query.
