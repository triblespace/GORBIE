#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! ```

use GORBIE::prelude::*;

#[notebook]
fn main() {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;
    view!(move |ui| {
        ui.with_padding(padding, |ui| {
            md!(
                ui,
                "# GORBIE!
This is **GORBIE!**, a _minimalist_ notebook environment for **Rust**!

It's much closer to a library and a shell script than the heavy environments
that notebooks typically provide. Which makes it much easier to integrate
into your existing projects and workflows.

Development is part of the [trible.space](https://trible.space) project.

![an image of 'GORBIE!' the cute alien blob and mascot of this project](./assets/gorbie.png)

# Intro

```
// This is the main function.
fn main() {{
    // Statements here are executed when the compiled binary is called.

    // Print text to the console.
    println!(\"Hello World!\");
}}
```

Lorem ipsum dolor sit amet, consectetur adipiscing elit.\
Vestibulum commodo purus ac arcu dapibus, quis scelerisque lacus pretium.\
Curabitur convallis ultrices neque. Ut lobortis non urna porttitor faucibus.\
Quisque blandit a urna a malesuada. Proin a convallis ipsum.\
Aliquam vitae nibh mi. Etiam tempor molestie bibendum.\
Suspendisse volutpat lorem eget ex sollicitudin, quis suscipit metus ultricies.\
Nam varius sem dapibus mi lobortis eleifend.\
Nulla pellentesque eros vel semper fringilla.\
Quisque facilisis tortor eu diam pharetra consectetur.\
Interdum et malesuada fames ac ante ipsum primis in faucibus.\
Donec imperdiet, quam at ornare sollicitudin, justo augue tincidunt purus,\
quis ultrices sapien nibh ac massa.

Sed egestas, risus sed sagittis ullamcorper, nisi eros aliquam elit,\
id posuere orci nulla sit amet nisi. Donec leo magna, lobortis at imperdiet vel,\
finibus quis massa. Cras a arcu neque. Pellentesque aliquet vehicula convallis.\
Aliquam erat volutpat. Nulla luctus justo tellus, sed mollis elit rhoncus ut.\
Aliquam sodales dui arcu, sed egestas ex eleifend eu. Donec eu tellus erat.\
Proin tincidunt felis metus, sit amet tempus eros semper at.\
Aenean in turpis tortor. Integer ut nibh a massa maximus bibendum.\
Praesent sodales eu felis sed vehicula. Donec condimentum efficitur sodales.
"
            );
        });
    });

    state!(_note_open = false, move |ui, open: &mut bool| {
        ui.with_padding(padding, |ui| {
            ui.horizontal(|ui| {
                ui.label("Pinned note:");
                let anchor = ui.button("Toggle note");
                if anchor.clicked() {
                    *open = !*open;
                }
                let _ = widgets::marginalia::pinned_note(
                    ui,
                    &anchor,
                    open,
                    egui::RectAlign::RIGHT_START,
                    280.0,
                    |ui| {
                        md!(
                            ui,
                            "**Pinned note**\n\nAnchored to the button response.\n\n- Pass a `RectAlign` to position the popup.\n- Control visibility with a mutable `bool`."
                        );
                    },
                );
            });
        });
    });

    state!(slider = 0.5, move |ui, value: &mut f32| {
        ui.with_padding(padding, |ui| {
            ui.add(widgets::Slider::new(value, 0.0..=1.0).text("input"));
        });
    });

    view!(move |ui| {
        ui.with_padding(0, |ui| {
            let progress = slider
                .read(ui)
                .map(|value| *value)
                .unwrap_or_default()
                * 0.5;
            md!(ui, "Progress: {:.2}%", progress * 100.0);
            ui.add(
                widgets::ProgressBar::new(progress)
                    .text("output")
                    .scale_percent(),
            );
        });
    });
}
