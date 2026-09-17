use eframe::egui::{pos2, vec2, Pos2, Rect, Response, Sense, Shape, Stroke, TextStyle, Ui, Widget};

use crate::themes;

/// What kind of element a node stands for.
///
/// The collection algebra has exactly one meaningful split here, and it is not
/// "collection versus member": it is whether the element was **asserted** or
/// **computed**. A `COMMIT` is exogenous — no machine can recompute whether an
/// author meant to publish it — while a `MERGE` result, a `DERIVE` output and a
/// derived descriptor are all reproducible from their inputs. That is the
/// distinction a reader needs in order to know what would survive losing the
/// store's materialized work, so it is the one the shape carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LatticeMark {
    /// Asserted by an author: a `COMMIT` member, or a named root collection.
    Authored,
    /// Reproducible from inputs: a `MERGE` result, a `DERIVE` output, or a
    /// derived collection descriptor.
    Computed,
}

/// Whether the element is resident in the observation being drawn.
///
/// Absence is a first-class value here because a store is an open world: a
/// record may name a descriptor, an input or an output whose bytes simply are
/// not here. Omitting such a node would quietly redraw the lattice as though it
/// were complete, which is the failure this widget exists to prevent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LatticePresence {
    /// The element's bytes are resident in this observation.
    Present,
    /// The element is named or implied, but not resident. Drawn as an explicit
    /// hole — a dashed outline — never omitted.
    Absent,
}

/// One element of the lattice.
#[derive(Clone, Debug)]
pub struct LatticeNode {
    /// Short handle or name, drawn under the mark.
    pub label: String,
    /// Asserted or computed.
    pub mark: LatticeMark,
    /// Resident here, or a hole.
    pub presence: LatticePresence,
    /// Materialized share of the work this node stands for, in `0.0..=1.0`.
    ///
    /// For a collection node this is endorsed equations over expected ones: the
    /// derive backlog, drawn rather than described. `None` means the caller had
    /// nothing comparable to divide, and draws as an **empty track** — absence
    /// of evidence is not completeness, and the two must not look alike.
    pub coverage: Option<f32>,
}

/// A directed edge: `from` is below `to` in the order — a source, a merge
/// input, or the collection a derivation reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LatticeEdge {
    /// Index of the lower element.
    pub from: usize,
    /// Index of the upper element.
    pub to: usize,
    /// `true` when a stored record endorses this step.
    ///
    /// `false` is the other half of "what is missing": the model says this
    /// equation should exist — a source member with no derivation, a pair the
    /// join never reached — and no record endorses it. It draws dashed, so an
    /// unfinished lattice looks unfinished.
    pub endorsed: bool,
}

/// What the pointer did to the lattice this frame.
pub struct LatticeResponse {
    /// The allocated region's own response.
    pub response: Response,
    /// Node under the pointer, if the pointer is near one.
    pub hovered: Option<usize>,
    /// Node clicked this frame, if any.
    pub clicked: Option<usize>,
}

/// A collection lattice, drawn as a layered Hasse diagram.
///
/// # Why layered and not force-directed
///
/// The thing being drawn is a **partial order**, and a partial order already
/// has a canonical picture: rank every element by its longest path from a
/// minimal one and draw the ranks as parallel columns. A force-directed layout
/// would discard exactly the structure that matters — it would place a source
/// and its third-generation derivation wherever the springs settled, and settle
/// differently on the next frame. Ranking is a pure function of the edge set,
/// so the same records always draw the same picture and two observations can be
/// compared by eye. Columns run left to right because that is where a reader
/// already looks for "came from": a node's sources are always to its left, its
/// dependents always to its right, which is why no arrowheads are needed.
///
/// # Encoding
///
/// Three facts, three independent channels, none of them colour alone:
///
/// * **Shape** — square is authored, circle is computed.
/// * **Stroke** — solid is resident, dashed is a hole. Dashed edges are
///   equations the model expects and no record endorses.
/// * **Arc** — the track around a mark is materialized work over expected
///   work. An empty track is no evidence, not agreement.
///
/// The accent is spent on one thing only: the ring around the selected node.
/// Direction, rank and state are all carried by geometry, so saturation is left
/// for the single fact geometry cannot carry — which of these did you ask about.
///
/// # Selection
///
/// Passing `selected` dims everything that is not on that node's chain, so a
/// derived collection can be walked back to the sources it was computed from.
/// The chain is the transitive closure in **both** directions: every element
/// that feeds the selection and everything the selection feeds. Unendorsed
/// edges are part of it, because the missing step is exactly what a reader
/// following a broken chain needs to see.
///
/// ```ignore
/// let lattice = LatticeGraph::new(&nodes, &edges)
///     .selected(selected)
///     .show(ui);
/// if let Some(index) = lattice.clicked {
///     selected = Some(index);
/// }
/// ```
#[must_use = "You should show this widget with `.show(ui)` or `ui.add(widget)`"]
pub struct LatticeGraph<'a> {
    nodes: &'a [LatticeNode],
    edges: &'a [LatticeEdge],
    selected: Option<usize>,
    height: Option<f32>,
}

