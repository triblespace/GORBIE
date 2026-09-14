//! Read-only, owned snapshots of actual `salva3d-f64` 0.10.0 fluid particles.
//! Radius is the configured particle radius, not kernel support or a radius
//! inferred from mass/rest-volume bookkeeping. No free surface is reconstructed.

use eframe::egui::Color32;
use salva3d_f64::{object::Fluid, LiquidWorld};

use super::{LegendEntry, Particle3, PhysicsScene};

const COLORS: [Color32; 4] = [
    Color32::from_rgb(85, 180, 235),
    Color32::from_rgb(90, 220, 195),
    Color32::from_rgb(180, 150, 245),
    Color32::from_rgb(245, 165, 105),
];

/// Copy a world's fluid positions and radii. Boundary sampling points are not
/// fluid particles and are not included; combine with a Rapier collider scene
/// when the caller wants actual rigid boundary geometry too.
pub fn scene(world: &LiquidWorld) -> PhysicsScene {
    let mut scene = PhysicsScene::default();
    for (index, fluid) in world.fluids().as_slice().iter().enumerate() {
        append(
            &mut scene,
            fluid,
            &format!("Fluid {index}"),
            COLORS[index % COLORS.len()],
        );
        if fluid.particle_radius() != world.particle_radius() {
            scene.warnings.push(format!("Fluid {index} radius {} differs from world radius {}; displayed radius comes from the fluid.", fluid.particle_radius(), world.particle_radius()));
        }
    }
    scene
}

/// Copy one fluid, using its actual configured radius and unmodified positions.
pub fn fluid_scene(fluid: &Fluid) -> PhysicsScene {
    let mut scene = PhysicsScene::default();
    append(&mut scene, fluid, "Fluid", COLORS[0]);
    scene
}

fn append(scene: &mut PhysicsScene, fluid: &Fluid, name: &str, color: Color32) {
    let radius = fluid.particle_radius();
    scene.particles.extend(
        fluid
            .positions
            .iter()
            .map(|p| Particle3::new([p[0], p[1], p[2]], radius, color)),
    );
    scene.legend.push(LegendEntry::new(
        format!(
            "{name}: {} particles, r={radius:.3e} (scene units)",
            fluid.positions.len()
        ),
        color,
    ));
    if fluid.num_deleted_particles() != 0 {
        scene.warnings.push(format!("{name}: {} particles marked for deletion next timestep remain in this current snapshot.", fluid.num_deleted_particles()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use salva3d_f64::math::Vector;

    #[test]
    fn copies_true_positions_and_configured_radius_without_mutating_fluid() {
        let mut fluid = Fluid::new(
            vec![Vector::new(1.0, -2.0, 3.0), Vector::new(4.0, 5.0, 6.0)],
            0.125,
            1000.0,
            Default::default(),
        );
        fluid.delete_particle_at_next_timestep(1);
        let snapshot = fluid_scene(&fluid);
        assert_eq!(snapshot.particles.len(), 2);
        assert_eq!(snapshot.particles[0].center, [1.0, -2.0, 3.0]);
        assert_eq!(snapshot.particles[1].radius, 0.125);
        assert_eq!(fluid.num_particles(), 2);
        assert_eq!(fluid.num_deleted_particles(), 1);
        assert_eq!(snapshot.warnings.len(), 1);
        fluid.positions[0][0] = 7.0;
        assert_eq!(snapshot.particles[0].center[0], 1.0);
    }
}
