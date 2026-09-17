use eframe::egui::{pos2, vec2, Align2, Pos2, Response, Sense, TextStyle, Ui, Widget};

use crate::graph::{self, Anchors, Glyph, GraphViewport, LayoutParams, Stroke2};
use crate::themes;

/// What this observation knows about one node's own reporting.
///
/// The last two used to be one value, and the conflation was visible from the
/// outside: asked whether the slashed daemons were down or merely quiet, the
/// picture could not say. They are different facts and only one of them is an
/// absence. A node nobody here has heard from is a **hole** — it exists in this
/// picture because a peer named it, and its own view of itself is missing. A
/// node that reported with a timestamp we cannot place is not missing at all;
/// it is present and its clock disagrees, which is a fault to chase rather than
/// a gap to fill.
///
/// `Freshness` is deliberately not where this split belongs: freshness
/// describes a *report*, so there is no freshness value for "no report exists".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeshNodeState {
    /// Reported within the freshness window.
    Fresh,
    /// Reported, but older than the window. Only a node that sent a report can
    /// be stale.
    Stale,
    /// Reported, and the report cannot be placed in time — a future timestamp,
    /// or a clock this observation cannot reconcile.
    Unknown,
    /// Named by a peer and never heard from here.
    ///
    /// This is the mesh's absence, and it is drawn the way every other absence
    /// in this vocabulary is drawn: dashed. A dashed mark means the same thing
    /// here as it does in the collection lattice — named, not resident — which
    /// is the point of there being one drawing kit rather than three.
    Unreported,
}

/// One node in the observed mesh.
#[derive(Clone, Debug)]
pub struct MeshNode {
    /// Short handle, drawn outside the mark.
    pub label: String,
    /// Freshness of this node's own report.
    pub state: MeshNodeState,
    /// Records here over records this node reports, in `0.0..=1.0`.
    ///
    /// `None` means the node reported nothing comparable. That is *not*
    /// convergence: it is absence of evidence, and it draws as an empty
    /// track rather than a full ring.
    pub convergence: Option<f32>,
}

/// A directed observation: `from` observed `to`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeshLink {
    /// Index into the node slice.
    pub from: usize,
    /// Index into the node slice.
    pub to: usize,
}

/// The observed peer mesh, drawn as a force-directed graph of observations.
///
/// # Why a simulation, and why the ring survives inside it
///
/// This widget used to argue against a force-directed layout, and the argument
/// was good for the size it was written for: "a force-directed layout earns its
/// cost on hundreds of nodes with latent structure. A peer mesh is a handful of
/// nodes whose only structure is who observed whom." A colony of a thousand
/// peers is exactly the case that paragraph named as the one where the cost is
/// earned, so the same reasoning now points the other way.
///
/// The old objection that still bites is the other one: "the same report
/// renders differently twice, which is exactly what an instrument must not do."
/// Two answers. First, the radial placement is **kept as an anchor** rather
/// than thrown away — up to [`RING_FULL`] peers the ring spring holds every
/// node on the circle, so a three-peer colony draws the picture it always drew,
/// and the ring's hold fades out between there and [`RING_NONE`] where there is
/// real structure to find instead. Second, an instrument *may* show motion that
/// means something: energy here is granted only where something changed, and a
/// quiet field with one moving neighbourhood is telling you where to look in a
/// channel that costs no ink and no colour.
///
/// Node order is still the caller's, and the ring is still seeded from it.
///
/// # Encoding
///
/// State is carried by **shape and stroke**, never by colour alone, so the
/// widget survives colourblind readers, greyscale print and forced-contrast
/// modes: a filled square reported freshly, an open square reported stale, a
/// dashed square never reported here at all, and a slashed square reported
/// something this observation cannot place in time. The accent is spent only on
/// the direction barbs, which is the one thing a still image cannot otherwise
/// disambiguate.
///
/// ```ignore
/// ui.add(MeshGraph::new(&nodes, &links));
/// ```
#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct MeshGraph<'a> {
    nodes: &'a [MeshNode],
    links: &'a [MeshLink],
    height: f32,
    salt: &'static str,
    highlight: Option<usize>,
}

impl<'a> MeshGraph<'a> {
    /// A mesh of `nodes` connected by directed `links`.
    ///
    /// Links whose endpoints fall outside `nodes` are skipped rather than
    /// panicking: the report is an open-world observation and may name a peer
    /// that no node reported for.
    pub fn new(nodes: &'a [MeshNode], links: &'a [MeshLink]) -> Self {
        Self {
            nodes,
            links,
            height: 320.0,
            salt: "gorbie_mesh",
            highlight: None,
        }
    }

