# GORBIE! (Introduction)
Every other notebook environment tries to make notebooks easier, we try to make it simpler.

Unlike other notebook environments GORBIE! doesn't really come with its own
notebook server or runtime environment. Instead, it is designed to be used as
a library and can be used in any Rust project.

# Getting Started
To get the typical interactive notebook experience, it's recommended to use
GORBIE! together with [`watchexec`](https://github.com/watchexec/watchexec) and [`rust-script`](https://github.com/fornwall/rust-script).
This way you can write your code as a simple `notebook.rs` script and have it
automatically run and update the notebook whenever you save the file.

In such a setup, the `notebook.rs` script would look something like this:

```rust
#!/usr/bin/env watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = "0.1.0"
//! egui = "0.31"
//! ```

use GORBIE::{md, notebook, state, view, Notebook};

fn intro(nb: &mut Notebook) {
    md(nb,
        "# GORBIE!
This is **GORBIE!**, a _minimalist_ notebook environment for **Rust**!

Development is part of the [trible.space](https://trible.space) project.

![an image of 'GORBIE!' the cute alien blob and mascot of this project](./assets/gorbie.png)
");

    let slider = state!(nb, 0.5, |ctx, value| {
        ctx.ui.add(egui::Slider::new(value, 0.0..=1.0).text("input"));
    });

    view!(nb, move |ctx| {
        ctx.ui.add(egui::ProgressBar::new(*slider.read()).text("output"));
    });
}

fn main() {
    notebook!(intro);
}
```

To run this script, you need to have `watchexec` and `rust-script` installed.
You can install them both with `cargo install watchexec-cli rust-script`.

Then you can run the script with `./notebook.rs` and it will automatically
load all dependencies, start the notebook and reload it whenever you make a
change to the script.