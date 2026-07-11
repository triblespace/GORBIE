use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::Arc;

use eframe::egui::{self, vec2, Color32, Galley, Margin, Response, Stroke, TextStyle, Ui};
use egui::text::{LayoutJob, TextFormat};

use crate::themes::colorhash;

/// Number of runs per layout chunk. Sealed chunks (all but the last)
/// never re-lay out, so appends only touch the newest chunk.
const CHUNK_RUNS: usize = 64;

/// Visual style for one tag in a [`StreamLane`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunStyle {
    /// Text color of runs carrying this tag.
    pub color: Color32,
    /// Optional background tint painted behind the run's glyphs.
    pub background: Option<Color32>,
}

impl RunStyle {
    /// A style with the given text color and no background tint.
    pub fn new(color: Color32) -> Self {
        Self {
            color,
            background: None,
        }
    }

    /// Add a background tint behind the run's glyphs.
    pub fn background(mut self, tint: Color32) -> Self {
        self.background = Some(tint);
        self
    }
}

struct Chunk<T> {
    runs: Vec<(T, String)>,
    /// Cached layout of this chunk at the current wrap width; `None`
    /// when the chunk changed or the cache was invalidated.
    galley: Option<Arc<Galley>>,
}

/// A scrolling stream of tagged text runs, e.g. an interleaved log of
/// several live sources.
///
/// Runs are `(tag, text)` pairs appended over time with [`push`]. A
/// palette maps each tag to a [`RunStyle`]; tags without an explicit
/// style get a stable default color hashed from the tag.
///
/// The default palette is **colorblind-safe** by construction: it draws
/// from [`colorhash::RAL_CVD_SAFE`], where every distinction is carried
/// by luminance plus the orange/blue axis — red-vs-green is never the
/// load-bearing difference. Custom palettes injected via [`style`]
/// should preserve this property when the color alone must identify
/// the tag.
///
/// Rendering stays cheap as the stream grows: runs are grouped into
/// fixed-size chunks, each chunk's [`LayoutJob`] result is cached
/// (sealed chunks never re-lay out), and only chunks intersecting the
/// visible scroll viewport are painted. The scroll view sticks to the
/// bottom while the user is at the bottom.
///
/// ```ignore
/// // In notebook state:
/// let mut lane = StreamLane::new();
/// lane.push("plan", "considering 3 candidate actions\n");
/// lane.push("act", "grasp(cup)\n");
/// // In the card closure:
/// lane.show(ui);
/// ```
///
/// Note: runs are concatenated verbatim — include `'\n'` in the text
/// where you want line breaks. When placing several lanes in the same
/// `Ui` scope, wrap each in `ui.push_id(..)` so their scroll states
/// don't collide.
///
/// [`push`]: Self::push
/// [`style`]: Self::style
pub struct StreamLane<T> {
    chunks: VecDeque<Chunk<T>>,
    styles: HashMap<T, RunStyle>,
    max_runs: usize,
    total_runs: usize,
    desired_height: f32,
    cached_width: f32,
    cached_dark: bool,
}

impl<T> Default for StreamLane<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> StreamLane<T> {
    /// An empty lane with the default palette, a 10 000-run retention
    /// cap, and a 240px viewport.
    pub fn new() -> Self {
        Self {
            chunks: VecDeque::new(),
            styles: HashMap::new(),
            max_runs: 10_000,
            total_runs: 0,
            desired_height: 240.0,
            cached_width: 0.0,
            cached_dark: false,
        }
    }

    /// Cap the number of retained runs; the oldest runs are dropped
    /// (in whole chunks of {`CHUNK_RUNS`}) once the cap is exceeded.
    /// Default is 10 000. Use `usize::MAX` for unbounded retention.
    pub fn max_runs(mut self, max_runs: usize) -> Self {
        self.max_runs = max_runs.max(1);
        self
    }

    /// Set the height of the scrolling viewport in pixels. Default is 240.
    pub fn desired_height(mut self, height: f32) -> Self {
        self.desired_height = height.max(24.0);
        self
    }

