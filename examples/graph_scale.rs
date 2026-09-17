//! Time the CPU force layout at the sizes the three views actually reach.
//!
//! This exists because the decision it feeds is otherwise made by instinct:
//! whether a GPU is warranted for a given view, or whether reaching for one is
//! laziness dressed as ambition. Run it, read the numbers, then decide.
//!
//! Every figure it prints is **milliseconds per solver step**, single core,
//! release build, on the named machine — not per frame, and not per token of
//! anything. A step is one force evaluation plus one integration plus the two
//! momentum corrections. A view that wants 4 ms of solver per frame gets
//! `4 / <figure>` steps of settling per frame at that size.
//!
//! ```text
//! cargo run --release --example graph_scale
//! ```

use GORBIE::graph::{ForceLayout, LayoutParams};

fn ring_with_chords(nodes: usize) -> Vec<(u32, u32)> {
    let mut edges = Vec::with_capacity(nodes * 2);
    for index in 0..nodes as u32 {
        edges.push((index, (index + 1) % nodes as u32));
        if index % 3 == 0 {
            edges.push((index, (index + 37) % nodes as u32));
        }
    }
    edges
}

fn time(nodes: usize, theta: f32, steps: usize) -> f64 {
    let edges = ring_with_chords(nodes);
    let params = LayoutParams {
        theta,
        ..LayoutParams::default()
    };
    let mut layout = ForceLayout::new(nodes, &edges, params);
    // Warm past the seed transient so the tree is the shape it will spend its
    // life being, rather than one ring's worth of degenerate cells.
    for _ in 0..20 {
        layout.step();
    }
    let start = std::time::Instant::now();
    for _ in 0..steps {
        layout.step();
    }
    start.elapsed().as_secs_f64() * 1000.0 / steps as f64
}

fn main() {
    println!("ms per solver step, single core, this machine");
    println!(
        "{:>8}  {:>10}  {:>12}  {:>10}",
        "nodes", "theta=0.7", "all-pairs", "speedup"
    );
    // The sizes are the real ones: a colony JP wants to reach, the wiki's live
    // graph, one collection's join lattice, and a whole derivation chain.
    for (nodes, steps, exact) in [
        (200usize, 200usize, true),
        (1_000, 100, true),
        (2_163, 50, true),
        (20_169, 10, false),
        (45_821, 5, false),
    ] {
        let approximate = time(nodes, 0.7, steps);
        if exact {
            let all_pairs = time(nodes, 0.0, steps.min(40));
            println!(
                "{nodes:>8}  {approximate:>10.2}  {all_pairs:>12.2}  {:>9.1}x",
                all_pairs / approximate
            );
        } else {
            println!("{nodes:>8}  {approximate:>10.2}  {:>12}  {:>10}", "-", "-");
        }
    }
}
