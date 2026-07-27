#[cfg(not(feature = "telemetry"))]
fn main() {
    eprintln!(
        "telemetry-viewer requires the `telemetry` feature.\n\n\
Try:\n  cargo run --bin telemetry-viewer --features telemetry,plots"
    );
    std::process::exit(2);
}

#[cfg(feature = "telemetry")]
mod app;

#[cfg(feature = "telemetry")]
mod axis;

#[cfg(feature = "telemetry")]
use GORBIE::notebook;
#[cfg(feature = "telemetry")]
use GORBIE::NotebookCtx;

#[cfg(feature = "telemetry")]
#[notebook(name = "Tracing telemetry viewer")]
fn telemetry_viewer(nb: &mut NotebookCtx) {
    app::notebook(nb);
}

#[cfg(feature = "telemetry")]
fn main() {
    telemetry_viewer();
}