    /// Number of runs currently retained.
    pub fn len(&self) -> usize {
        self.total_runs
    }

    /// True when no runs are retained.
    pub fn is_empty(&self) -> bool {
        self.total_runs == 0
    }

    /// Drop all runs (styles are kept).
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.total_runs = 0;
    }
}

impl<T: Hash + Eq> StreamLane<T> {
    /// Assign an explicit style to a tag, overriding the hashed
    /// default. Prefer colors whose distinctions survive red-green
    /// color vision deficiency (luminance steps, orange vs blue —
    /// see [`colorhash::RAL_CVD_SAFE`]).
    pub fn style(&mut self, tag: T, style: RunStyle) {
        self.styles.insert(tag, style);
        for chunk in &mut self.chunks {
            chunk.galley = None;
        }
    }

    /// Append a run to the end of the stream.
    pub fn push(&mut self, tag: T, text: impl Into<String>) {
        let needs_chunk = self
            .chunks
            .back()
            .is_none_or(|chunk| chunk.runs.len() >= CHUNK_RUNS);
        if needs_chunk {
            self.chunks.push_back(Chunk {
                runs: Vec::new(),
                galley: None,
            });
        }
        let chunk = self.chunks.back_mut().expect("chunk pushed above");
        chunk.runs.push((tag, text.into()));
        chunk.galley = None;
        self.total_runs += 1;

        while self.total_runs > self.max_runs && self.chunks.len() > 1 {
            if let Some(front) = self.chunks.pop_front() {
                self.total_runs -= front.runs.len();
            }
        }
    }

    /// Render the lane into `ui` as a stick-to-bottom scroll view.
    pub fn show(&mut self, ui: &mut Ui) -> Response {
        let font_id = TextStyle::Monospace.resolve(ui.style());
        let dark = ui.visuals().dark_mode;
        let outline = ui.visuals().widgets.noninteractive.bg_stroke.color;

        let frame = egui::Frame::new()
            .stroke(Stroke::new(1.0, outline))
            .inner_margin(Margin::same(4))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("gorbie_stream_lane")
                    .max_height(self.desired_height)
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let wrap_width = ui.available_width().max(24.0);
                        if (wrap_width - self.cached_width).abs() > 0.5
                            || dark != self.cached_dark
                        {
                            self.cached_width = wrap_width;
                            self.cached_dark = dark;
                            for chunk in &mut self.chunks {
                                chunk.galley = None;
                            }
                        }

                        ui.spacing_mut().item_spacing.y = 0.0;
                        let Self { chunks, styles, .. } = self;
                        for chunk in chunks {
                            let galley = chunk
                                .galley
                                .get_or_insert_with(|| {
                                    let mut job = LayoutJob::default();
                                    job.wrap.max_width = wrap_width;
                                    for (tag, text) in &chunk.runs {
                                        let style = resolve_style(styles, tag);
                                        job.append(
                                            text,
                                            0.0,
                                            TextFormat {
                                                font_id: font_id.clone(),
                                                color: style.color,
                                                background: style
                                                    .background
                                                    .unwrap_or(Color32::TRANSPARENT),
                                                ..Default::default()
                                            },
                                        );
                                    }
                                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                                })
                                .clone();

                            let (rect, _) = ui.allocate_exact_size(
                                vec2(wrap_width, galley.size().y),
                                egui::Sense::hover(),
                            );
                            if ui.is_rect_visible(rect) {
                                // The per-section colors live in the job;
                                // the fallback color is never used.
                                ui.painter().galley(rect.min, galley, Color32::PLACEHOLDER);
                            }
                        }
                    });
            });

        frame.response
    }
}

/// Explicit style if assigned, otherwise a stable colorblind-safe
/// color hashed from the tag.
fn resolve_style<T: Hash + Eq>(styles: &HashMap<T, RunStyle>, tag: &T) -> RunStyle {
    if let Some(style) = styles.get(tag) {
        return *style;
    }
    let mut hasher = colorhash::Fnv1a64::new();
    tag.hash(&mut hasher);
    RunStyle::new(colorhash::ral_cvd_safe_from_hash(hasher.finish()))
}
