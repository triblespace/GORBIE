#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = "..", features = ["plots", "triblespace"] }
//! egui = "0.34"
//! egui_plot = "0.35"
//! triblespace = { path = "../../triblespace-rs" }
//! ed25519-dalek = "2"
//! ```
//!
//! Live-dashboard widget tour: `StreamLane`, `BudgetGauge`,
//! `MetricStrip`, `EventFeed`, and the `PileTail` data helper, all fed
//! with synthetic data. A background thread appends commits to a
//! temporary pile every 500 ms; the `PileTail` card tails it live.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use ed25519_dalek::SigningKey;
use triblespace::core::id::rngid;
use triblespace::core::repo::pile::Pile;
use triblespace::core::repo::Repository;
use triblespace::core::trible::TribleSet;
use triblespace::macros::{entity, find, pattern};

use GORBIE::notebook;
use GORBIE::widgets;
use GORBIE::widgets::triblespace::PileTail;
use GORBIE::widgets::{BudgetGauge, EventFeed, MetricStrip, StreamLane};
use GORBIE::NotebookCtx;

mod dashboard {
    use triblespace::macros::attributes;
    attributes! {
        // Minted with `trible genid`.
        "8B2031A4D900BBD14DF360C479ACE1E5" as pub tick_label:
            triblespace::core::inline::encodings::shortstring::ShortString;
    }
}

/// Temp pile fed by a background writer thread; created once.
fn demo_pile() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = std::env::temp_dir().join(format!(
            "gorbie_dashboard_demo_{pid}.pile",
            pid = std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        std::fs::File::create(&path).expect("create demo pile");
        spawn_writer(path.clone());
        path
    })
}