impl<'a> LatticeGraph<'a> {
    /// A lattice of `nodes` ordered by `edges`.
    ///
    /// Edges whose endpoints fall outside `nodes` are skipped rather than
    /// panicking: the records are an open-world observation and may name an
    /// element the caller did not include.
    pub fn new(nodes: &'a [LatticeNode], edges: &'a [LatticeEdge]) -> Self {
        Self {
            nodes,
            edges,
            selected: None,
            height: None,
        }
    }

    /// Dim everything off this node's chain. Out-of-range indices select
    /// nothing, which keeps a stale selection from blanking the view.
    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    /// Fix the drawing height. Width always follows the available space.
    ///
    /// Left alone, the height is computed from the layout: one row pitch per
    /// node in the busiest column. A three-node lattice then takes three rows
    /// of space instead of reserving a screenful and drawing a sparse picture
    /// in it, and a caller cannot make the marks collide by guessing low.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Draw the lattice and report what the pointer did.
    pub fn show(self, ui: &mut Ui) -> LatticeResponse {
        let count = self.nodes.len();
        let ranks = levels(count, self.edges);
        let width = ui.available_width();
        let height = self.height.unwrap_or_else(|| {
            let rows = (0..count)
                .map(|node| ranks.iter().filter(|rank| **rank == ranks[node]).count())
                .max()
                .unwrap_or(1);
            (rows as f32 * PITCH + TOP + BOTTOM).max(PITCH + TOP + BOTTOM)
        });
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
        if !ui.is_rect_visible(rect) || self.nodes.is_empty() {
            return LatticeResponse {
                response,
                hovered: None,
                clicked: None,
            };
        }

        let visuals = ui.visuals();
        let ink = visuals.text_color();
        let hairline = visuals.widgets.noninteractive.bg_stroke.color;
        let dim = themes::blend(visuals.window_fill, ink, 0.32);
        let track = themes::blend(visuals.window_fill, hairline, 0.55);
        let accent = themes::button_light_on();
        let painter = ui.painter().with_clip_rect(rect);
        let font = TextStyle::Small.resolve(ui.style());

        let positions = place(rect, &ranks);
        let lit = match self.selected.filter(|index| *index < count) {
            Some(index) => chain(count, self.edges, index),
            None => vec![true; count],
        };

        // Edges first, so marks always sit on top of them.
        for edge in self.edges {
            let (Some(from), Some(to)) = (positions.get(edge.from), positions.get(edge.to)) else {
                continue;
            };
            if edge.from == edge.to {
                continue;
            }
            let on_chain = lit[edge.from] && lit[edge.to];
            let colour = if on_chain { hairline } else { dim };
            let along = (*to - *from).normalized();
            let start = *from + along * CLEARANCE;
            let end = *to - along * CLEARANCE;
            if edge.endorsed {
                painter.line_segment([start, end], Stroke::new(1.0_f32, colour));
            } else {
                painter.extend(Shape::dashed_line(
                    &[start, end],
                    Stroke::new(1.0_f32, colour),
                    4.0,
                    4.0,
                ));
            }
        }

        for (index, node) in self.nodes.iter().enumerate() {
            let at = positions[index];
            let colour = if lit[index] { ink } else { dim };

            // Coverage track, then the materialized arc over it. Drawn for
            // every node so a full ring and an empty one sit in the same place
            // and can be told apart at a glance.
            painter.add(Shape::line(
                arc_points(at, TRACK, 0.0, std::f32::consts::TAU),
                Stroke::new(1.0_f32, if lit[index] { track } else { dim }),
            ));
            if let Some(ratio) = node.coverage {
                let sweep = ratio.clamp(0.0, 1.0) * std::f32::consts::TAU;
                if sweep > f32::EPSILON {
                    painter.add(Shape::line(
                        arc_points(at, TRACK, -std::f32::consts::FRAC_PI_2, sweep),
                        Stroke::new(2.0_f32, colour),
                    ));
                }
            }

            draw_mark(&painter, at, node, colour);

            if self.selected == Some(index) {
                painter.circle_stroke(at, TRACK + 4.0, Stroke::new(1.5_f32, accent));
            }

            // Labels are centred under the mark, then slid back inside the
            // region. The end columns sit close to the edges, so a centred
            // label there would run off and be clipped mid-word — which loses
            // the very name the reader needs to tell two collections apart.
            let galley = painter.layout_no_wrap(node.label.clone(), font.clone(), colour);
            let left = (at.x - galley.size().x * 0.5).clamp(
                rect.left(),
                (rect.right() - galley.size().x).max(rect.left()),
            );
            painter.galley(pos2(left, at.y + TRACK + 3.0), galley, colour);
        }

        let near = |at: Pos2| {
            positions
                .iter()
                .enumerate()
                .map(|(index, node)| (index, (*node - at).length()))
                .filter(|(_, distance)| *distance <= HIT)
                .min_by(|left, right| left.1.total_cmp(&right.1))
                .map(|(index, _)| index)
        };
        let hovered = response.hover_pos().and_then(near);
        let clicked = response
            .clicked()
            .then(|| response.interact_pointer_pos().and_then(near))
            .flatten();

        LatticeResponse {
            response,
            hovered,
            clicked,
        }
    }
}

