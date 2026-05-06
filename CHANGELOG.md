# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

## 0.13.1 - 2026-05-07

- **RUSTSEC patch-bump four transitive deps in Cargo.lock**:
  - `thin-vec 0.2.14 → 0.2.16` (GHSA-xphw-cqx3-667j, high) —
    Use-After-Free + Double Free in `IntoIter::drop` when an
    element's `Drop` panics. Pulled in via `typst-utils`.
  - `quinn-proto 0.11.13 → 0.11.14` (GHSA-6xvm-j4wr-6v98, high) —
    Unauthenticated remote DoS via panic in QUIC transport
    parameter parsing. Stale lockfile entry — not in the active
    dep graph but tracked in `Cargo.lock`.
  - `rand 0.8.5 → 0.8.6` (GHSA-cq8v-f236-94qc, low) — Unsoundness
    with custom logger using `rand::rng()`. Pulled in via
    `ashpd` (dark-light) and `lipsum` (typst-library).
  - `rand 0.9.2 → 0.9.3` — Same advisory, 0.9 line. Pulled in
    via `cubecl-common`.
  All four are pure patch bumps within existing semver ranges;
  no Cargo.toml changes needed.

## 0.13.0 - 2026-05-07

- **Bump optional `triblespace` dep to 0.37.** Telemetry +
  pile-inspector examples track the new release. The bump is
  the headline reason this is a minor (rather than patch)
  release — pre-1.0, but breaking for downstreams that pin to
  `triblespace 0.36`.
- **Bump egui family to 0.34.x latest** (`egui_plot` 0.34 →
  0.35). Picks up the upstream `hit_test.rs:365` fix that
  prevented `Sense::click_and_drag` on widgets adjacent to
  click-sensors — see float / drag-sense changes below.
- **Bump `rustls-webpki` 0.103.9 → 0.103.13** (RUSTSEC
  advisories).
- **Stacked floats no longer drag in lockstep.** Float handles
  use `Sense::click_and_drag` and read `dragged()` /
  `drag_delta()` directly — egui's drag sense is z-aware, so a
  drag-start event is consumed by exactly the topmost handle
  under the pointer. The 0.34 migration's `hit_test.rs:365`
  panic that prompted manual pointer tracking has been fixed
  upstream in 0.34.x; ~25 lines of memory-id bookkeeping gone.
- **Floating cards render at natural content height.** Tall
  floats (multi-page wiki fragments, long compass goal lists)
  no longer clip at a fixed viewport-height cap. `max_rect.max.y`
  is set to `min_y + min_height` so `available_height()` is a
  finite floor, but `min_rect` grows freely past it via
  `allocate_space(...)` — so `inner.response.rect.height()` ends
  up as the actual natural content height, regardless of how
  tall the body is, while still preventing fill-available
  widgets (transitive ScrollArea, vertical layouts) from seeing
  `f32::INFINITY` and reporting runaway heights.
- **Fix infinite-scroll feedback when a content-anchored float
  is open.** The notebook's scroll-content extension was
  comparing `frame_rect.bottom()` (screen coords, drifts with
  scroll position) against `float_bottom` (content coords,
  fixed) — subtracting the two mixed coordinate systems and
  grew linearly with scroll position, so each frame the user
  scrolled, the allocated scroll content expanded, which let
  them scroll further next frame. Compare *heights* instead
  (`frame_rect.height()` vs `float_bottom`); both are
  coord-system-independent extents. Wiki and compass floats
  triggered this; activity timeline (no content-anchored
  floats) was unaffected.

## 0.12.0 - 2026-04-19
- **Central panel background** restored after the egui 0.34 migration —
  `fn ui` now receives a bare `&mut Ui` with no default frame/fill, so
  the notebook wraps its body in `Frame::central_panel(&ctx.global_style())`.
- **Drag-to-scroll disabled** on the main notebook ScrollArea. egui's
  hit-test panics (`hit_test.rs:365`) when a big drag-sensing ScrollArea
  coexists with nearby click-sensing widgets — which is every interactive
  card in a notebook. Users still scroll via scroll bars + mouse wheel.
- **Floating card drag handle** uses `Sense::click()` with manual drag
  detection via `pointer.primary_down` + `pointer.hover_pos`, same
  sidestep around the egui hit-test bug.

## 0.9.13 - 2026-04-05
- Switch `telemetry` to the shared `triblespace::telemetry` implementation and
  use `TELEMETRY_*` environment variables.
- Upgrade optional `triblespace` dependency to `0.34.1`.
- Update telemetry viewer to rely on generic shared telemetry fields (no
  `card_index` dependency).
- Refresh the TribleSpace examples for current `Checkout` and `NsTAIInterval`
  APIs, and drop the removed `Blake2b` schema from `schema_inspector`.
- Rename telemetry viewer binary to `telemetry-viewer`.

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
