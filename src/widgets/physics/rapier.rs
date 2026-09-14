//! Snapshot adapters for `rapier3d-f64` 0.35.1. No stepping or world mutation.
//!
//! Cuboids, balls, capsules and compounds are tessellated as wireframes. Other
//! finite shapes use an explicitly warned world-axis-aligned bounding box.
//! These display approximations are never substituted into Rapier's geometry.

use eframe::egui::Color32;
use rapier3d_f64::{
    math::{Pose, Vector},
    parry::shape::{Shape, TypedShape},
    prelude::{ColliderSet, RigidBodySet},
};

use super::{Bounds3, Label3, LegendEntry, Line3, PhysicsScene};

const FIXED: Color32 = Color32::from_rgb(155, 175, 190);
const DYNAMIC: Color32 = Color32::from_rgb(255, 195, 95);
const KINEMATIC: Color32 = Color32::from_rgb(140, 210, 150);
const SENSOR: Color32 = Color32::from_rgb(205, 155, 235);
const FALLBACK: Color32 = Color32::from_rgb(245, 120, 95);
const SEGMENTS: usize = 32;

/// Capture current body/collider poses as owned geometry, in Rapier length units
/// (labelled metres by default). Parent body pose times collider-local pose is
/// used even if Rapier has not yet propagated a manually edited body pose.
/// Standalone colliders use their current world pose. Disabled colliders remain
/// visible in grey and are labelled; display is not collision participation.
pub fn scene(bodies: &RigidBodySet, colliders: &ColliderSet) -> PhysicsScene {
    let mut scene = PhysicsScene::default();
    for (handle, collider) in colliders.iter() {
        let body = collider.parent().and_then(|handle| bodies.get(handle));
        let (color, label) = if !collider.is_enabled() {
            (Color32::from_gray(90), "Disabled collider")
        } else if collider.is_sensor() {
            (SENSOR, "Sensor wireframe")
        } else if body.is_some_and(|body| body.is_dynamic()) {
            (DYNAMIC, "Dynamic collider")
        } else if body.is_some_and(|body| body.is_kinematic()) {
            (KINEMATIC, "Kinematic collider")
        } else {
            (FIXED, "Fixed / standalone collider")
        };
        let pose = match (body, collider.position_wrt_parent()) {
            (Some(body), Some(local)) => body.position() * local,
            _ => {
                if collider.parent().is_some() && body.is_none() {
                    scene.warnings.push(format!(
                        "Collider {handle:?}: missing parent body; using cached world pose."
                    ));
                }
                *collider.position()
            }
        };
        let entry = LegendEntry::new(label, color);
        if !scene.legend.contains(&entry) {
            scene.legend.push(entry);
        }
        shape(
            &mut scene,
            collider.shape(),
            pose,
            color,
            &format!("Collider {handle:?}"),
        );
    }
    scene
}

fn shape(scene: &mut PhysicsScene, shape: &dyn Shape, pose: Pose, color: Color32, name: &str) {
    match shape.as_typed_shape() {
        TypedShape::Cuboid(cuboid) => {
            box_edges(
                scene,
                -cuboid.half_extents,
                cuboid.half_extents,
                pose,
                color,
            );
        }
        TypedShape::Ball(ball) => sphere(scene, Vector::ZERO, ball.radius, pose, color),
        TypedShape::Capsule(capsule) => {
            capsule_edges(
                scene,
                capsule.segment.a,
                capsule.segment.b,
                capsule.radius,
                pose,
                color,
            );
        }
        TypedShape::Compound(compound) => {
            for (index, (local, child)) in compound.shapes().iter().enumerate() {
                self::shape(
                    scene,
                    &**child,
                    pose * local,
                    color,
                    &format!("{name}/{index}"),
                );
            }
        }
        _ => {
            let aabb = shape.compute_aabb(&pose);
            let bounds = Bounds3 {
                min: aabb.mins.to_array(),
                max: aabb.maxs.to_array(),
            };
            if bounds.is_valid() {
                scene.wire_box(bounds, FALLBACK);
                scene
                    .labels
                    .push(Label3::new(bounds.center(), "AABB approximation", FALLBACK));
                scene.warnings.push(format!(
                    "{name}: {:?} displayed only as its world AABB, not its collision surface.",
                    shape.shape_type()
                ));
                let entry = LegendEntry::new("Unsupported shape: AABB only", FALLBACK);
                if !scene.legend.contains(&entry) {
                    scene.legend.push(entry);
                }
            } else {
                scene.warnings.push(format!(
                    "{name}: {:?} has no representable finite AABB; omitted from view.",
                    shape.shape_type()
                ));
            }
        }
    }
}

fn edge(scene: &mut PhysicsScene, a: Vector, b: Vector, pose: Pose, color: Color32) {
    scene.lines.push(Line3::new(
        pose.transform_point(a).to_array(),
        pose.transform_point(b).to_array(),
        color,
    ));
}