impl Widget for LatticeGraph<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui).response
    }
}

/// Half-width of a node mark, in points.
const MARK: f32 = 6.0;
/// Radius of the coverage track drawn around each mark.
const TRACK: f32 = 11.0;
/// How far an edge stops short of a mark, so lines never run under it.
const CLEARANCE: f32 = TRACK + 4.0;
/// Pointer distance that counts as touching a node.
const HIT: f32 = TRACK + 6.0;
/// Space kept clear at the left and right edges so end labels stay readable.
const SIDE: f32 = 34.0;
/// Vertical distance between two marks sharing a column.
const PITCH: f32 = 46.0;
/// Space kept clear above a mark, and below it for the label.
const TOP: f32 = TRACK + 4.0;
/// Space kept clear under the lowest mark, sized for its label.
const BOTTOM: f32 = TRACK + 18.0;

fn draw_mark(
    painter: &eframe::egui::Painter,
    at: Pos2,
    node: &LatticeNode,
    colour: eframe::egui::Color32,
) {
    let stroke = Stroke::new(1.5_f32, colour);
    match (node.mark, node.presence) {
        (LatticeMark::Authored, LatticePresence::Present) => {
            painter.rect_filled(
                Rect::from_center_size(at, vec2(MARK * 2.0, MARK * 2.0)),
                0.0,
                colour,
            );
        }
        (LatticeMark::Authored, LatticePresence::Absent) => {
            let mark = Rect::from_center_size(at, vec2(MARK * 2.0, MARK * 2.0));
            painter.extend(Shape::dashed_line(
                &[
                    mark.left_top(),
                    mark.right_top(),
                    mark.right_bottom(),
                    mark.left_bottom(),
                    mark.left_top(),
                ],
                stroke,
                3.0,
                3.0,
            ));
        }
        (LatticeMark::Computed, LatticePresence::Present) => {
            painter.circle_filled(at, MARK, colour);
        }
        (LatticeMark::Computed, LatticePresence::Absent) => {
            painter.extend(Shape::dashed_line(
                &arc_points(at, MARK, 0.0, std::f32::consts::TAU),
                stroke,
                3.0,
                3.0,
            ));
        }
    }
}

