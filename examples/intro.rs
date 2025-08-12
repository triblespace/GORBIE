#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! ```

use std::ops::DerefMut;

use GORBIE::{derive, md, notebook, state, view, Notebook, NotifiedState};

fn intro(nb: &mut Notebook) {
    view!(nb, (), move |ui| {
        md!(ui,
"# GORBIE!
This is **GORBIE!**, a _minimalist_ notebook environment for **Rust**!

It's much closer to a library and a shell script than the heavy environemnts
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
");

        ui.ctx().clone().style_ui(ui, egui::Theme::Light);
    });

    let slider = state!(nb, (), (0.5).into(), |ui, value: &mut NotifiedState<_>| {
        if ui
            .add(egui::Slider::new(value.deref_mut(), 0.0..=1.0).text("input"))
            .changed()
        {
            value.notify();
        }
    });

    let progress = derive!(nb, (slider), move |(slider,)| {
        //Derives are executed on a new thread, so we can sleep or perform heavy computations here.
        //Uncomment the line below to see waiting in action.
        //std::thread::sleep(std::time::Duration::from_secs(2));
        slider * 0.5
    });

    view!(nb, (progress), move |ui| {
        let Some(progress) = progress.try_read() else {
            return;
        };
        let Some(progress) = progress.ready() else {
            return;
        };
        md!(ui, "Progress: {:.2}%", *progress * 100.0);
        ui.add(egui::ProgressBar::new(*progress).text("output"));
    });
}

fn main() {
    notebook!(intro);
}
