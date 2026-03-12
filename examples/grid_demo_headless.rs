#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.33"
//! ```

use GORBIE::prelude::*;

fn main() -> eframe::Result {
    NotebookConfig::new("Grid Demo")
        .with_headless_capture("/tmp/gorbie_grid_demo2")
        .run(move |nb| {
            nb.view(move |ctx| {
                md!(ctx, "# Grid Layout Demo");
            });

            nb.view(move |ctx| {
                ctx.grid(|g| {
                    g.place(12, |ctx| {
                        ctx.label(egui::RichText::new("FULL WIDTH").monospace().strong());
                    });
                    g.place(8, |ctx| {
                        ctx.label("Main column (8 of 12).");
                        ctx.label("Counter: 42");
                        ctx.button("INCREMENT");
                    });
                    g.place(4, |ctx| {
                        ctx.label(egui::RichText::new("SIDEBAR").monospace().strong());
                        ctx.label("4 of 12 columns.");
                    });
                });
            });

            nb.view(move |ctx| {
                ctx.with_padding(egui::Margin::same(GRID_EDGE_PAD as i8), |ctx| {
                    ctx.label(egui::RichText::new("STATE ACCESS AT DEPTH").monospace().strong());
                    ctx.label("Reading counter from inside horizontal: 42");
                });
            });

            nb.view(move |ctx| {
                ctx.grid(|g| {
                    for label in ["LEFT", "CENTER", "RIGHT"] {
                        g.place(4, |ctx| {
                            ctx.label(egui::RichText::new(label).monospace());
                            ctx.add(
                                widgets::ProgressBar::new(0.5)
                                    .text(label)
                                    .scale_percent(),
                            );
                        });
                    }
                    g.place(9, |ctx| { ctx.label("Main (9)"); });
                    g.place(3, |ctx| { ctx.label("Aside (3)"); });

                    g.place(4, |ctx| { ctx.label("Left (4)"); });
                    g.skip(4);
                    g.place(4, |ctx| { ctx.label("Right (4)"); });
                });
            });

            nb.settled();
        })
}