/// Rank every node by its longest path from a minimal element.
///
/// This is the layout's whole geometry: a node's rank is its column. Ranking by
/// *longest* path rather than shortest is what keeps a node strictly to the
/// right of every one of its sources even when a short edge skips a generation,
/// which is the property that makes the picture readable as an order.
///
/// Unendorsed edges rank exactly like endorsed ones. A missing derivation still
/// says where its output would sit, and placing the hole in the right column is
/// the point of drawing it at all.
fn levels(count: usize, edges: &[LatticeEdge]) -> Vec<usize> {
    let mut level = vec![0usize; count];
    let mut indegree = vec![0usize; count];
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); count];
    for edge in edges {
        if edge.from >= count || edge.to >= count || edge.from == edge.to {
            continue;
        }
        indegree[edge.to] += 1;
        outgoing[edge.from].push(edge.to);
    }

    // Kahn's algorithm seeded in index order, so the ranking is a pure function
    // of the input and never depends on hash or iteration order.
    let mut queue: Vec<usize> = (0..count).filter(|node| indegree[*node] == 0).collect();
    let mut settled = vec![false; count];
    let mut head = 0;
    while head < queue.len() {
        let node = queue[head];
        head += 1;
        settled[node] = true;
        for next in outgoing[node].clone() {
            level[next] = level[next].max(level[node] + 1);
            indegree[next] -= 1;
            if indegree[next] == 0 {
                queue.push(next);
            }
        }
    }

    // A cycle has no rank. Records should not produce one, but the reader is
    // open-world and must not hang or panic on bytes it did not write: park
    // every unranked node one column past everything that could be ranked, so
    // the nodes and their edges are still drawn and still obviously odd.
    let parked = level.iter().copied().max().map_or(0, |max| max + 1);
    for node in 0..count {
        if !settled[node] {
            level[node] = parked;
        }
    }
    level
}

/// Every node that feeds `selected`, everything `selected` feeds, and itself.
///
/// Both directions, because a reader asking "where did this come from" almost
/// always then asks "and what else was built on it". Unendorsed edges are
/// followed too: a chain that is broken is still the chain you asked for, and
/// hiding the break would answer the question wrongly.
fn chain(count: usize, edges: &[LatticeEdge], selected: usize) -> Vec<bool> {
    let mut on = vec![false; count];
    if selected >= count {
        return on;
    }
    on[selected] = true;
    for downward in [true, false] {
        let mut frontier = vec![selected];
        while let Some(node) = frontier.pop() {
            for edge in edges {
                if edge.from >= count || edge.to >= count {
                    continue;
                }
                let next = if downward {
                    (edge.to == node).then_some(edge.from)
                } else {
                    (edge.from == node).then_some(edge.to)
                };
                if let Some(next) = next {
                    if !on[next] {
                        on[next] = true;
                        frontier.push(next);
                    }
                }
            }
        }
    }
    on
}

