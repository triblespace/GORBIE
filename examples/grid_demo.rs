#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.33"
//! ```

use GORBIE::prelude::*;

#[notebook]
fn main(nb: &mut NotebookCtx) {
    nb.view(move |ctx| {
        md!(
            ctx,
            "# Grid Layout Demo
Flat 12-column grid with coordination-free layout.
Same spans → same pixel widths, every time, in every card."
        );
    });

    // Demonstrate the flat grid with state access at depth.
    let counter = nb.state("counter", 0u32, move |ctx, count| {
        ctx.grid(|g| {
            // Full-width heading
            g.place(12, |ctx| {
                ctx.label(egui::RichText::new("FLAT GRID").monospace().strong());
            });
            // 8 + 4 split
            g.place(8, |ctx| {
                ctx.label("Main column (8 of 12).");
                ctx.label(format!("Counter: {count}"));
                if ctx.button("INCREMENT").clicked() {
                    *count += 1;
                }
            });
            g.place(4, |ctx| {
                ctx.label(egui::RichText::new("SIDEBAR").monospace().strong());
                ctx.label("4 of 12 columns.");
            });
        });
    });

    // State reads at depth — works inside grid, horizontal, collapsing.
    nb.view(move |ctx| {
        ctx.with_padding(egui::Margin::same(GRID_EDGE_PAD as i8), |ctx| {
            ctx.label(egui::RichText::new("STATE ACCESS AT DEPTH").monospace().strong());
            ctx.horizontal(|ctx| {
                let count = *counter.read(ctx);
                ctx.label(format!("Reading counter from inside horizontal: {count}"));
            });
            ctx.collapsing("And inside collapsing too", |ctx| {
                let count = *counter.read(ctx);
                ctx.label(format!("Counter is: {count}"));
            });
        });
    });

    // Different grid layouts, all on the same 12-col grid.
    nb.view(move |ctx| {
        ctx.grid(|g| {
            // Full width label
            g.place(12, |ctx| {
                ctx.label(egui::RichText::new("GRID RATIOS").monospace().strong());
                ctx.add_space(8.0);
            });

            // Equal thirds (4 + 4 + 4)
            for label in ["LEFT", "CENTER", "RIGHT"] {
                g.place(4, |ctx| {
                    ctx.label(egui::RichText::new(label).monospace());
                    ctx.add(widgets::ProgressBar::new(0.5).text(label).scale_percent());
                });
            }

            // 3 + 6 + 3 — wide center
            g.place(3, |ctx| { ctx.label("Narrow (3)"); });
            g.place(6, |ctx| { ctx.label("Wide center (6)"); });
            g.place(3, |ctx| { ctx.label("Narrow (3)"); });

            // 9 + 3 — content + aside
            g.place(9, |ctx| { ctx.label("Main content area (9)"); });
            g.place(3, |ctx| { ctx.label("Aside (3)"); });

            // Furniture demo: 4 cols, skip 4, 4 cols
            g.place(4, |ctx| { ctx.label("Left (4)"); });
            g.skip(4);
            g.place(4, |ctx| { ctx.label("Right (4)"); });
        });
    });
}