/// Appends one small commit to the `dashboard` branch every 500 ms.
fn spawn_writer(path: PathBuf) {
    std::thread::spawn(move || {
        let mut pile = Pile::open(&path).expect("open demo pile");
        pile.refresh().expect("refresh demo pile");
        // Deterministic demo key — this pile is throwaway synthetic data.
        let mut repo = Repository::new(pile, SigningKey::from_bytes(&[7u8; 32]), TribleSet::new())
            .expect("create demo repository");
        let branch_id = *repo
            .create_branch("dashboard", None)
            .expect("create demo branch");

        for tick in 0u64.. {
            let mut ws = repo.pull(branch_id).expect("pull demo branch");
            let mut facts = TribleSet::new();
            let e = rngid();
            facts += entity! { &e @ dashboard::tick_label: format!("tick {tick}").as_str() };
            ws.commit(facts, "demo tick");
            repo.push(&mut ws).expect("push demo tick");
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

const LANE_TAGS: [&str; 4] = ["sense", "plan", "act", "note"];

#[derive(Default)]
struct LaneState {
    lane: StreamLane<&'static str>,
    emitted: usize,
}

#[derive(Default)]
struct FeedState {
    feed: EventFeed,
    emitted: usize,
}

struct StripState {
    strip: MetricStrip,
}

struct TailState {
    tail: Option<Result<PileTail, String>>,
    lane: StreamLane<&'static str>,
    total_facts: usize,
}

/// Synthetic "frame time" in ms, oscillating across the 80 ms budget.
fn synthetic_ms(t: f64) -> f64 {
    60.0 + 25.0 * (t * 0.6).sin() + 8.0 * (t * 2.9).sin()
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;

    nb.view(move |ctx| {
        widgets::markdown(
            ctx,
            "# Dashboard widgets\n\nA tour of the live-dashboard widgets with synthetic data: \
             `StreamLane` (tagged text runs), `BudgetGauge` (scalar vs budget), `MetricStrip` \
             (rolling time-series), `EventFeed` (newest-first rows), and `PileTail` (live \
             branch tailing — a background thread commits to a temporary pile every 500 ms).",
        );
    });

    // ── StreamLane ───────────────────────────────────────────────────
    nb.state("lane", LaneState::default(), move |ctx, state| {
        ctx.with_padding(padding, |ctx| {
            ctx.heading("StreamLane");
            let t = ctx.input(|i| i.time);
            let due = (t * 4.0) as usize;
            while state.emitted < due {
                let n = state.emitted;
                let tag = LANE_TAGS[n % LANE_TAGS.len()];
                state.lane.push(tag, format!("[{tag}] event {n}: value={:.2}\n", synthetic_ms(n as f64 * 0.25)));
                state.emitted += 1;
            }
            state.lane.show(ctx);
            ctx.ctx().request_repaint_after(Duration::from_millis(100));
        });
    });

    // ── BudgetGauge ──────────────────────────────────────────────────
    nb.view(move |ctx| {
        ctx.with_padding(padding, |ctx| {
            ctx.heading("BudgetGauge");
            let t = ctx.input(|i| i.time);
            ctx.add(
                BudgetGauge::new(synthetic_ms(t), 80.0)
                    .label("frame")
                    .suffix(" ms"),
            );
            ctx.ctx().request_repaint_after(Duration::from_millis(50));
        });
    });

    // ── MetricStrip ──────────────────────────────────────────────────
    nb.state(
        "strip",
        StripState {
            strip: MetricStrip::new("frame time", 600)
                .suffix(" ms")
                .percentile_band(true),
        },
        move |ctx, state| {
            ctx.with_padding(padding, |ctx| {
                ctx.heading("MetricStrip");
                let t = ctx.input(|i| i.time);
                state.strip.push(t, synthetic_ms(t));
                state.strip.show(ctx);
                ctx.ctx().request_repaint_after(Duration::from_millis(50));
            });
        },
    );

    // ── EventFeed ────────────────────────────────────────────────────
    nb.state("feed", FeedState::default(), move |ctx, state| {
        ctx.with_padding(padding, |ctx| {
            ctx.heading("EventFeed");
            let t = ctx.input(|i| i.time);
            let due = (t / 1.2) as usize;
            while state.emitted < due {
                let n = state.emitted;
                let category = LANE_TAGS[n % LANE_TAGS.len()];
                if n % 3 == 0 {
                    state.feed.push_with_detail(
                        category,
                        format!("event {n} finished"),
                        format!(
                            "synthetic detail for event {n}:\nvalue = {:.2} ms\nphase = {}",
                            synthetic_ms(n as f64),
                            n % 7
                        ),
                    );
                } else {
                    state.feed.push(category, format!("event {n} finished"));
                }
                state.emitted += 1;
            }
            state.feed.show(ctx);
            ctx.ctx().request_repaint_after(Duration::from_millis(200));
        });
    });

    // ── PileTail ─────────────────────────────────────────────────────
    nb.state(
        "tail",
        TailState {
            tail: None,
            lane: StreamLane::new(),
            total_facts: 0,
        },
        move |ctx, state| {
            ctx.with_padding(padding, |ctx| {
                ctx.heading("PileTail");
                let path = demo_pile();

                let tail = state.tail.get_or_insert_with(|| {
                    PileTail::open_by_name(path, "dashboard")
                        .map(|tail| tail.min_interval(Duration::from_millis(250)))
                });

                match tail {
                    Err(err) => {
                        let color = ctx.visuals().error_fg_color;
                        ctx.label(egui::RichText::new(err.as_str()).color(color).monospace());
                    }
                    Ok(tail) => match tail.poll() {
                        Err(err) => {
                            let color = ctx.visuals().error_fg_color;
                            ctx.label(egui::RichText::new(err).color(color).monospace());
                        }
                        Ok(delta) => {
                            if !delta.is_empty() {
                                state.total_facts += delta.len();
                                for (label,) in find!(
                                    (label: String),
                                    pattern!(&delta, [{ dashboard::tick_label: ?label }])
                                ) {
                                    state
                                        .lane
                                        .push("delta", format!("{label} ({} tribles)\n", delta.len()));
                                }
                            }
                        }
                    },
                }

                ctx.horizontal(|ctx| {
                    widgets::row_label(ctx, "Facts tailed:");
                    widgets::lcd_readout(ctx, &format!("{}", state.total_facts), None);
                });
                ctx.add_space(4.0);
                state.lane.show(ctx);
                ctx.ctx().request_repaint_after(Duration::from_millis(250));
            });
        },
    );
}
