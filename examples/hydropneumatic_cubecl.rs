//! A deliberately small GPU-first hydropneumatic experiment.
//!
//! This is a lumped model, not CFD and not a claim that Salva advances this
//! system.  CubeCL owns the evolving state on the device; GORBIE renders a
//! sampled observation through the same read-only physics view used by the
//! Salva adapter.  Run it with:
//!
//! ```text
//! cargo run --release --example hydropneumatic_cubecl --features cubecl,salva
//! ```

use std::time::Duration;

use cubecl::client::ComputeClient;
use cubecl::prelude::*;
use cubecl::server::Handle;
use cubecl::wgpu::{WgpuDevice, WgpuRuntime};
use eframe::egui::Color32;
use salva3d_f64::{math::Vector, object::Fluid};

use GORBIE::widgets::{
    Bounds3, Label3, LegendEntry, Line3, Particle3, PhysicsScene, PhysicsView,
};
use GORBIE::{notebook, NotebookCtx};

const STATE_STRIDE: usize = 5;
const PRESSURE_H: usize = 0;
const PRESSURE_P: usize = 1;
const POSITION: usize = 2;
const VELOCITY: usize = 3;
const PHASE: usize = 4;

// SI-ish parameters for a stable, intentionally modest toy system.
const AMBIENT_PRESSURE: f32 = 100_000.0;
const SOURCE_AMPLITUDE: f32 = 300_000.0;
const SOURCE_ANGULAR_FREQUENCY: f32 = 2.0 * std::f32::consts::PI * 1.5;
const HYDRAULIC_TIME_CONSTANT: f32 = 0.030;
const PNEUMATIC_TIME_CONSTANT: f32 = 0.080;
const PISTON_AREA: f32 = 4.0e-4;
const PISTON_MASS: f32 = 0.40;
const SPRING_STIFFNESS: f32 = 900.0;
const DAMPING: f32 = 8.0;
const DT: f32 = 1.0e-4;
const SCENARIOS: usize = 256;
const STEPS_PER_FRAME: u32 = 1_000;

#[derive(Clone, Copy, Debug)]
struct Sample {
    pressure_hydraulic: f32,
    pressure_pneumatic: f32,
    position: f32,
    velocity: f32,
    time: f32,
}

struct HydroGpu {
    _device: WgpuDevice,
    client: ComputeClient<WgpuRuntime>,
    state: Handle,
    source_scales: Handle,
    sample: Handle,
    scenario_count: usize,
    time: f32,
}

impl HydroGpu {
    fn new() -> Self {
        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);

        let mut initial = vec![0.0f32; SCENARIOS * STATE_STRIDE];
        for scenario in 0..SCENARIOS {
            let base = scenario * STATE_STRIDE;
            initial[base + PRESSURE_H] = AMBIENT_PRESSURE;
            initial[base + PRESSURE_P] = AMBIENT_PRESSURE;
        }
        let source_scales: Vec<f32> = (0..SCENARIOS)
            .map(|scenario| 0.65 + 0.70 * scenario as f32 / (SCENARIOS - 1) as f32)
            .collect();

        Self {
            _device: device,
            state: client.create_from_slice(f32::as_bytes(&initial)),
            source_scales: client.create_from_slice(f32::as_bytes(&source_scales)),
            sample: client.empty(STATE_STRIDE * std::mem::size_of::<f32>()),
            client,
            scenario_count: SCENARIOS,
            time: 0.0,
        }
    }

    fn advance(&mut self, steps: u32) -> Result<Sample, String> {
        if steps == 0 {
            return Err("GPU step count must be nonzero".into());
        }
        let state_len = self.scenario_count * STATE_STRIDE;
        unsafe {
            hydropneumatic_step_kernel::launch::<WgpuRuntime>(
                &self.client,
                CubeCount::new_1d(self.scenario_count as u32),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts(self.state.clone(), state_len),
                ArrayArg::from_raw_parts(self.source_scales.clone(), self.scenario_count),
                ArrayArg::from_raw_parts(self.sample.clone(), STATE_STRIDE),
                self.scenario_count as u32,
                steps,
                DT,
                AMBIENT_PRESSURE,
                SOURCE_AMPLITUDE,
                SOURCE_ANGULAR_FREQUENCY,
                HYDRAULIC_TIME_CONSTANT,
                PNEUMATIC_TIME_CONSTANT,
                PISTON_AREA,
                PISTON_MASS,
                SPRING_STIFFNESS,
                DAMPING,
            );
        }

        self.time += DT * steps as f32;
        let bytes = self
            .client
            .read_one(self.sample.clone())
            .map_err(|error| format!("GPU sample readback failed: {error:?}"))?;
        let values = f32::from_bytes(&bytes);
        if values.len() < STATE_STRIDE {
            return Err(format!(
                "GPU sample returned {} floats, expected {STATE_STRIDE}",
                values.len()
            ));
        }
        Ok(Sample {
            pressure_hydraulic: values[PRESSURE_H],
            pressure_pneumatic: values[PRESSURE_P],
            position: values[POSITION],
            velocity: values[VELOCITY],
            time: self.time,
        })
    }
}

