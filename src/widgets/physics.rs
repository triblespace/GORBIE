//! Read-only physics snapshots, independent of simulation ownership or timing.
//!
//! Coordinates/radii remain f64 in caller-declared units (metres by default).
//! Only the final, camera-relative projection uses egui's f32 screen coordinates.
//! Colliders are wireframes; particles are depth-sorted projected spheres, not a
//! reconstructed fluid surface or a pressure field. No physics is stepped here.

use eframe::egui::{
    self, vec2, Align2, Color32, PointerButton, Rect, Response, Sense, Stroke, TextStyle, Ui,
};

#[cfg(feature = "rapier")]
pub mod rapier;
#[cfg(feature = "salva")]
pub mod salva;

/// One world-space line segment. Width is in screen points, not world units.
#[derive(Clone, Debug)]
pub struct Line3 {
    pub a: [f64; 3],
    pub b: [f64; 3],
    pub color: Color32,
    pub width: f32,
}

impl Line3 {
    pub fn new(a: [f64; 3], b: [f64; 3], color: Color32) -> Self {
        Self {
            a,
            b,
            color,
            width: 1.25,
        }
    }

    fn valid(&self) -> bool {
        finite(self.a) && finite(self.b) && self.width.is_finite() && self.width > 0.0
    }
}

/// A particle at its measured position with its configured physical radius.
#[derive(Clone, Debug)]
pub struct Particle3 {
    pub center: [f64; 3],
    pub radius: f64,
    pub color: Color32,
}

impl Particle3 {
    pub fn new(center: [f64; 3], radius: f64, color: Color32) -> Self {
        Self {
            center,
            radius,
            color,
        }
    }

    fn valid(&self) -> bool {
        finite(self.center)
            && self.radius.is_finite()
            && self.radius > 0.0
            && (0..3).all(|i| {
                (self.center[i] - self.radius).is_finite()
                    && (self.center[i] + self.radius).is_finite()
            })
    }
}

#[derive(Clone, Debug)]
pub struct Label3 {
    pub position: [f64; 3],
    pub text: String,
    pub color: Color32,
}

impl Label3 {
    pub fn new(position: [f64; 3], text: impl Into<String>, color: Color32) -> Self {
        Self {
            position,
            text: text.into(),
            color,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegendEntry {
    pub label: String,
    pub color: Color32,
}

impl LegendEntry {
    pub fn new(label: impl Into<String>, color: Color32) -> Self {
        Self {
            label: label.into(),
            color,
        }
    }
}

/// A caller-specified, stable camera envelope. This does not clip simulation data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds3 {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds3 {
    pub fn is_valid(self) -> bool {
        finite(self.min)
            && finite(self.max)
            && (0..3).all(|i| self.min[i] <= self.max[i] && (self.max[i] - self.min[i]).is_finite())
    }

    pub fn center(self) -> [f64; 3] {
        std::array::from_fn(|i| 0.5 * self.min[i] + 0.5 * self.max[i])
    }

    fn include(&mut self, point: [f64; 3]) {
        for (i, value) in point.into_iter().enumerate() {
            self.min[i] = self.min[i].min(value);
            self.max[i] = self.max[i].max(value);
        }
    }
}

/// Owned display geometry captured by the caller at a chosen simulation time.
/// Cloning a scene never clones or advances a physics world.
#[derive(Clone, Debug)]
pub struct PhysicsScene {
    pub lines: Vec<Line3>,
    pub particles: Vec<Particle3>,
    pub labels: Vec<Label3>,
    pub legend: Vec<LegendEntry>,
    pub warnings: Vec<String>,
    pub units: String,
}

impl Default for PhysicsScene {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            particles: Vec::new(),
            labels: Vec::new(),
            legend: Vec::new(),
            warnings: Vec::new(),
            units: "m".into(),
        }
    }
}

