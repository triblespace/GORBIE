#!/usr/bin/env -S watchexec -r cargo run --example telemetry_viewer --features telemetry

#[path = "../src/bin/gorbie-telemetry-viewer/app.rs"]
mod app;

use GORBIE::notebook;
use GORBIE::NotebookCtx;

#[notebook]
fn main(nb: &mut NotebookCtx) {
    app::notebook(nb);
}