/// Turn ranks into points: one column per rank, nodes stacked within it in
/// index order so a caller that sorts its nodes gets a layout that does not
/// jump when one node's presence changes.
fn place(rect: Rect, ranks: &[usize]) -> Vec<Pos2> {
    let columns = ranks.iter().copied().max().map_or(0, |max| max + 1);
    let span = (rect.width() - 2.0 * SIDE).max(1.0);
    let column_x = |rank: usize| {
        if columns <= 1 {
            rect.center().x
        } else {
            rect.left() + SIDE + span * (rank as f32 / (columns - 1) as f32)
        }
    };
    let height = (rect.height() - TOP - BOTTOM).max(1.0);
    let middle = rect.top() + TOP + height * 0.5;

    ranks
        .iter()
        .enumerate()
        .map(|(index, rank)| {
            let siblings: Vec<usize> = ranks
                .iter()
                .enumerate()
                .filter(|(_, other)| *other == rank)
                .map(|(other, _)| other)
                .collect();
            let row = siblings
                .iter()
                .position(|other| *other == index)
                .unwrap_or(0);
            // A fixed pitch, centred: a column of three marks is three marks
            // tall wherever it sits, so two columns are comparable by eye
            // instead of being stretched apart by however much room the
            // caller happened to give. Only a column that would otherwise
            // overflow is compressed, because a mark outside the allocated
            // region is clipped away and silently missing.
            let pitch = match siblings.len() > 1 {
                true => PITCH.min(height / (siblings.len() - 1) as f32),
                false => PITCH,
            };
            let offset = row as f32 - (siblings.len() as f32 - 1.0) * 0.5;
            pos2(column_x(*rank), middle + offset * pitch)
        })
        .collect()
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

    fn edge(from: usize, to: usize) -> LatticeEdge {
        LatticeEdge {
            from,
            to,
            endorsed: true,
        }
    }

    #[test]
    fn a_source_chain_ranks_one_node_per_column() {
        // facts -> index -> summary: exactly the derive chain the view exists
        // to walk, and it must read left to right without stacking.
        let ranks = levels(3, &[edge(0, 1), edge(1, 2)]);
        assert_eq!(ranks, vec![0, 1, 2]);
    }

    #[test]
    fn a_join_sits_past_the_longer_of_its_two_arms() {
        // 0 -> 1 -> 2 -> 3 and 0 -> 3. Shortest-path ranking would put the
        // join in column 1 and draw an edge running backwards.
        let ranks = levels(4, &[edge(0, 1), edge(1, 2), edge(2, 3), edge(0, 3)]);
        assert_eq!(ranks, vec![0, 1, 2, 3]);
    }

    #[test]
    fn an_unendorsed_edge_ranks_its_missing_output() {
        // The whole point of drawing a hole is that it lands in the column
        // where the derivation would have gone.
        let ranks = levels(
            2,
            &[LatticeEdge {
                from: 0,
                to: 1,
                endorsed: false,
            }],
        );
        assert_eq!(ranks, vec![0, 1]);
    }

    #[test]
    fn a_cycle_is_parked_instead_of_hanging() {
        // Records should never produce one, but an open-world reader must not
        // spin or panic on bytes it did not write.
        let ranks = levels(3, &[edge(0, 1), edge(1, 2), edge(2, 1)]);
        assert_eq!(ranks.len(), 3);
        assert_eq!(ranks[0], 0);
        assert!(
            ranks[1] > 0 && ranks[2] > 0,
            "cycle nodes still get a column"
        );
    }

    #[test]
    fn out_of_range_and_self_edges_are_skipped_not_panics() {
        let ranks = levels(2, &[edge(0, 9), edge(7, 1), edge(1, 1), edge(0, 1)]);
        assert_eq!(ranks, vec![0, 1]);
    }

    #[test]
    fn the_chain_reaches_transitive_sources_and_leaves_siblings_out() {
        // 0 -> 1 -> 2 is the chain; 3 -> 2 is a second source of the same
        // target and belongs in it; 4 is unrelated and must stay dimmed.
        let lit = chain(5, &[edge(0, 1), edge(1, 2), edge(3, 2)], 2);
        assert_eq!(lit, vec![true, true, true, true, false]);
    }

    #[test]
    fn the_chain_includes_what_was_built_on_the_selection() {
        let lit = chain(3, &[edge(0, 1), edge(1, 2)], 0);
        assert_eq!(lit, vec![true, true, true]);
    }

    #[test]
    fn a_selection_past_the_end_lights_nothing() {
        // A stale selection must not blank the view by lighting everything,
        // nor light a node that is not the one that was asked about.
        let lit = chain(2, &[edge(0, 1)], 9);
        assert_eq!(lit, vec![false, false]);
    }

    #[test]
    fn a_single_column_is_centred_rather_than_pinned_left() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 100.0));
        let points = place(rect, &[0]);
        assert_eq!(points.len(), 1);
        assert!((points[0].x - 100.0).abs() < 1e-3);
    }

    #[test]
    fn columns_run_left_to_right_and_siblings_stack() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 200.0));
        // Ranks 0, 1, 1: one source feeding two outputs.
        let points = place(rect, &[0, 1, 1]);
        assert!(
            points[0].x < points[1].x,
            "a source sits left of its output"
        );
        assert!(
            (points[1].x - points[2].x).abs() < 1e-3,
            "one rank, one column"
        );
        assert!(points[1].y < points[2].y, "siblings stack in index order");
        for point in &points {
            assert!(rect.contains(*point), "every mark stays inside the region");
        }
    }

    #[test]
    fn a_column_uses_a_fixed_pitch_rather_than_the_whole_height() {
        // Two marks in a tall region must sit one pitch apart and centred, not
        // be flung to the top and bottom edges with emptiness between them.
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 600.0));
        let points = place(rect, &[0, 0]);
        assert!(
            (points[1].y - points[0].y - PITCH).abs() < 1e-3,
            "siblings are one pitch apart"
        );
        let middle = (points[0].y + points[1].y) * 0.5;
        let expected = rect.top() + TOP + (rect.height() - TOP - BOTTOM) * 0.5;
        assert!(
            (middle - expected).abs() < 1e-3,
            "and centred in the region"
        );
    }

    #[test]
    fn a_column_that_cannot_fit_is_compressed_instead_of_overflowing() {
        // Many marks in a short region must still land inside it: a mark drawn
        // outside the allocated rect is clipped away and silently missing.
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 120.0));
        let ranks = vec![0; 12];
        for point in place(rect, &ranks) {
            assert!(
                point.y >= rect.top() && point.y <= rect.bottom(),
                "every mark stays inside the region"
            );
        }
    }

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
}