impl PhysicsScene {
    /// Append another snapshot without converting coordinates or units.
    pub fn extend(&mut self, other: Self) {
        if self.units != other.units {
            self.warnings.push(format!(
                "Mixed coordinate units: {} and {}; no unit conversion was performed.",
                self.units, other.units
            ));
        }
        self.lines.extend(other.lines);
        self.particles.extend(other.particles);
        self.labels.extend(other.labels);
        for entry in other.legend {
            if !self.legend.contains(&entry) {
                self.legend.push(entry);
            }
        }
        self.warnings.extend(other.warnings);
    }

    /// Finite geometry bounds, including particle radii. Invalid primitives are
    /// skipped here and counted explicitly when displayed, never repaired.
    pub fn bounds(&self) -> Option<Bounds3> {
        let mut bounds: Option<Bounds3> = None;
        let mut include = |point| {
            if let Some(bounds) = &mut bounds {
                bounds.include(point);
            } else {
                bounds = Some(Bounds3 {
                    min: point,
                    max: point,
                });
            }
        };
        for line in self.lines.iter().filter(|line| line.valid()) {
            include(line.a);
            include(line.b);
        }
        for particle in self.particles.iter().filter(|particle| particle.valid()) {
            include(particle.center.map(|v| v - particle.radius));
            include(particle.center.map(|v| v + particle.radius));
        }
        for label in self.labels.iter().filter(|label| finite(label.position)) {
            include(label.position);
        }
        bounds.filter(|bounds| bounds.is_valid())
    }

    /// Append a wireframe world-axis-aligned box, useful for measured envelopes.
    pub fn wire_box(&mut self, bounds: Bounds3, color: Color32) {
        if !bounds.is_valid() {
            self.warnings
                .push("Invalid wire-box bounds; box omitted.".into());
            return;
        }
        let corners: [[f64; 3]; 8] = std::array::from_fn(|n| {
            std::array::from_fn(|i| {
                if n & (1 << i) == 0 {
                    bounds.min[i]
                } else {
                    bounds.max[i]
                }
            })
        });
        for n in 0..8 {
            for axis in 0..3 {
                if n & (1 << axis) == 0 {
                    self.lines
                        .push(Line3::new(corners[n], corners[n | (1 << axis)], color));
                }
            }
        }
    }

    fn invalid_count(&self) -> usize {
        self.lines.iter().filter(|line| !line.valid()).count()
            + self
                .particles
                .iter()
                .filter(|particle| !particle.valid())
                .count()
            + self
                .labels
                .iter()
                .filter(|label| !finite(label.position))
                .count()
    }
}

/// Camera state for an orthographic, x-ray wireframe scene viewer.
///
/// Put this in notebook state and call [`Self::show`] with each chosen snapshot.
/// Initial fitting happens once, when finite geometry first arrives. Replacing a
/// scene does not move the camera; use Fit, Reset, or a fixed [`Self::bounds`].
#[derive(Clone, Debug)]
pub struct PhysicsView {
    height: f32,
    fixed_bounds: Option<Bounds3>,
    center: [f64; 3],
    half_span: f64,
    yaw: f64,
    pitch: f64,
    pan: [f64; 2],
    zoom: f64,
    fitted: bool,
    axes: bool,
}

impl Default for PhysicsView {
    fn default() -> Self {
        Self {
            height: 360.0,
            fixed_bounds: None,
            center: [0.0; 3],
            half_span: 1.0,
            yaw: -0.6,
            pitch: 0.35,
            pan: [0.0; 2],
            zoom: 1.0,
            fitted: false,
            axes: true,
        }
    }
}

