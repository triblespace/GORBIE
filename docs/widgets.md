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
