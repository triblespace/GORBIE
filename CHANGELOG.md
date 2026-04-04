# Changelog

All notable changes to this project will be documented in this file.

## 0.8.2 - 2026-03-16
- **Collapsed detached placeholders**: detached cards leave a slim 12px
  (GRID_ROW_MODULE) hatched strip instead of reserving full card height.
- **Grid-aligned card chrome**: detach/open-in-editor tab buttons (24px),
  drag handle (12px), and handle stripes all snap to the grid module.

## 0.8.1 - 2026-03-16
- **Clickable Typst links**: `#link()` and auto-detected URLs are now
  interactive — pointer changes to a hand cursor on hover and click opens
  the URL. Link text renders in RAL signal blue.
- Links demo card in `grid_demo` example.

## 0.8.0 - 2026-03-16
- **Typst integration** (`typst` feature): compile Typst markup and render as
  vector geometry directly on egui's Painter. Supports math (inline/display),
  full documents, shapes, dash patterns, text stroke, even-odd fill rule, and
  the RAL color palette. Includes AST-aware text selection, copy, and
  double-click (opacity transition for structure, paragraph for text).
  Grid constants (`grid-span`, `grid-gutter`, etc.) available in the Typst
  preamble for grid-aligned column layouts. Compilation errors render inline
  as rustc-style diagnostics with source context and hints.
- **CardCtx convenience methods**: `slider()`, `number()`, `text_field()`,
  `toggle()`, `small_button()`, `progress()` — shadows egui defaults with
  GORBIE-styled widgets.
- **Grid-native widget sizing**: NumberField and TextField snap to
  `2 * GRID_ROW_MODULE` (24px) minimum height instead of egui's default.
- 12-column modular grid with named spans (`full`, `half`, `third`, `quarter`,
  `two_thirds`, `three_quarters`) and skip/furniture helpers.
- Grid-aligned markdown heading sizes for IosevkaGorbie.
- Widget showcase and visual grid reference in `grid_demo` example.

## Unreleased
- Switch `telemetry` to the shared `triblespace::telemetry` implementation and
  use `TELEMETRY_*` environment variables.
- Bump optional `triblespace` dependency to `0.34.1`.
- Update telemetry viewer to rely on generic shared telemetry fields (no
  `card_index` dependency).
- Rename telemetry viewer binary to `telemetry-viewer`.

## 0.5.1 - 2026-01-22
- Fix detached card dragging across the margin by deferring anchor layer switches.

## 0.5.2 - 2026-02-04
- Switch to crates.io dependencies for gorbie-macros, gorbie-commonmark, and triblespace 0.9.0.

## 0.5.3 - 2026-02-08
- Bump optional `triblespace` dependency to `0.11.0`.

## 0.5.0 - 2026-01-22
- Bump crate version to 0.5.0.
- Pin optional `triblespace` dependency to `0.8.0` for publishing.
- Update README snippets to `GORBIE = "0.5.0"`.