#[cube(launch)]
fn hydropneumatic_step_kernel(
    state: &mut Array<f32>,
    source_scales: &Array<f32>,
    sample: &mut Array<f32>,
    scenario_count: u32,
    steps: u32,
    dt: f32,
    ambient_pressure: f32,
    source_amplitude: f32,
    source_angular_frequency: f32,
    hydraulic_time_constant: f32,
    pneumatic_time_constant: f32,
    piston_area: f32,
    piston_mass: f32,
    spring_stiffness: f32,
    damping: f32,
) {
    let scenario = ABSOLUTE_POS as u32;
    if scenario < scenario_count {
        let base = (scenario as usize) * STATE_STRIDE;
        let source_scale = source_scales[scenario as usize];
        let mut pressure_hydraulic = state[base + PRESSURE_H];
        let mut pressure_pneumatic = state[base + PRESSURE_P];
        let mut position = state[base + POSITION];
        let mut velocity = state[base + VELOCITY];
        let mut phase = state[base + PHASE];
        let mut step = 0u32;

        while step < steps {
            let source = ambient_pressure
                + source_amplitude
                    * source_scale
                    * (0.5f32 + 0.5f32 * (phase * source_angular_frequency).sin());
            let hydraulic_delta =
                (source - pressure_hydraulic) / hydraulic_time_constant;
            let transfer = (pressure_hydraulic - pressure_pneumatic)
                / pneumatic_time_constant;
            pressure_hydraulic += hydraulic_delta * dt;
            pressure_pneumatic += transfer * dt;

            let force = piston_area * (pressure_pneumatic - ambient_pressure);
            let acceleration =
                (force - spring_stiffness * position - damping * velocity) / piston_mass;
            velocity += acceleration * dt;
            position += velocity * dt;
            phase += dt;
            step += 1u32;
        }

        state[base + PRESSURE_H] = pressure_hydraulic;
        state[base + PRESSURE_P] = pressure_pneumatic;
        state[base + POSITION] = position;
        state[base + VELOCITY] = velocity;
        state[base + PHASE] = phase;

        if scenario == 0u32 {
            sample[PRESSURE_H] = pressure_hydraulic;
            sample[PRESSURE_P] = pressure_pneumatic;
            sample[POSITION] = position;
            sample[VELOCITY] = velocity;
            sample[PHASE] = phase;
        }
    }
}

fn static_fluid_scene() -> PhysicsScene {
    let positions: Vec<_> = (0..5)
        .flat_map(|x| {
            (0..3).flat_map(move |y| {
                (0..3).map(move |z| {
                    Vector::new(
                        -1.30 + x as f64 * 0.12,
                        -0.18 + y as f64 * 0.18,
                        -0.18 + z as f64 * 0.18,
                    )
                })
            })
        })
        .collect();
    let fluid = Fluid::new(positions, 0.045, 1000.0, Default::default());
    let mut scene = GORBIE::widgets::physics::salva::fluid_scene(&fluid);
    scene
        .warnings
        .push("Salva particles are a read-only visual sample; CubeCL owns dynamics.".into());
    scene
}