fn box_edges(scene: &mut PhysicsScene, min: Vector, max: Vector, pose: Pose, color: Color32) {
    let vertices: [Vector; 8] = std::array::from_fn(|n| {
        Vector::new(
            if n & 1 == 0 { min.x } else { max.x },
            if n & 2 == 0 { min.y } else { max.y },
            if n & 4 == 0 { min.z } else { max.z },
        )
    });
    for n in 0..8 {
        for axis in 0..3 {
            if n & (1 << axis) == 0 {
                edge(scene, vertices[n], vertices[n | (1 << axis)], pose, color);
            }
        }
    }
}

fn circle(
    scene: &mut PhysicsScene,
    center: Vector,
    u: Vector,
    v: Vector,
    radius: f64,
    pose: Pose,
    color: Color32,
) {
    for i in 0..SEGMENTS {
        let a = std::f64::consts::TAU * i as f64 / SEGMENTS as f64;
        let b = std::f64::consts::TAU * (i + 1) as f64 / SEGMENTS as f64;
        edge(
            scene,
            center + radius * (u * a.cos() + v * a.sin()),
            center + radius * (u * b.cos() + v * b.sin()),
            pose,
            color,
        );
    }
}

fn sphere(scene: &mut PhysicsScene, center: Vector, radius: f64, pose: Pose, color: Color32) {
    for (u, v) in [
        (Vector::X, Vector::Y),
        (Vector::Y, Vector::Z),
        (Vector::Z, Vector::X),
    ] {
        circle(scene, center, u, v, radius, pose, color);
    }
}

fn capsule_edges(
    scene: &mut PhysicsScene,
    a: Vector,
    b: Vector,
    radius: f64,
    pose: Pose,
    color: Color32,
) {
    let axis = b - a;
    if axis.length_squared() == 0.0 {
        sphere(scene, a, radius, pose, color);
        return;
    }
    let axis = axis.normalize();
    let helper = if axis.x.abs() < 0.8 {
        Vector::X
    } else {
        Vector::Y
    };
    let u = axis.cross(helper).normalize();
    let v = axis.cross(u);
    circle(scene, a, u, v, radius, pose, color);
    circle(scene, b, u, v, radius, pose, color);
    for meridian in 0..8 {
        let theta = std::f64::consts::TAU * meridian as f64 / 8.0;
        let radial = u * theta.cos() + v * theta.sin();
        edge(scene, a + radius * radial, b + radius * radial, pose, color);
        for i in 0..8 {
            let phi0 = std::f64::consts::FRAC_PI_2 * i as f64 / 8.0;
            let phi1 = std::f64::consts::FRAC_PI_2 * (i + 1) as f64 / 8.0;
            for (center, sign) in [(a, -1.0), (b, 1.0)] {
                edge(
                    scene,
                    center + radius * (radial * phi0.cos() + axis * (sign * phi0.sin())),
                    center + radius * (radial * phi1.cos() + axis * (sign * phi1.sin())),
                    pose,
                    color,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d_f64::prelude::{ColliderBuilder, RigidBodyBuilder, SharedShape};

    #[test]
    fn parent_rotation_and_local_offset_are_composed_without_stepping() {
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let body = bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(10.0, 0.0, 0.0))
                .rotation(Vector::new(0.0, 0.0, std::f64::consts::FRAC_PI_2))
                .build(),
        );
        colliders.insert_with_parent(
            ColliderBuilder::cuboid(0.5, 0.25, 0.125)
                .translation(Vector::new(2.0, 0.0, 0.0))
                .build(),
            body,
            &mut bodies,
        );
        let snapshot = scene(&bodies, &colliders);
        assert_eq!(snapshot.lines.len(), 12);
        let bounds = snapshot.bounds().unwrap();
        assert!((bounds.center()[0] - 10.0).abs() < 1e-12);
        assert!((bounds.center()[1] - 2.0).abs() < 1e-12);
        assert!((bounds.max[0] - bounds.min[0] - 0.5).abs() < 1e-12);
        assert!((bounds.max[1] - bounds.min[1] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn compound_children_use_world_transform_and_supported_shapes() {
        let mut colliders = ColliderSet::new();
        colliders.insert(
            ColliderBuilder::compound(vec![
                (Pose::translation(1.0, 0.0, 0.0), SharedShape::ball(0.25)),
                (
                    Pose::translation(-1.0, 0.0, 0.0),
                    SharedShape::capsule_y(0.5, 0.25),
                ),
            ])
            .translation(Vector::new(3.0, 0.0, 0.0))
            .build(),
        );
        let snapshot = scene(&RigidBodySet::new(), &colliders);
        assert!(snapshot.warnings.is_empty());
        let bounds = snapshot.bounds().unwrap();
        assert!((bounds.min[0] - 1.75).abs() < 1e-12);
        assert!((bounds.max[0] - 4.25).abs() < 1e-12);
    }

    #[test]
    fn unsupported_shape_is_an_explicit_world_aabb() {
        let mut colliders = ColliderSet::new();
        colliders.insert(ColliderBuilder::cylinder(1.0, 0.5).build());
        let snapshot = scene(&RigidBodySet::new(), &colliders);
        assert_eq!(snapshot.lines.len(), 12);
        assert_eq!(snapshot.warnings.len(), 1);
        assert!(snapshot.warnings[0].contains("AABB"));
    }
}
