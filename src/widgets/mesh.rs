use eframe::egui::{
    pos2, vec2, Align2, Pos2, Response, Sense, Shape, Stroke, TextStyle, Ui, Widget,
};

use crate::themes;

/// Freshness of one node's own report, as observed locally.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeshNodeState {
    /// Reported within the freshness window.
    Fresh,
    /// Last report is older than the window.
    Stale,
    /// Seen only as someone else's peer, or timestamped in the future.
    Unknown,
}

/// One node in the observed mesh.
#[derive(Clone, Debug)]
pub struct MeshNode {
    /// Short handle, drawn outside the ring.
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

/// The observed peer mesh, drawn as a ring of nodes with observation links.
///
/// # Why radial and not force-directed
///
/// A force-directed layout earns its cost on hundreds of nodes with latent
/// structure worth revealing. A peer mesh is a handful of nodes whose only
/// structure is who observed whom, and a simulation over that wobbles between
/// frames without discovering anything: the same report renders differently
/// twice, which is exactly what an instrument must not do. A radial placement
/// is deterministic, stable frame to frame, and gives every node equal
/// prominence, which is honest for peers that are equals.
///
/// Node order is the caller's, so a caller that sorts by handle gets a layout
/// that does not jump when a node's freshness changes.
///
/// # Encoding
///
/// State is carried by **shape**, never by colour alone, so the widget
/// survives colourblind readers, greyscale print and forced-contrast modes:
/// a filled square is fresh, an open square is stale, and an open square with
/// a slash is unknown. The accent is spent only on the direction barbs, which
/// is the one thing a still image cannot otherwise disambiguate.
///
/// ```ignore
/// ui.add(MeshGraph::new(&nodes, &links));
/// ```
#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct MeshGraph<'a> {
    nodes: &'a [MeshNode],
    links: &'a [MeshLink],
    height: f32,
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
        }
    }

    /// Override the drawing height. Width follows the available space.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }
}

/// Radius of the node mark, in points.
const MARK: f32 = 7.0;
/// Radius of the convergence track drawn around each node mark.
const TRACK: f32 = 12.0;
/// How far a link stops short of the node mark, so lines never run under it.
const CLEARANCE: f32 = TRACK + 4.0;

impl Widget for MeshGraph<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(vec2(width, self.height), Sense::hover());
        if !ui.is_rect_visible(rect) || self.nodes.is_empty() {
            return response;
        }

        let visuals = ui.visuals();
        let ink = visuals.text_color();
        let hairline = visuals.widgets.noninteractive.bg_stroke.color;
        let track = themes::blend(visuals.window_fill, hairline, 0.55);
        let accent = themes::button_light_on();
        let painter = ui.painter().with_clip_rect(rect);
        let font = TextStyle::Small.resolve(ui.style());

        // Deterministic radial placement. The first node sits at the top and
        // the rest run clockwise, so the layout is a pure function of order.
        let centre = rect.center();
        let radius = (rect.width().min(rect.height()) * 0.5 - TRACK - 34.0).max(24.0);
        let count = self.nodes.len();
        let positions: Vec<Pos2> = (0..count)
            .map(|index| {
                if count == 1 {
                    return centre;
                }
                let turn = index as f32 / count as f32;
                let angle = turn * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                pos2(
                    centre.x + radius * angle.cos(),
                    centre.y + radius * angle.sin(),
                )
            })
            .collect();

        // Links first, so node marks always sit on top of them.
        for link in self.links {
            let (Some(from), Some(to)) = (positions.get(link.from), positions.get(link.to)) else {
                continue;
            };
            if link.from == link.to {
                continue;
            }
            let along = (*to - *from).normalized();
            let start = *from + along * CLEARANCE;
            let end = *to - along * CLEARANCE;
            painter.line_segment([start, end], Stroke::new(1.0_f32, hairline));

            // A direction barb near the target: a short perpendicular tick
            // rather than an arrowhead, matching the measurement language of
            // the réseau marks instead of importing a diagram convention.
            let barb_at = end - along * 9.0;
            let across = vec2(-along.y, along.x) * 4.0;
            painter.line_segment(
                [barb_at - across, barb_at + across],
                Stroke::new(1.5_f32, accent),
            );
        }

        for (index, node) in self.nodes.iter().enumerate() {
            let at = positions[index];

            // Convergence track, then the filled arc over it. An empty track
            // reads as "no evidence", which is distinct from a full ring.
            painter.add(Shape::line(
                arc_points(at, TRACK, 0.0, std::f32::consts::TAU),
                Stroke::new(1.0_f32, track),
            ));
            if let Some(ratio) = node.convergence {
                let sweep = ratio.clamp(0.0, 1.0) * std::f32::consts::TAU;
                if sweep > f32::EPSILON {
                    painter.add(Shape::line(
                        arc_points(at, TRACK, -std::f32::consts::FRAC_PI_2, sweep),
                        Stroke::new(2.0_f32, ink),
                    ));
                }
            }

            // Shape carries state; colour never carries it alone.
            let mark = eframe::egui::Rect::from_center_size(at, vec2(MARK * 2.0, MARK * 2.0));
            match node.state {
                MeshNodeState::Fresh => {
                    painter.rect_filled(mark, 0.0, ink);
                }
                MeshNodeState::Stale => {
                    painter.rect_stroke(
                        mark,
                        0.0,
                        Stroke::new(1.5_f32, ink),
                        eframe::egui::StrokeKind::Inside,
                    );
                }
                MeshNodeState::Unknown => {
                    painter.rect_stroke(
                        mark,
                        0.0,
                        Stroke::new(1.5_f32, ink),
                        eframe::egui::StrokeKind::Inside,
                    );
                    painter.line_segment(
                        [mark.left_bottom(), mark.right_top()],
                        Stroke::new(1.5_f32, ink),
                    );
                }
            }

            // Labels sit outside the ring, pushed away from the centre so they
            // never collide with the marks or the links.
            let outward = if count == 1 {
                vec2(0.0, 1.0)
            } else {
                (at - centre).normalized()
            };
            let anchor = at + outward * (TRACK + 10.0);
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

        response
    }
}

/// Sample an arc as a polyline. egui has no arc primitive, and sampling keeps
/// the stroke joins consistent with every other line the theme draws.
fn arc_points(centre: Pos2, radius: f32, start: f32, sweep: f32) -> Vec<Pos2> {
    let steps = ((sweep.abs() / std::f32::consts::TAU) * 64.0)
        .ceil()
        .max(2.0) as usize;
    (0..=steps)
        .map(|step| {
            let angle = start + sweep * (step as f32 / steps as f32);
            pos2(
                centre.x + radius * angle.cos(),
                centre.y + radius * angle.sin(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_is_sampled_with_endpoints_on_the_circle() {
        let points = arc_points(pos2(10.0, 10.0), 5.0, 0.0, std::f32::consts::TAU);
        assert!(points.len() >= 3);
        for point in &points {
            let dx = point.x - 10.0;
            let dy = point.y - 10.0;
            assert!((dx.hypot(dy) - 5.0).abs() < 1e-3, "point off the circle");
        }
    }

    #[test]
    fn a_zero_sweep_still_yields_a_usable_polyline() {
        // A node with 0% convergence must not produce a degenerate shape.
        let points = arc_points(pos2(0.0, 0.0), 4.0, 0.0, 0.0);
        assert_eq!(points.len(), 3);
    }
}
