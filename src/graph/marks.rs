//! The drawing kit the three graph views share.
//!
//! This exists because `arc_points` was written twice — byte-identical bodies
//! in `widgets/mesh.rs` and `widgets/lattice.rs` — while the constants beside
//! it had already drifted apart (`MARK` 7.0 against 6.0, `TRACK` 12.0 against
//! 11.0). Two copies of a function and one copy of its meaning is how a
//! vocabulary stops being one vocabulary. The unified values are the lattice's,
//! because they are the later ones and the ones that have to survive dense
//! packing; the mesh's marks are one point smaller as a result, and that is the
//! only cosmetic change the unification makes.
//!
//! # Encoding
//!
//! Every fact these views carry is in **shape, stroke, arc or position**, never
//! in colour alone. That is not a preference. GORBIE's palette is a true
//! inversion whose endpoints are 16.03:1 apart, which caps any single value
//! against either ground at `sqrt(16.03) = 4.00:1` — under the 4.5:1 text
//! threshold. Colour here cannot carry a fact on its own even when it looks
//! like it does, which is why links carry mandatory underlines elsewhere and
//! why state is carried by geometry here.

use eframe::egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};

/// Half-width of a node mark, in points.
pub const MARK: f32 = 6.0;
/// Radius of the track drawn around each mark.
pub const TRACK: f32 = 11.0;
/// How far a link stops short of a mark, so lines never run under it.
pub const CLEARANCE: f32 = TRACK + 4.0;
/// Pointer distance that counts as touching a node.
pub const HIT: f32 = TRACK + 6.0;

/// What kind of thing a mark stands for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Glyph {
    /// Asserted by an author, or reported for itself.
    Square,
    /// Reproducible from inputs.
    Circle,
    /// Known only as someone else's peer — named, never heard from.
    SquareSlashed,
    /// Reached only as an input: nothing below here produced it.
    ///
    /// The gap sits at the bottom because that is the direction the missing
    /// producer would have come from, and because a bite out of a mark reads as
    /// incompleteness without needing a legend.
    CircleOpenBelow,
    /// Several marks standing in for many, at a zoom where each would be under
    /// a pixel. Area is proportional to the count it folds.
    Aggregate,
}

/// How the mark's outline is drawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stroke2 {
    /// Resident, and whole.
    Filled,
    /// Resident, but not asserted here.
    Open,
    /// A hole: named or implied, not resident. Never omitted, because omitting
    /// it would redraw an incomplete picture as a complete one.
    Dashed,
}