impl PhysicsView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn height(mut self, height: f32) -> Self {
        if height.is_finite() {
            self.height = height.max(128.0);
        }
        self
    }

    /// Use the same camera envelope when comparing different frames/runs.
    /// Invalid bounds are reported in the UI and finite scene bounds used instead.
    pub fn bounds(mut self, bounds: Bounds3) -> Self {
        self.fixed_bounds = Some(bounds);
        self.fitted = false;
        self
    }

    pub fn axes(mut self, show: bool) -> Self {
        self.axes = show;
        self
    }

    /// Fit the fixed envelope, or current scene if none is supplied. Keeps orbit.
    pub fn fit(&mut self, scene: &PhysicsScene) {
        if let Some(bounds) = self
            .fixed_bounds
            .filter(|b| b.is_valid())
            .or_else(|| scene.bounds())
        {
            let half: [f64; 3] = std::array::from_fn(|i| 0.5 * (bounds.max[i] - bounds.min[i]));
            let radius = half[0].hypot(half[1]).hypot(half[2]);
            if radius.is_finite() {
                self.center = bounds.center();
                // A single zero-size point has no intrinsic fit scale.
                self.half_span = if radius > 0.0 { radius } else { 1.0 };
                self.pan = [0.0; 2];
                self.zoom = 1.0;
                self.fitted = true;
            }
        }
    }

    pub fn reset(&mut self, scene: &PhysicsScene) {
        self.yaw = -0.6;
        self.pitch = 0.35;
        self.fit(scene);
    }

    /// Draw a snapshot. Only this camera changes; the scene/world is borrowed.
    pub fn show(&mut self, ui: &mut Ui, scene: &PhysicsScene) -> Response {
        if !self.fitted {
            self.fit(scene);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Fit").clicked() {
                self.fit(scene);
            }
            if ui.button("Reset").clicked() {
                self.reset(scene);
            }
            ui.label(format!(
                "Orthographic · {} edges · {} particles",
                scene.lines.len(),
                scene.particles.len()
            ));
        });
        let (rect, response) = ui.allocate_exact_size(
            vec2(ui.available_width().max(1.0), self.height),
            Sense::click_and_drag(),
        );
        let scale_before = self.scale(rect);
        if response.dragged_by(PointerButton::Primary) {
            let delta = response.drag_delta();
            if ui.input(|input| input.modifiers.shift) {
                self.pan[0] += f64::from(delta.x) / scale_before;
                self.pan[1] -= f64::from(delta.y) / scale_before;
            } else {
                self.yaw += f64::from(delta.x) * 0.008;
                self.pitch = (self.pitch + f64::from(delta.y) * 0.008).clamp(-1.5, 1.5);
            }
        } else if response.dragged_by(PointerButton::Secondary)
            || response.dragged_by(PointerButton::Middle)
        {
            let delta = response.drag_delta();
            self.pan[0] += f64::from(delta.x) / scale_before;
            self.pan[1] -= f64::from(delta.y) / scale_before;
        }
        if response.hovered() {
            let scroll = ui.input_mut(|input| {
                let y = input.smooth_scroll_delta.y;
                input.smooth_scroll_delta.y = 0.0;
                y
            });
            self.zoom = (self.zoom * (f64::from(scroll) * 0.003).exp()).clamp(0.001, 1.0e6);
        }
        if response.double_clicked() {
            self.fit(scene);
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        let scale = self.scale(rect);
        let mut particles = scene
            .particles
            .iter()
            .filter(|p| p.valid())
            .filter_map(|p| {
                let (position, depth) = self.project(p.center, rect, scale)?;
                let radius = p.radius * scale;
                if !radius.is_finite() || radius <= 0.0 || radius > f64::from(f32::MAX) {
                    return None;
                }
                let radius = radius as f32;
                rect.intersects(Rect::from_center_size(
                    position,
                    vec2(radius * 2.0, radius * 2.0),
                ))
                .then_some((depth, position, radius, p.color))
            })
            .collect::<Vec<_>>();
        particles.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, position, radius, color) in particles {
            painter.circle_filled(position, radius, color);
        }
        // Intentional x-ray overlay: collider walls never hide fluid particles.
        for line in scene.lines.iter().filter(|line| line.valid()) {
            if let (Some((a, _)), Some((b, _))) = (
                self.project(line.a, rect, scale),
                self.project(line.b, rect, scale),
            ) {
                painter.line_segment([a, b], Stroke::new(line.width, line.color));
            }
        }
        for label in &scene.labels {
            if let Some((position, _)) = self.project(label.position, rect, scale) {
                painter.text(
                    position,
                    Align2::LEFT_BOTTOM,
                    &label.text,
                    TextStyle::Small.resolve(ui.style()),
                    label.color,
                );
            }
        }
        if self.axes {
            self.paint_axes(ui, &painter, rect);
        }
        let bar_value = 90.0 / scale;
        if bar_value.is_finite() && bar_value > 0.0 {
            let base = 10.0_f64.powf(bar_value.log10().floor());
            let multiple = [5.0, 2.0, 1.0]
                .into_iter()
                .find(|n| n * base <= bar_value)
                .unwrap_or(1.0);
            let value = base * multiple;
            let width = (value * scale) as f32;
            let end = rect.right_bottom() - vec2(16.0, 28.0);
            let start = end - vec2(width, 0.0);
            let color = ui.visuals().text_color();
            painter.line_segment([start, end], Stroke::new(2.0_f32, color));
            for point in [start, end] {
                painter.line_segment(
                    [point - vec2(0.0, 3.0), point + vec2(0.0, 3.0)],
                    Stroke::new(1.0_f32, color),
                );
            }
            painter.text(
                end + vec2(0.0, 6.0),
                Align2::RIGHT_TOP,
                format!("{value:.3e} {}", scene.units),
                TextStyle::Small.resolve(ui.style()),
                color,
            );
        }
        if scene.bounds().is_none() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "No finite scene geometry",
                TextStyle::Body.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            );
        }
        ui.small("Drag: orbit · Shift/right drag: pan · wheel: zoom · double click: fit · wireframes are x-ray overlays");
        ui.horizontal_wrapped(|ui| {
            for entry in &scene.legend {
                ui.label(
                    egui::RichText::new(format!("● {}", entry.label))
                        .small()
                        .color(entry.color),
                );
            }
        });
        let invalid = scene.invalid_count();
        if invalid != 0 {
            ui.colored_label(ui.visuals().warn_fg_color, format!("{invalid} invalid primitives omitted (non-finite coordinates, radius or width)."));
        }
        if self.fixed_bounds.is_some_and(|b| !b.is_valid()) {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Invalid fixed camera bounds; using finite scene bounds.",
            );
        }
        if !scene.warnings.is_empty() {
            ui.collapsing(
                format!("Snapshot diagnostics ({})", scene.warnings.len()),
                |ui| {
                    for warning in &scene.warnings {
                        ui.label(warning);
                    }
                },
            );
        }
        response
    }

    fn scale(&self, rect: Rect) -> f64 {
        (f64::from(rect.width().min(rect.height())) / self.half_span) * (0.42 * self.zoom)
    }

    fn basis(&self) -> [[f64; 3]; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        [
            [cy, 0.0, -sy],
            [-sy * sp, cp, -cy * sp],
            [sy * cp, sp, cy * cp],
        ]
    }

    fn project(&self, point: [f64; 3], rect: Rect, scale: f64) -> Option<(egui::Pos2, f64)> {
        if !finite(point) {
            return None;
        }
        let relative = std::array::from_fn(|i| point[i] - self.center[i]);
        let basis = self.basis();
        let x = (dot(relative, basis[0]) + self.pan[0]) * scale;
        let y = (dot(relative, basis[1]) + self.pan[1]) * scale;
        let depth = dot(relative, basis[2]);
        if !x.is_finite()
            || !y.is_finite()
            || !depth.is_finite()
            || x.abs() > f64::from(f32::MAX) * 0.25
            || y.abs() > f64::from(f32::MAX) * 0.25
        {
            return None;
        }
        Some((rect.center() + vec2(x as f32, -y as f32), depth))
    }

    fn paint_axes(&self, ui: &Ui, painter: &egui::Painter, rect: Rect) {
        let origin = rect.left_bottom() + vec2(42.0, -42.0);
        let basis = self.basis();
        for (i, (name, color)) in [
            ("X", Color32::from_rgb(245, 110, 105)),
            ("Y", Color32::from_rgb(110, 210, 135)),
            ("Z", Color32::from_rgb(110, 165, 245)),
        ]
        .into_iter()
        .enumerate()
        {
            let end = origin + vec2((basis[0][i] * 25.0) as f32, (-basis[1][i] * 25.0) as f32);
            painter.line_segment([origin, end], Stroke::new(1.5_f32, color));
            painter.text(
                end,
                Align2::CENTER_BOTTOM,
                name,
                TextStyle::Small.resolve(ui.style()),
                color,
            );
        }
    }
}