    /// Override the drawing height. Width follows the available space.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Spend the accent on this node's links.
    ///
    /// Direction is worth an accent mark only where somebody is asking about
    /// it; at rest nobody is, and a barb on every link reads as measles rather
    /// than as scarce saturation. Pass the hovered or selected node.
    pub fn highlight(mut self, node: Option<usize>) -> Self {
        self.highlight = node;
        self
    }

    /// Distinguish two meshes drawn from the same `Ui`.
    ///
    /// The layout persists between frames under an id derived from the calling
    /// `Ui`, so two meshes in one row would otherwise share one simulation and
    /// retarget each other every frame.
    pub fn id_salt(mut self, salt: &'static str) -> Self {
        self.salt = salt;
        self
    }
}

/// At or below this many peers the ring anchor holds every node on the circle,
/// so a small colony keeps the deterministic picture it has always had.
pub const RING_FULL: usize = 12;
/// At or above this many the ring is gone and the layout is free to find
/// whatever structure the observations actually carry.
pub const RING_NONE: usize = 32;
/// World units between two neighbouring peers on the ring.
///
/// World scale is NOT free, which cost a render to learn. The viewport frames
/// whatever the layout occupies, so a ring laid out in arbitrarily large world
/// units fits by zooming out — and a three-peer mesh came back as three specks
/// on a card with all the room in the world. Spacing the ring by a pitch close
/// to the points it will be drawn at makes the fit land near unity, so a small
/// colony draws at the size it always drew at.
const NODE_PITCH: f32 = 116.0;
/// Smallest ring, so two peers are not drawn on top of each other.
const MIN_RING: f32 = 92.0;
/// Ring spring stiffness at or below [`RING_FULL`].
const RING_K: f32 = 1.0;
/// Screen points kept clear around the marks for the labels outside them.
const LABEL_MARGIN: f32 = 46.0;
/// Solver time per frame. The rest of the frame belongs to everything else.
const STEP_BUDGET_MS: f32 = 4.0;

/// The mark for one node state.
///
/// Four states, two channels, no colour. Absence is dashed, the way absence is
/// dashed everywhere else in this vocabulary; a clock fault is a solid outline
/// with a slash through it, because the node is present and its report is the
/// thing that cannot be read.
fn mark_for(state: MeshNodeState) -> (Glyph, Stroke2) {
    match state {
        MeshNodeState::Fresh => (Glyph::Square, Stroke2::Filled),
        MeshNodeState::Stale => (Glyph::Square, Stroke2::Open),
        // Both of these are holes in the data and both are dashed. The slash
        // is what separates them: it says an attempt was made and failed,
        // where a bare dashed square says no attempt reached us at all.
        MeshNodeState::Unknown => (Glyph::SquareSlashed, Stroke2::Dashed),
        MeshNodeState::Unreported => (Glyph::Square, Stroke2::Dashed),
    }
}

/// Is this node one of the ones somebody opened the view for?
///
/// The level of detail decimates the **norm**, never the exception. A level of
/// detail that folds everything equally is a blur, and a blur of a thousand
/// healthy peers hides the eleven that are not — which are the only reason
/// anyone looked. So the cost of drawing is bounded by how bad things are
/// rather than by how big the colony is, and exceptions are rare by definition.
fn exceptional(state: MeshNodeState) -> bool {
    !matches!(state, MeshNodeState::Fresh)
}

/// Where node `index` of `count` sits on the seed ring. First node at the top,
/// the rest clockwise, exactly as the radial placement did.
fn ring_point(index: usize, count: usize) -> [f32; 2] {
    if count <= 1 {
        return [0.0, 0.0];
    }
    // Circumference is the pitch times the peers, so neighbours sit a fixed
    // world distance apart however many there are.
    let radius = (NODE_PITCH * count as f32 / std::f32::consts::TAU).max(MIN_RING);
    let turn = index as f32 / count as f32;
    let angle = turn * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    [radius * angle.cos(), radius * angle.sin()]
}

/// How far a colony of `count` peers has travelled from ring to free layout.
///
/// `0.0` at or below [`RING_FULL`], `1.0` at or above [`RING_NONE`], linear
/// between. One number drives the whole handover, so the ring can never be
/// half-on in one channel and half-off in another.
fn ring_fade(count: usize) -> f32 {
    let span = (RING_NONE - RING_FULL) as f32;
    (count.saturating_sub(RING_FULL) as f32 / span).clamp(0.0, 1.0)
}