/// Sample an arc as a polyline. egui has no arc primitive, and sampling keeps
/// the stroke joins consistent with every other line the theme draws.
pub fn arc_points(centre: Pos2, radius: f32, start: f32, sweep: f32) -> Vec<Pos2> {
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

/// Draw one node mark.
///
/// `scale` multiplies [`MARK`], so a caller can size by zoom or by degree
/// without the kit having an opinion about which. `count` is only read for
/// [`Glyph::Aggregate`], where it sets the radius so that **area** is
/// proportional to the number folded — four times the members is twice the
/// radius, which is the honest mapping and the one a reader estimates
/// correctly.
pub fn draw_node(
    painter: &Painter,
    at: Pos2,
    scale: f32,
    glyph: Glyph,
    outline: Stroke2,
    colour: Color32,
    count: usize,
) {
    let radius = MARK * scale.max(0.05);
    let stroke = Stroke::new(1.5_f32 * scale.clamp(0.5, 2.0), colour);
    match glyph {
        Glyph::Square | Glyph::SquareSlashed => {
            let mark = Rect::from_center_size(at, vec2(radius * 2.0, radius * 2.0));
            match outline {
                Stroke2::Filled => {
                    painter.rect_filled(mark, 0.0, colour);
                }
                Stroke2::Open => {
                    painter.rect_stroke(mark, 0.0, stroke, StrokeKind::Inside);
                }
                Stroke2::Dashed => painter.extend(Shape::dashed_line(
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
                )),
            }
            if glyph == Glyph::SquareSlashed {
                painter.line_segment([mark.left_bottom(), mark.right_top()], stroke);
            }
        }
        Glyph::Circle => match outline {
            Stroke2::Filled => {
                painter.circle_filled(at, radius, colour);
            }
            Stroke2::Open => {
                painter.circle_stroke(at, radius, stroke);
            }
            Stroke2::Dashed => painter.extend(Shape::dashed_line(
                &arc_points(at, radius, 0.0, std::f32::consts::TAU),
                stroke,
                3.0,
                3.0,
            )),
        },
        // Always an outline: a bite taken out of a filled disc cannot be drawn
        // without knowing the ground behind it, and asking every caller for its
        // background to draw one glyph is a worse trade than one open shape.
        Glyph::CircleOpenBelow => {
            let gap = std::f32::consts::FRAC_PI_3;
            let start = std::f32::consts::FRAC_PI_2 + gap * 0.5;
            let sweep = std::f32::consts::TAU - gap;
            let points = arc_points(at, radius, start, sweep);
            match outline {
                Stroke2::Dashed => painter.extend(Shape::dashed_line(&points, stroke, 3.0, 3.0)),
                _ => {
                    painter.add(Shape::line(points, Stroke::new(2.0_f32, colour)));
                }
            }
        }
        Glyph::Aggregate => {
            let ring = radius * (count.max(1) as f32).sqrt();
            match outline {
                Stroke2::Dashed => painter.extend(Shape::dashed_line(
                    &arc_points(at, ring, 0.0, std::f32::consts::TAU),
                    stroke,
                    3.0,
                    3.0,
                )),
                _ => {
                    painter.circle_stroke(at, ring, stroke);
                    // The inner mark is the point where the members unfold, so
                    // it is a FIXED size and not a fraction of the ring.
                    // Scaling it with the count turns a row of aggregates into
                    // a row of eyeballs, and says nothing the ring does not
                    // already say.
                    painter.circle_filled(at, 1.5 * scale.max(0.05), colour);
                }
            }
        }
    }
}

/// Draw the track around a mark, the datum tick that says a measurement
/// exists, and the arc that says how much.
///
/// `ratio` of `None` draws an **empty track**, deliberately not a full ring.
/// Absence of evidence is not completeness, and the two must not look alike —
/// which is why the track is drawn for every node, so an empty one and a full
/// one sit in the same place and can be told apart at a glance.
///
/// That was not enough, and the gap is the one thing these views exist to
/// prevent. A node successfully observed and reporting **exactly zero** drew an
/// empty track too, because a zero sweep paints nothing — so a measured zero
/// and a missing measurement were the same picture. The **datum tick** is the
/// fix: a short radial mark at the track's twelve o'clock origin, drawn
/// whenever a measurement exists *of any value including zero*, and omitted
/// when there is none.
///
/// The tick sits strictly **outside** the track, and that placement is
/// load-bearing rather than aesthetic. Drawn across the track it lands exactly
/// where the arc begins and ends, so a ninety-four percent ring reads as a
/// closed ring with a tick on it rather than as six percent short — the tick
/// fills in the very gap it exists to let you see. Outside, it is an index mark
/// beside a gauge and the gap beneath it is the shortfall.
pub fn draw_track(
    painter: &Painter,
    at: Pos2,
    scale: f32,
    ratio: Option<f32>,
    track: Color32,
    ink: Color32,
) {
    let scale = scale.max(0.05);
    let radius = TRACK * scale;
    painter.add(Shape::line(
        arc_points(at, radius, 0.0, std::f32::consts::TAU),
        Stroke::new(1.0_f32, track),
    ));
    let Some(ratio) = ratio else { return };

    let origin = -std::f32::consts::FRAC_PI_2;
    let inner = at + vec2(origin.cos(), origin.sin()) * (radius + 1.5 * scale);
    let outer = at + vec2(origin.cos(), origin.sin()) * (radius + 4.5 * scale);
    painter.line_segment([inner, outer], Stroke::new(1.5_f32, ink));

    let sweep = ratio.clamp(0.0, 1.0) * std::f32::consts::TAU;
    if sweep <= f32::EPSILON {
        return;
    }
    painter.add(Shape::line(
        arc_points(at, radius, origin, sweep),
        Stroke::new(2.0_f32, ink),
    ));
}

/// Draw one link between two marks.
///
/// `endorsed` of `false` draws dashed: the model says this equation should
/// exist and no record endorses it, so an unfinished lattice looks unfinished.
/// `barb` paints a short perpendicular tick near the target — a measurement
/// mark rather than an imported diagram arrowhead — and is the one place the
/// accent is spent, because direction is the one thing a still image cannot
/// otherwise disambiguate.
///
/// `weight` is the stroke width, and it is how a link recedes as the level of
/// detail drops. It recedes by getting **thinner**, never by getting lighter:
/// a link lightened toward the page ground walks through the réseau that
/// measures the field and ends up behind the instrument — measured at 45%
/// receded, a wire lands 1.05:1 against the réseau, and by the lowest level it
/// is lighter than the field it is drawn on. Thinning keeps every link forward
/// of the floor at every zoom. Colour-based recession needs a réseau to recede
/// *toward*, and there is not one here yet.
pub fn draw_link(
    painter: &Painter,
    from: Pos2,
    to: Pos2,
    scale: f32,
    endorsed: bool,
    barb: Option<Color32>,
    colour: Color32,
    weight: f32,
) {
    let delta = to - from;
    if delta.length() < f32::EPSILON {
        return;
    }
    let along = delta.normalized();
    let clearance = CLEARANCE * scale.max(0.05);
    // A link shorter than twice the clearance would be drawn backwards. Draw
    // nothing rather than a reversed segment: at that separation the two marks
    // already overlap and the reader can see they touch.
    if delta.length() <= clearance * 2.0 {
        return;
    }
    let start = from + along * clearance;
    let end = to - along * clearance;
    let stroke = Stroke::new(weight.clamp(0.25, 4.0), colour);
    if endorsed {
        painter.line_segment([start, end], stroke);
    } else {
        painter.extend(Shape::dashed_line(&[start, end], stroke, 4.0, 4.0));
    }
    let Some(accent) = barb else { return };
    let barb_at = end - along * 9.0 * scale.clamp(0.5, 1.5);
    let across = vec2(-along.y, along.x) * 4.0 * scale.clamp(0.5, 1.5);
    painter.line_segment(
        [barb_at - across, barb_at + across],
        Stroke::new(1.5_f32, accent),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_is_sampled_with_endpoints_on_the_circle() {
        // This test lives here once. Its absence from `widgets/mesh.rs` and
        // `widgets/lattice.rs` is itself the check that the two copies are
        // gone: a third copy of `arc_points` would need a third copy of this.
        let points = arc_points(pos2(10.0, 10.0), 5.0, 0.0, std::f32::consts::TAU);
        assert!(points.len() >= 3);
        for point in &points {
            let offset = (point.x - 10.0).hypot(point.y - 10.0);
            assert!((offset - 5.0).abs() < 1e-3, "point off the circle");
        }
    }

    #[test]
    fn a_zero_sweep_still_yields_a_usable_polyline() {
        // A node with 0% coverage must not produce a degenerate shape.
        assert_eq!(arc_points(pos2(0.0, 0.0), 4.0, 0.0, 0.0).len(), 3);
    }

    fn drawn(f: impl FnOnce(&Painter)) -> Vec<eframe::egui::Shape> {
        // Counted against a baseline frame, because a panel paints its own
        // background and an absolute count would be off by whatever the frame
        // happens to contain.
        let render = |draw: Option<&dyn Fn(&Painter)>| {
            let ctx = eframe::egui::Context::default();
            let output = ctx.run(Default::default(), |ctx| {
                eframe::egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter().clone();
                    if let Some(draw) = draw {
                        draw(&painter);
                    }
                });
            });
            output
                .shapes
                .into_iter()
                .map(|clipped| clipped.shape)
                .collect::<Vec<_>>()
        };
        let base = render(None).len();
        let cell = std::cell::RefCell::new(Some(f));
        let call = move |painter: &Painter| {
            if let Some(f) = cell.borrow_mut().take() {
                f(painter);
            }
        };
        let mut all = render(Some(&call));
        all.drain(..base);
        all
    }

    #[test]
    fn a_measured_zero_is_not_a_missing_measurement() {
        // The defect this widget exists to prevent, and it was in the widget:
        // a zero sweep paints nothing, so a node observed successfully and
        // reporting exactly zero drew the same empty track as a node nobody
        // measured. The datum tick is what tells them apart.
        let ink = Color32::WHITE;
        let track = Color32::GRAY;
        let nothing = drawn(|p| draw_track(p, pos2(80.0, 80.0), 1.0, None, track, ink));
        let zero = drawn(|p| draw_track(p, pos2(80.0, 80.0), 1.0, Some(0.0), track, ink));
        let some = drawn(|p| draw_track(p, pos2(80.0, 80.0), 1.0, Some(0.6), track, ink));

        let ticks = |shapes: &[eframe::egui::Shape]| {
            shapes
                .iter()
                .filter(|shape| matches!(shape, eframe::egui::Shape::LineSegment { .. }))
                .count()
        };
        assert_eq!(ticks(&nothing), 0, "no measurement must draw no datum tick");
        assert_eq!(ticks(&zero), 1, "a measured zero must draw a datum tick");
        assert_eq!(ticks(&some), 1);
        assert_ne!(nothing.len(), zero.len(), "zero still looks like absence");
    }

    #[test]
    fn the_datum_tick_sits_outside_the_track() {
        // Drawn across the track it lands where the arc starts and ends, so a
        // nearly-complete ring reads as complete: the tick fills in the very
        // gap it exists to reveal.
        let shapes = drawn(|p| {
            draw_track(
                p,
                pos2(0.0, 0.0),
                1.0,
                Some(0.94),
                Color32::GRAY,
                Color32::WHITE,
            )
        });
        let segment = shapes
            .iter()
            .find_map(|shape| match shape {
                eframe::egui::Shape::LineSegment { points, .. } => Some(*points),
                _ => None,
            })
            .expect("no datum tick");
        for point in segment {
            let distance = point.x.hypot(point.y);
            assert!(distance > TRACK, "the tick crosses the track at {distance}");
        }
    }

    #[test]
    fn a_dashed_circle_keeps_its_dash_phase_across_the_polyline() {
        // `arc_points` emits sixty-four short segments. Dashing them one at a
        // time restarts the pattern on every one, which paints a full dash
        // each time and produces a solid ring — silently turning every absence
        // mark back into an ordinary present one. One `dashed_line` over the
        // whole polyline carries the phase.
        let dashed = drawn(|p| {
            draw_node(
                p,
                pos2(0.0, 0.0),
                1.0,
                Glyph::Circle,
                Stroke2::Dashed,
                Color32::WHITE,
                1,
            )
        });
        let open = drawn(|p| {
            draw_node(
                p,
                pos2(0.0, 0.0),
                1.0,
                Glyph::Circle,
                Stroke2::Open,
                Color32::WHITE,
                1,
            )
        });
        let segments = dashed
            .iter()
            .filter(|shape| matches!(shape, eframe::egui::Shape::LineSegment { .. }))
            .count();
        assert!(segments > 4, "the ring came out solid: {segments} dashes");
        assert!(
            segments < 60,
            "one dash per sample is a solid ring wearing a dash pattern: {segments}"
        );
        assert!(
            open.len() < dashed.len(),
            "open and dashed draw the same thing"
        );
    }

    #[test]
    fn an_aggregate_states_its_count_by_area_with_a_fixed_inner_mark() {
        // Four times the members is twice the radius, which is the mapping a
        // reader estimates correctly. The inner dot is the point where they
        // unfold, so it does not grow with them.
        let inner = |count: usize| {
            drawn(move |p| {
                draw_node(
                    p,
                    pos2(0.0, 0.0),
                    1.0,
                    Glyph::Aggregate,
                    Stroke2::Filled,
                    Color32::WHITE,
                    count,
                )
            })
            .into_iter()
            .filter_map(|shape| match shape {
                eframe::egui::Shape::Circle(circle) => Some(circle.radius),
                _ => None,
            })
            .collect::<Vec<_>>()
        };
        let small = inner(4);
        let large = inner(16);
        let ring = |radii: &[f32]| radii.iter().cloned().fold(0.0f32, f32::max);
        let dot = |radii: &[f32]| radii.iter().cloned().fold(f32::INFINITY, f32::min);
        assert!(
            (ring(&large) / ring(&small) - 2.0).abs() < 0.01,
            "four times the count must be twice the radius"
        );
        assert_eq!(
            dot(&small),
            dot(&large),
            "the inner mark grew with the ring"
        );
    }

    #[test]
    fn an_open_below_circle_leaves_the_bottom_clear() {
        // The gap is what says "nothing below here produced me", so it has to
        // actually be at the bottom in egui's y-down screen space.
        let gap = std::f32::consts::FRAC_PI_3;
        let start = std::f32::consts::FRAC_PI_2 + gap * 0.5;
        let points = arc_points(pos2(0.0, 0.0), 6.0, start, std::f32::consts::TAU - gap);
        let lowest = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        assert!(lowest < 6.0, "the arc reaches the bottom of the circle");
        assert!(points.iter().any(|p| p.y < -5.0), "the arc misses the top");
    }
}
