#[cfg(not(feature = "telemetry"))]
fn main() {
    eprintln!(
        "gorbie-telemetry-viewer requires the `telemetry` feature.\n\n\
Try:\n  cargo run --bin gorbie-telemetry-viewer --features telemetry"
    );
    std::process::exit(2);
}

#[cfg(feature = "telemetry")]
mod app;

#[cfg(feature = "telemetry")]
use GORBIE::notebook;
#[cfg(feature = "telemetry")]
use GORBIE::NotebookCtx;

#[cfg(feature = "telemetry")]
#[notebook(name = "Gorbie telemetry viewer")]
fn telemetry_viewer(nb: &mut NotebookCtx) {
    app::notebook(nb);
}

#[cfg(feature = "telemetry")]
fn main() {
    telemetry_viewer();
}