/// Ring stiffness: full hold up to [`RING_FULL`], fading to the ordinary
/// gravity well at [`RING_NONE`] and beyond.
fn ring_stiffness(count: usize, gravity: [f32; 2]) -> f32 {
    let fade = ring_fade(count);
    RING_K * (1.0 - fade) + gravity[0] * fade
}

/// The law this view runs, which is the shared one with the observation spring
/// faded in as the ring fades out.
///
/// A stiffer anchor cannot hold a ring against springs, and it took a wrong
/// turn to see why: a spring's force grows with the distance it spans, so an
/// anchored node settles where `anchor_k * displacement` meets
/// `attraction * span`, and the ratio is a property of the two coefficients
/// and not of the ring's size. Making the ring bigger scaled the error with
/// it — a six-peer mesh with one hub sat three percent out of round at every
/// radius tried. The honest fix is not a stronger anchor. It is that **at a
/// size where the ring is the right picture, a link is something to draw and
/// not something to lay out by** — which is exactly what the radial placement
/// this widget used to do meant, and it is preserved rather than approximated.
fn mesh_params(count: usize) -> LayoutParams {
    let base = LayoutParams::default();
    LayoutParams {
        attraction: base.attraction * ring_fade(count),
        ..base
    }
}

/// What the mesh drew, and what it folded away.
///
/// A view that folds and does not disclose it is a view you cannot trust the
/// next time it looks sparse, so the counts come back rather than staying
/// inside the paint loop.
pub struct MeshResponse {
    /// The allocated region's own response.
    pub response: Response,
    /// Node under the pointer, if the pointer is near one.
    pub hovered: Option<usize>,
    /// Marks drawn in full this frame.
    pub drawn: usize,
    /// Nodes collapsed to a dot because the zoom made their mark unreadable.
    pub folded: usize,
}

