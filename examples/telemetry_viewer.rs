#!/usr/bin/env -S watchexec -r cargo run --example telemetry_viewer --features telemetry

#[path = "../src/bin/telemetry-viewer/app.rs"]
mod app;

use GORBIE::NotebookCtx;
use GORBIE::notebook;

#[notebook]
fn main(nb: &mut NotebookCtx) {
    app::notebook(nb);
}
