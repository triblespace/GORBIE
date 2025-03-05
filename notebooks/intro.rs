#!/usr/bin/env watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! gorbie = { path = ".." }
//! egui = "0.31"
//! ```

use gorbie::{notebook, Notebook, md, code, reactive};

fn intro(nb: &mut Notebook) {
    nb.cell(md("
# GORBIE!
This is **GORBIE!**, a _minimalist_ notebook environment for **Rust**!

Part of the [trible.space](https://trible.space) project.

![an image of 'GORBIE!' the cute alien blob and mascot of this project](./assets/gorbie.png)

# Intro

```
// This is the main function.
fn main() {
    // Statements here are executed when the compiled binary is called.

    // Print text to the console.
    println!(\"Hello World!\");
}
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
"));

    nb.cell(code!(|ui| {    
        ui.ctx().clone().style_ui(ui, egui::Theme::Light);
    }));

    nb.cell(reactive!(|ui, prev| {
        let mut value = prev.unwrap_or(0.5);
        let result = ui.add(egui::Slider::new(&mut value, 0.0..=1.0).text("input"));
        
        ui.add(egui::ProgressBar::new(value).text("output"));
    
        value
    }));


    nb.cell(reactive!(|ui, prev| {
        let mut value = prev.unwrap_or(0.5);
        let result = ui.add(egui::Slider::new(&mut value, 0.0..=1.0).text("input"));
        
        ui.add(egui::ProgressBar::new(value).text("output"));
    
        value
    }));
}

notebook!(intro);