impl MeshGraph<'_> {
    /// Draw the mesh and report what the pointer did and what was folded.
    pub fn show(self, ui: &mut Ui) -> MeshResponse {
        let width = ui.available_width();
        let (rect, response) =
            ui.allocate_exact_size(vec2(width, self.height), Sense::click_and_drag());
        if !ui.is_rect_visible(rect) || self.nodes.is_empty() {
            return MeshResponse {
                response,
                hovered: None,
                drawn: 0,
                folded: 0,
            };
        }
        let count = self.nodes.len();

        let keys: Vec<u64> = self
            .nodes
            .iter()
            .map(|node| graph::key_of(node.label.as_bytes()))
            .collect();
        let edges: Vec<(u32, u32)> = self
            .links
            .iter()
            .filter(|link| link.from != link.to)
            .map(|link| (link.from as u32, link.to as u32))
            .collect();

        let params = mesh_params(count);
        let layout_id = ui.id().with(self.salt);
        let (shared, changed) = graph::shared(ui, layout_id, &keys, &edges, params);
        let mut shared = shared.lock();
        let layout = shared.layout_mut();

        let view_id = layout_id.with("view");
        let mut view = GraphViewport::load(ui, view_id);
        if changed {
            // The roster decides how much ring is left, so the law is re-read
            // here rather than only at creation.
            layout.set_params(params);
            let target: Vec<[f32; 2]> = (0..count).map(|index| ring_point(index, count)).collect();
            let stiffness = ring_stiffness(count, params.anchor_k);
            for (index, point) in target.iter().enumerate() {
                layout.seed(index, *point);
            }
            layout.anchors(Some(&Anchors::uniform(target, [stiffness, stiffness])));
            // A changed roster is a changed picture, so the framing goes back
            // to the widget. A reader who has panned keeps their view; see
            // `GraphViewport::touched`.
            view.release();
        }
        let stats = layout.step_budget(STEP_BUDGET_MS);
        view.interact(ui, &response, rect);
        view.fit_unless_touched(rect, stats.bounds, LABEL_MARGIN);
        view.store(ui, view_id);
        if !stats.quiet {
            ui.ctx().request_repaint();
        }

        let positions = layout.positions();
        let screen: Vec<Pos2> = (0..count)
            .map(|index| view.to_screen(rect, positions[index]))
            .collect();

        let visuals = ui.visuals();
        let ink = visuals.text_color();
        let hairline = visuals.widgets.noninteractive.bg_stroke.color;
        let track = themes::blend(visuals.window_fill, hairline, 0.55);
        let accent = themes::button_light_on();
        let painter = ui.painter().with_clip_rect(rect);
        let font = TextStyle::Small.resolve(ui.style());
        let scale = view.mark_scale();
        let lod = view.lod(NODE_PITCH);
        let visible = rect.expand(graph::HIT * scale + 4.0);

        // Links first, so node marks always sit on top of them.
        for link in self.links {
            let (Some(from), Some(to)) = (screen.get(link.from), screen.get(link.to)) else {
                continue;
            };
            if link.from == link.to || !(visible.contains(*from) || visible.contains(*to)) {
                continue;
            }
            // Scarcity is a function of how many there are. On a ring-sized
            // colony a barb on every link is a handful of accent marks and the
            // one thing a still image cannot otherwise say, so it stays — this
            // is the picture the dashboard draws today. Past the ring it would
            // be twenty accent marks in one field, which reads as measles
            // rather than as scarce saturation, so above that size the accent
            // goes only to the links of whatever the reader asked about.
            let asked = count <= RING_FULL
                || self.highlight == Some(link.from)
                || self.highlight == Some(link.to);
            let barb = (asked && matches!(lod, graph::Lod::Full)).then_some(accent);
            // Links recede by thinning, never by lightening: see `draw_link`.
            let weight = match lod {
                graph::Lod::Full => 1.0,
                graph::Lod::Marks => 0.7,
                _ => 0.45,
            };
            graph::draw_link(&painter, *from, *to, scale, true, barb, hairline, weight);
        }

        let centre = centroid(&screen);
        let ground = visuals.panel_fill;
        let mut drawn = 0usize;
        let mut folded = 0usize;
        for (index, node) in self.nodes.iter().enumerate() {
            let at = screen[index];
            if !visible.contains(at) {
                continue;
            }
            // An exceptional node is drawn in full at every zoom, on a small
            // knockout of the page ground so it stays legible in the densest
            // part of the field.
            let detail = if exceptional(node.state) {
                graph::Lod::Full
            } else {
                lod
            };
            if matches!(detail, graph::Lod::Dots | graph::Lod::Aggregate) {
                painter.circle_filled(at, 1.5, ink);
                folded += 1;
                continue;
            }
            drawn += 1;
            if exceptional(node.state) && !matches!(lod, graph::Lod::Full) {
                painter.circle_filled(at, graph::MARK * scale + 2.0, ground);
            }
            if matches!(detail, graph::Lod::Full) {
                graph::draw_track(&painter, at, scale, node.convergence, track, ink);
            }
            let (glyph, outline) = mark_for(node.state);
            graph::draw_node(&painter, at, scale, glyph, outline, ink, 1);

            if !matches!(lod, graph::Lod::Full) {
                continue;
            }
            // Labels sit outside the mark, pushed away from the centre of the
            // drawn cloud so they never collide with the marks or the links.
            let outward = if count == 1 || (at - centre).length() < f32::EPSILON {
                vec2(0.0, 1.0)
            } else {
                (at - centre).normalized()
            };
            let anchor = at + outward * (graph::TRACK * scale + 10.0);
            let align = if outward.y.abs() > outward.x.abs() {
                if outward.y < 0.0 {
                    Align2::CENTER_BOTTOM
                } else {
                    Align2::CENTER_TOP
                }
            } else if outward.x < 0.0 {
                Align2::RIGHT_CENTER
            } else {
                Align2::LEFT_CENTER
            };
            painter.text(anchor, align, &node.label, font.clone(), ink);
        }

        let hovered = response
            .hover_pos()
            .and_then(|at| graph::pick(&screen, at, graph::HIT * scale.max(0.4)));
        MeshResponse {
            response,
            hovered,
            drawn,
            folded,
        }
    }
}

impl Widget for MeshGraph<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui).response
    }
}

fn centroid(points: &[Pos2]) -> Pos2 {
    if points.is_empty() {
        return pos2(0.0, 0.0);
    }
    let mut sum = vec2(0.0, 0.0);
    for point in points {
        sum += point.to_vec2();
    }
    (sum / points.len() as f32).to_pos2()
}