fn finite(point: [f64; 3]) -> bool {
    point.into_iter().all(f64::is_finite)
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_include_real_particle_radii_and_report_invalid_geometry() {
        let mut scene = PhysicsScene::default();
        scene
            .particles
            .push(Particle3::new([1.0, 2.0, 3.0], 0.25, Color32::WHITE));
        scene
            .particles
            .push(Particle3::new([f64::NAN, 0.0, 0.0], 1.0, Color32::WHITE));
        assert_eq!(
            scene.bounds(),
            Some(Bounds3 {
                min: [0.75, 1.75, 2.75],
                max: [1.25, 2.25, 3.25]
            })
        );
        assert_eq!(scene.invalid_count(), 1);
    }

    #[test]
    fn snapshot_replacement_does_not_automatically_refit_camera() {
        let mut scene = PhysicsScene::default();
        scene.wire_box(
            Bounds3 {
                min: [-1.0; 3],
                max: [1.0; 3],
            },
            Color32::WHITE,
        );
        let mut view = PhysicsView::default();
        view.fit(&scene);
        let center = view.center;
        scene
            .particles
            .push(Particle3::new([100.0; 3], 1.0, Color32::WHITE));
        assert_eq!(view.center, center);
        view.fit(&scene);
        assert_ne!(view.center, center);
        assert_eq!(scene.lines.len(), 12);
    }

    #[test]
    fn camera_relative_projection_preserves_small_local_geometry() {
        let mut view = PhysicsView::default().bounds(Bounds3 {
            min: [1.0e9, 0.0, 0.0],
            max: [1.0e9 + 0.001, 0.001, 0.001],
        });
        view.fit(&PhysicsScene::default());
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(400.0, 400.0));
        let a = view
            .project([1.0e9, 0.0, 0.0], rect, view.scale(rect))
            .unwrap()
            .0;
        let b = view
            .project([1.0e9 + 0.001, 0.0, 0.0], rect, view.scale(rect))
            .unwrap()
            .0;
        assert!(a.distance(b) > 100.0);
    }

    #[test]
    fn merging_different_units_is_not_silent() {
        let mut scene = PhysicsScene::default();
        scene.extend(PhysicsScene {
            units: "mm".into(),
            ..PhysicsScene::default()
        });
        assert_eq!(scene.warnings.len(), 1);
        assert_eq!(scene.units, "m");
    }

    #[test]
    fn egui_snapshot_replacement_keeps_an_existing_camera_fit() {
        let context = egui::Context::default();
        let mut scene = PhysicsScene::default();
        scene
            .particles
            .push(Particle3::new([0.0; 3], 0.1, Color32::WHITE));
        let mut view = PhysicsView::default();
        let _ = context.run_ui(egui::RawInput::default(), |root_ui| {
            egui::CentralPanel::default().show_inside(root_ui, |ui| {
                view.show(ui, &scene);
            });
        });
        let center = view.center;
        let half_span = view.half_span;
        scene
            .particles
            .push(Particle3::new([10.0; 3], 0.1, Color32::WHITE));
        let _ = context.run_ui(egui::RawInput::default(), |root_ui| {
            egui::CentralPanel::default().show_inside(root_ui, |ui| {
                view.show(ui, &scene);
            });
        });
        assert_eq!(view.center, center);
        assert_eq!(view.half_span, half_span);
    }
}
