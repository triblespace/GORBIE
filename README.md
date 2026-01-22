![Discord Shield](https://discordapp.com/api/guilds/795317845181464651/widget.png?style=shield)

# GORBIE! - A Minimalist Notebook Environment for Rust

Every other notebook environment tries to make notebooks easier, we try to make them simpler.

![GORBIE screenshot](https://github.com/triblespace/GORBIE/blob/main/assets/screenshot.png?raw=true)

## Core Ideas
A notebook is just Rust. By being fully native you can visualize huge datasets,
build complex UIs, and leverage the entire Rust ecosystem without being forced to
shoehorn everyting into a web browser, JavaScript and serialized JSON.

This is a library, not a server. Your notebook lives in your Rust project,
runs in-process with your existing dependencies. No separate server, no custom
file format, no sync step - just Rust and an egui window when you want it.

We don't ship yet another editor. Most developers already have a
well-tuned setup, and notebook tools often spend time re-inventing the wheel
with worse results. We focus on the notebook experience and plug into the
tools you already use.

Immediate-mode: the notebook redraws every frame, and state lives in
`nb.state` handles. This makes it easy to build interactive UIs that are extremely
robust and responsive to user input without complex reactivity systems.

Interactive development stays simple: we re-run the notebook on each change,
not hot-reload. Rust's incremental compilation keeps that fast enough to feel
live.

# Getting Started
For development, use a normal Cargo project so your IDE can index GORBIE! and
provide full static analysis.

Add the dependency and drop in a `main`:

```toml
# Cargo.toml
[dependencies]
GORBIE = "0.5.0"
```

```rust
// src/main.rs
use GORBIE::prelude::*;
use GORBIE::cards::DEFAULT_CARD_PADDING;

#[notebook]
fn main(nb: &mut NotebookCtx) {
    nb.view(|ui| {
        md!(
            ui,
            "# GORBIE!
This is **GORBIE!**, a _minimalist_ notebook environment for **Rust**!

Development is part of the [trible.space](https://trible.space) project.

![an image of 'GORBIE!' the cute alien blob and mascot of this project](https://github.com/triblespace/GORBIE/blob/main/assets/gorbie.png?raw=true)
"
        );
    });

    let slider = nb.state("slider", 0.5, |ui, value| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.add(widgets::Slider::new(value, 0.0..=1.0).text("input"));
        });
    });

    nb.view(move |ui| {
        let value = slider.read(ui);
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.add(widgets::ProgressBar::new(*value).text("output"));
        });
    });
}
```

Run it with `cargo run` to start the notebook.

For reload-on-change with Cargo, use:
`watchexec -r -w src -w Cargo.toml -- cargo run`
or `cargo watch -x run` (install with `cargo install cargo-watch`).

## Script Workflow (Quick/Share)
If you want a single-file notebook or quick distribution, use
[`watchexec`](https://github.com/watchexec/watchexec) and
[`rust-script`](https://github.com/fornwall/rust-script). It skips IDE support,
but it is handy for sharing.

Install them with `cargo install watchexec-cli rust-script`, then add this
header to `notebook.rs` and paste the same `main` function below it:

```rust
#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = "0.5.0"
//! ```
```

Make the file executable once with `chmod +x notebook.rs`.

Run it with `./notebook.rs` to load dependencies, start the notebook, and
reload on save.

The first run can take a while because Rust needs to compile and cache
dependencies - grab a coffee. Subsequent launches are fast enough that we use
them for interactive editing.

# Editor Integration
GORBIE! does not ship an editor, but it can jump to card sources. Set
`GORBIE_EDITOR` to a command with placeholders `{{file}}`, `{{line}}`, and
`{{column}}`, for example
`GORBIE_EDITOR='code -g {{file}}:{{line}}:{{column}}'` for VS Code. When set, cards show
an open-in-editor tab.

# Examples
See `GORBIE/examples` for larger notebooks and patterns. Most are runnable with
the same `watchexec` + `rust-script` shebang.

For cargo examples:
`cargo run --example polars --features polars`
`cargo run --example pile_inspector --features triblespace`


# Feature Flags
GORBIE! defaults to a lean build with `markdown` enabled. Add extras as needed:
- `markdown`: rich Markdown rendering with `md!` and `note!` (default).
- `code`: syntax-highlighted `code_view`.
- `polars`: dataframe widget (Polars + egui_extras).
- `triblespace`: Triblespace widgets and visualizations.

# Community

If you have any questions or want to chat about Rust notebooks hop into our [discord](https://discord.gg/UWZ35yHzz3).