/// Settle a mesh the way the widget would, without a `Ui`.
///
/// The widget's own layout lives in egui's memory and needs a context; this is
/// the same construction, so the tests below exercise the placement rules
/// rather than a separate model of them.
#[cfg(test)]
fn settled(count: usize, links: &[MeshLink], steps: usize) -> Vec<[f32; 2]> {
    use crate::graph::ForceLayout;
    let params = mesh_params(count);
    let edges: Vec<(u32, u32)> = links
        .iter()
        .map(|link| (link.from as u32, link.to as u32))
        .collect();
    let mut layout = ForceLayout::new(count, &edges, params);
    let target: Vec<[f32; 2]> = (0..count).map(|index| ring_point(index, count)).collect();
    let stiffness = ring_stiffness(count, params.anchor_k);
    for (index, point) in target.iter().enumerate() {
        layout.seed(index, *point);
    }
    layout.anchors(Some(&Anchors::uniform(target, [stiffness, stiffness])));
    for _ in 0..steps {
        layout.step();
    }
    layout.positions().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn radii(points: &[[f32; 2]]) -> Vec<f32> {
        let count = points.len() as f32;
        let mut centre = [0.0f32; 2];
        for point in points {
            centre[0] += point[0];
            centre[1] += point[1];
        }
        centre[0] /= count;
        centre[1] /= count;
        points
            .iter()
            .map(|point| (point[0] - centre[0]).hypot(point[1] - centre[1]))
            .collect()
    }

    #[test]
    fn a_small_mesh_is_a_ring() {
        // The colony the dashboard actually draws today. The ring anchor has
        // to hold it exactly, or a working instrument gets replaced by a
        // wobblier one for no gain.
        for count in [2usize, 3, 6, RING_FULL] {
            let points = settled(count, &[], 600);
            let radii = radii(&points);
            let mean = radii.iter().sum::<f32>() / radii.len() as f32;
            for radius in &radii {
                let error = (radius - mean).abs() / mean;
                assert!(
                    error < 0.01,
                    "count {count}: radius {radius} vs mean {mean}"
                );
            }
        }
    }

    #[test]
    fn a_small_mesh_holds_its_ring_against_observations() {
        // Links pull; the ring must win at this size, because the whole point
        // of keeping the ring is that peers of equal standing are drawn as
        // equals whatever the observation graph happens to look like.
        let links: Vec<MeshLink> = (0..5).map(|from| MeshLink { from, to: 5 }).collect();
        let points = settled(6, &links, 800);
        let radii = radii(&points);
        let mean = radii.iter().sum::<f32>() / radii.len() as f32;
        for radius in &radii {
            assert!((radius - mean).abs() / mean < 0.02, "{radius} vs {mean}");
        }
    }

    #[test]
    fn a_large_mesh_is_not_a_ring() {
        // Past the fade the ring is gone and the layout is free to show what
        // the observations say, which is the whole reason for the change.
        let links: Vec<MeshLink> = (0..199usize)
            .map(|from| MeshLink {
                from,
                to: (from * 7 + 1) % 200,
            })
            .collect();
        let points = settled(200, &links, 1_200);
        let radii = radii(&points);
        let mean = radii.iter().sum::<f32>() / radii.len() as f32;
        let spread = radii
            .iter()
            .map(|radius| (radius - mean).abs() / mean)
            .fold(0.0f32, f32::max);
        assert!(spread > 0.1, "layout stayed a ring: spread {spread}");
    }

    /// What one state's mark is actually made of, counted from the painter.
    ///
    /// Returns `(filled rects, stroked rects, line segments, paths)`. Shape
    /// *kind* alone is not enough — a filled square and an open one are both a
    /// rect — so the fill and the stroke are read too. Counted against a
    /// baseline frame, because a panel paints its own background.
    fn state_shapes(state: MeshNodeState) -> (usize, usize, usize, usize) {
        let count = |state: Option<MeshNodeState>| {
            let ctx = eframe::egui::Context::default();
            let output = ctx.run(Default::default(), |ctx| {
                eframe::egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter().clone();
                    if let Some(state) = state {
                        let (glyph, outline) = mark_for(state);
                        graph::draw_node(
                            &painter,
                            pos2(50.0, 50.0),
                            1.0,
                            glyph,
                            outline,
                            eframe::egui::Color32::WHITE,
                            1,
                        );
                    }
                });
            });
            let mut kinds = [0usize; 4];
            for clipped in &output.shapes {
                match &clipped.shape {
                    eframe::egui::Shape::Rect(rect) => {
                        if rect.fill != eframe::egui::Color32::TRANSPARENT {
                            kinds[0] += 1;
                        } else {
                            kinds[1] += 1;
                        }
                    }
                    eframe::egui::Shape::LineSegment { .. } => kinds[2] += 1,
                    eframe::egui::Shape::Path(_) => kinds[3] += 1,
                    _ => {}
                }
            }
            kinds
        };
        let base = count(None);
        let drawn = count(Some(state));
        (
            drawn[0] - base[0],
            drawn[1] - base[1],
            drawn[2] - base[2],
            drawn[3] - base[3],
        )
    }

    #[test]
    fn every_state_draws_a_different_mark() {
        // The conflation this enum was split to end: a node nobody has heard
        // from and a node whose clock we cannot read used to render
        // identically, so the picture could not answer "are they down, or just
        // quiet". Four states, four marks, and no two the same.
        let all = [
            state_shapes(MeshNodeState::Fresh),
            state_shapes(MeshNodeState::Stale),
            state_shapes(MeshNodeState::Unknown),
            state_shapes(MeshNodeState::Unreported),
        ];
        for (index, left) in all.iter().enumerate() {
            for right in all.iter().skip(index + 1) {
                assert_ne!(left, right, "two states draw the same mark: {all:?}");
            }
        }
        let [fresh, stale, unknown, unreported] = all;
        assert_eq!(fresh.0, 1, "fresh is a filled square");
        assert_eq!(stale.1, 1, "stale is an open square");
        // Both holes are dashed, as absence is dashed everywhere else in this
        // vocabulary, and the slash is the single extra mark that separates a
        // failed attempt from a silence.
        assert_eq!(
            unreported.0 + unreported.1,
            0,
            "a hole draws no solid square"
        );
        assert_eq!(
            unknown.0 + unknown.1,
            0,
            "a failed attempt draws no solid square"
        );
        assert!(unreported.2 >= 4, "a dashed square needs a dash per side");
        assert_eq!(
            unknown.2,
            unreported.2 + 1,
            "the slash is what tells a failed attempt from a silence"
        );
    }

    #[test]
    fn hearsay_nodes_are_peripheral() {
        // A peer named only by a health condition has one observer and nothing
        // pulling it into the middle, so it ends up on the outside. That is
        // the mesh's version of drawing an absence: the shape says "known only
        // by name" and the position says "and only to one of us".
        let mut links: Vec<MeshLink> = Vec::new();
        for from in 0..40usize {
            for to in 0..40usize {
                if from != to && (from + to) % 3 == 0 {
                    links.push(MeshLink { from, to });
                }
            }
        }
        // Node 40 is hearsay: one observer, nothing else.
        links.push(MeshLink { from: 0, to: 40 });
        let points = settled(41, &links, 1_500);
        let radii = radii(&points);
        let hearsay = radii[40];
        let mut core = radii[..40].to_vec();
        core.sort_by(f32::total_cmp);
        let median = core[core.len() / 2];
        assert!(
            hearsay > median,
            "hearsay node at {hearsay} is not outside the median {median}"
        );
    }

    #[test]
    fn the_ring_fades_rather_than_switching_off() {
        // A step change at one node count would make the picture jump when a
        // peer joins, which is the discontinuity the radial placement was
        // chosen to avoid in the first place.
        let gravity = LayoutParams::default().anchor_k;
        let full = ring_stiffness(RING_FULL, gravity);
        let mid = ring_stiffness((RING_FULL + RING_NONE) / 2, gravity);
        let none = ring_stiffness(RING_NONE, gravity);
        assert_eq!(full, RING_K);
        assert!(none < RING_K * 0.01, "ring still holds at {none}");
        assert!(mid < full && mid > none, "fade is not monotone: {mid}");
        assert!(ring_stiffness(1_000, gravity) > 0.0, "gravity well lost");

        // The observation spring fades in as the ring fades out. They are one
        // handover driven by one number, so the layout is never being pulled
        // two ways at once by two settings that drifted apart.
        let shared = LayoutParams::default().attraction;
        assert_eq!(mesh_params(RING_FULL).attraction, 0.0);
        assert_eq!(mesh_params(RING_NONE).attraction, shared);
        assert_eq!(mesh_params(10_000).attraction, shared);
        let middle = mesh_params((RING_FULL + RING_NONE) / 2).attraction;
        assert!(middle > 0.0 && middle < shared, "spring does not fade in");
    }

    #[test]
    fn a_link_naming_a_peer_outside_the_report_is_skipped() {
        // Open world: the report may name a peer no node reported for.
        let points = settled(3, &[MeshLink { from: 0, to: 99 }], 50);
        assert_eq!(points.len(), 3);
        assert!(points.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
    }
}
