![Discord Shield](https://discordapp.com/api/guilds/795317845181464651/widget.png?style=shield)

# GORBIE! (Introduction)
Every other notebook environment tries to make notebooks easier, we try to make them simpler.

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
#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = "0.3.1"
//! ```

use GORBIE::prelude::*;
use GORBIE::cards::DEFAULT_CARD_PADDING;

#[notebook]
fn main(nb: &mut NotebookCtx) {
    nb.view(|ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            md!(
                ui,
                "# GORBIE!
This is **GORBIE!**, a _minimalist_ notebook environment for **Rust**!

Development is part of the [trible.space](https://trible.space) project.

![an image of 'GORBIE!' the cute alien blob and mascot of this project](https://github.com/triblespace/GORBIE/blob/main/assets/gorbie.png?raw=true)
"
            );
        });
    });

    let slider = nb.state("slider", 0.5, |ui, value| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.add(widgets::Slider::new(value, 0.0..=1.0).text("input"));
        });
    });

    nb.view(move |ui| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            let value = slider.read(ui.store());
            ui.add(widgets::ProgressBar::new(*value).text("output"));
        });
    });
}
```

To run this script, you need to have `watchexec` and `rust-script` installed.
You can install them both with `cargo install watchexec-cli rust-script`.

Then you can run the script with `./notebook.rs` and it will automatically
load all dependencies, start the notebook and reload it whenever you make a
change to the script.

# Community

If you have any questions or want to chat about graph databases hop into our [discord](https://discord.gg/UWZ35yHzz3).