fn scene(sample: Sample, static_fluid: &PhysicsScene) -> PhysicsScene {
    let mut scene = static_fluid.clone();
    let pressure_mix = ((sample.pressure_pneumatic - AMBIENT_PRESSURE)
        / SOURCE_AMPLITUDE)
        .clamp(0.0, 1.0);
    let piston_color = Color32::from_rgb(
        (80.0 + pressure_mix * 170.0) as u8,
        (170.0 - pressure_mix * 80.0) as u8,
        230,
    );

    scene.wire_box(
        Bounds3 {
            min: [-1.45, -0.32, -0.32],
            max: [-0.55, 0.32, 0.32],
        },
        Color32::from_rgb(80, 150, 220),
    );
    scene.wire_box(
        Bounds3 {
            min: [0.55, -0.32, -0.32],
            max: [1.45, 0.32, 0.32],
        },
        Color32::from_rgb(180, 130, 220),
    );
    scene.lines.push(Line3::new(
        [-0.55, 0.0, 0.0],
        [0.55, 0.0, 0.0],
        Color32::from_rgb(245, 170, 80),
    ));

    let piston_x = (0.82 + sample.position as f64 * 3.0).clamp(0.60, 1.40);
    scene.particles.push(Particle3::new(
        [piston_x, 0.0, 0.0],
        0.12,
        piston_color,
    ));
    scene.labels.push(Label3::new(
        [-1.42, 0.42, -0.32],
        format!("hydraulic {:.1} kPa", sample.pressure_hydraulic / 1_000.0),
        Color32::from_rgb(110, 190, 240),
    ));
    scene.labels.push(Label3::new(
        [0.55, 0.42, -0.32],
        format!("pneumatic {:.1} kPa", sample.pressure_pneumatic / 1_000.0),
        Color32::from_rgb(205, 165, 245),
    ));
    scene.labels.push(Label3::new(
        [0.58, -0.48, -0.32],
        format!("x={:.4} m  v={:.3} m/s  t={:.2} s", sample.position, sample.velocity, sample.time),
        piston_color,
    ));
    scene.legend.push(LegendEntry::new(
        "CubeCL sampled state: pressure → force → piston", piston_color,
    ));
    scene.units = "m".into();
    scene
}

struct HydroNotebook {
    gpu: HydroGpu,
    camera: PhysicsView,
    static_fluid: PhysicsScene,
    last: Option<Sample>,
    error: Option<String>,
}

impl HydroNotebook {
    fn new() -> Self {
        Self {
            gpu: HydroGpu::new(),
            camera: PhysicsView::default()
                .height(390.0)
                .bounds(Bounds3 {
                    min: [-1.6, -0.6, -0.6],
                    max: [1.6, 0.6, 0.6],
                }),
            static_fluid: static_fluid_scene(),
            last: None,
            error: None,
        }
    }

    fn frame(&mut self) -> PhysicsScene {
        match self.gpu.advance(STEPS_PER_FRAME) {
            Ok(sample) => {
                self.error = None;
                self.last = Some(sample);
            }
            Err(error) => self.error = Some(error),
        }
        self.last
            .map(|sample| scene(sample, &self.static_fluid))
            .unwrap_or_else(|| self.static_fluid.clone())
    }
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    nb.state_with("hydropneumatic-cubecl", HydroNotebook::new, |ctx, state| {
        let scene = state.frame();
        ctx.heading("CubeCL hydropneumatic experiment");
        ctx.label("256 source-amplitude scenarios evolve on the GPU; only scenario 0 is sampled for this view.");
        if let Some(error) = &state.error {
            ctx.label(format!("GPU error: {error}"));
        }
        state.camera.show(ctx, &scene);
        ctx.ctx().request_repaint_after(Duration::from_millis(50));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_kernel_moves_the_piston_under_pressure() {
        let mut gpu = HydroGpu::new();
        let sample = gpu.advance(20_000).expect("CubeCL WGPU step");
        assert!(sample.pressure_pneumatic > AMBIENT_PRESSURE);
        assert!(sample.position > 0.0);
    }

    #[test]
    fn gpu_kernel_keeps_equilibrium_without_source() {
        let mut gpu = HydroGpu::new();
        // A zero-amplitude input is represented by an all-ambient initial
        // state in this test-specific runner, so the analytical equilibrium
        // should remain at x=v=0 when the kernel is exercised.
        gpu.source_scales = gpu
            .client
            .create_from_slice(f32::as_bytes(&vec![0.0; SCENARIOS]));
        let sample = gpu.advance(10_000).expect("CubeCL WGPU step");
        assert!((sample.pressure_hydraulic - AMBIENT_PRESSURE).abs() < 1.0);
        assert!((sample.pressure_pneumatic - AMBIENT_PRESSURE).abs() < 1.0);
        assert!(sample.position.abs() < 1.0e-6);
        assert!(sample.velocity.abs() < 1.0e-4);
    }
}
