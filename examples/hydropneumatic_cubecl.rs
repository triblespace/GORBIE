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

const STATE_STRIDE: usize = 7;
const OBSERVATION_STRIDE: usize = 8;
const PRESSURE_H: usize = 0;
const PRESSURE_P: usize = 1;
const POSITION: usize = 2;
const VELOCITY: usize = 3;
const PHASE: usize = 4;
const FLOW_H: usize = 5;
const FLOW_P: usize = 6;
const OBS_SOURCE_SCALE: usize = 0;
const OBS_VALVE_OPENING: usize = 1;
const OBS_PRESSURE_H: usize = 2;
const OBS_PRESSURE_P: usize = 3;
const OBS_FLOW_H: usize = 4;
const OBS_FLOW_P: usize = 5;
const OBS_POSITION: usize = 6;
const OBS_VELOCITY: usize = 7;

// SI-ish parameters for a stable, intentionally modest toy system.
const AMBIENT_PRESSURE: f32 = 100_000.0;
const SOURCE_AMPLITUDE: f32 = 300_000.0;
const SOURCE_ANGULAR_FREQUENCY: f32 = 2.0 * std::f32::consts::PI * 1.5;
// Compliance and conductance make the pressure transfer explicit: each
// pressure state is advanced from a volumetric flow instead of teleporting
// toward its target.  The ratios retain the stable response times of the
// original toy model while exposing a useful flow observable.
const HYDRAULIC_COMPLIANCE: f32 = 8.0e-9;
const PNEUMATIC_COMPLIANCE: f32 = 3.2e-8;
const HYDRAULIC_CONDUCTANCE: f32 = HYDRAULIC_COMPLIANCE / 0.030;
const PNEUMATIC_CONDUCTANCE: f32 = PNEUMATIC_COMPLIANCE / 0.080;
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
    flow_hydraulic: f32,
    flow_pneumatic: f32,
    valve_opening: f32,
    position: f32,
    velocity: f32,
    time: f32,
}

#[derive(Clone, Copy, Debug)]
struct SweepSummary {
    scenarios: usize,
    min_valve_opening: f32,
    max_valve_opening: f32,
    min_position: f32,
    max_position: f32,
    max_pressure_pneumatic: f32,
    peak_flow_pneumatic: f32,
}

struct HydroGpu {
    _device: WgpuDevice,
    client: ComputeClient<WgpuRuntime>,
    state: Handle,
    source_scales: Handle,
    valve_openings: Handle,
    sample: Handle,
    observations: Handle,
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
        let valve_openings: Vec<f32> = (0..SCENARIOS)
            .map(|scenario| 0.20 + 0.80 * scenario as f32 / (SCENARIOS - 1) as f32)
            .collect();

        Self {
            _device: device,
            state: client.create_from_slice(f32::as_bytes(&initial)),
            source_scales: client.create_from_slice(f32::as_bytes(&source_scales)),
            valve_openings: client.create_from_slice(f32::as_bytes(&valve_openings)),
            sample: client.empty(STATE_STRIDE * std::mem::size_of::<f32>()),
            observations: client.empty(
                SCENARIOS * OBSERVATION_STRIDE * std::mem::size_of::<f32>(),
            ),
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
                ArrayArg::from_raw_parts(self.valve_openings.clone(), self.scenario_count),
                ArrayArg::from_raw_parts(self.sample.clone(), STATE_STRIDE),
                ArrayArg::from_raw_parts(
                    self.observations.clone(),
                    self.scenario_count * OBSERVATION_STRIDE,
                ),
                self.scenario_count as u32,
                steps,
                DT,
                AMBIENT_PRESSURE,
                SOURCE_AMPLITUDE,
                SOURCE_ANGULAR_FREQUENCY,
                HYDRAULIC_COMPLIANCE,
                PNEUMATIC_COMPLIANCE,
                HYDRAULIC_CONDUCTANCE,
                PNEUMATIC_CONDUCTANCE,
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
            flow_hydraulic: values[FLOW_H],
            flow_pneumatic: values[FLOW_P],
            valve_opening: values[PHASE],
            position: values[POSITION],
            velocity: values[VELOCITY],
            time: self.time,
        })
    }

    fn read_sweep(&self) -> Result<SweepSummary, String> {
        self.read_observations(0).map(|(summary, _)| summary)
    }

    fn read_scenario(&self, scenario: usize) -> Result<Sample, String> {
        self.read_observations(scenario).map(|(_, sample)| sample)
    }

    fn read_observations(&self, selected_scenario: usize) -> Result<(SweepSummary, Sample), String> {
        if selected_scenario >= self.scenario_count {
            return Err(format!(
                "scenario {selected_scenario} is outside 0..{}",
                self.scenario_count
            ));
        }
        let bytes = self
            .client
            .read_one(self.observations.clone())
            .map_err(|error| format!("GPU sweep readback failed: {error:?}"))?;
        let values = f32::from_bytes(&bytes);
        let expected = self.scenario_count * OBSERVATION_STRIDE;
        if values.len() != expected {
            return Err(format!(
                "GPU sweep returned {} floats, expected {expected}",
                values.len()
            ));
        }

        let mut min_position = f32::INFINITY;
        let mut max_position = f32::NEG_INFINITY;
        let mut max_pressure_pneumatic = f32::NEG_INFINITY;
        let mut peak_flow_pneumatic: f32 = 0.0;
        let mut min_valve_opening = f32::INFINITY;
        let mut max_valve_opening = f32::NEG_INFINITY;
        for observation in values.chunks_exact(OBSERVATION_STRIDE) {
            min_valve_opening = min_valve_opening.min(observation[OBS_VALVE_OPENING]);
            max_valve_opening = max_valve_opening.max(observation[OBS_VALVE_OPENING]);
            min_position = min_position.min(observation[OBS_POSITION]);
            max_position = max_position.max(observation[OBS_POSITION]);
            max_pressure_pneumatic = max_pressure_pneumatic.max(observation[OBS_PRESSURE_P]);
            peak_flow_pneumatic = peak_flow_pneumatic.max(observation[OBS_FLOW_P].abs());
        }

        let selected = &values[selected_scenario * OBSERVATION_STRIDE
            ..(selected_scenario + 1) * OBSERVATION_STRIDE];
        Ok((
            SweepSummary {
                scenarios: self.scenario_count,
                min_valve_opening,
                max_valve_opening,
                min_position,
                max_position,
                max_pressure_pneumatic,
                peak_flow_pneumatic,
            },
            Sample {
                pressure_hydraulic: selected[OBS_PRESSURE_H],
                pressure_pneumatic: selected[OBS_PRESSURE_P],
                flow_hydraulic: selected[OBS_FLOW_H],
                flow_pneumatic: selected[OBS_FLOW_P],
                valve_opening: selected[OBS_VALVE_OPENING],
                position: selected[OBS_POSITION],
                velocity: selected[OBS_VELOCITY],
                time: self.time,
            },
        ))
    }
}

#[cube(launch)]
fn hydropneumatic_step_kernel(
    state: &mut Array<f32>,
    source_scales: &Array<f32>,
    valve_openings: &Array<f32>,
    sample: &mut Array<f32>,
    observations: &mut Array<f32>,
    scenario_count: u32,
    steps: u32,
    dt: f32,
    ambient_pressure: f32,
    source_amplitude: f32,
    source_angular_frequency: f32,
    hydraulic_compliance: f32,
    pneumatic_compliance: f32,
    hydraulic_conductance: f32,
    pneumatic_conductance: f32,
    piston_area: f32,
    piston_mass: f32,
    spring_stiffness: f32,
    damping: f32,
) {
    let scenario = ABSOLUTE_POS as u32;
    if scenario < scenario_count {
        let base = (scenario as usize) * STATE_STRIDE;
        let source_scale = source_scales[scenario as usize];
        let valve_opening = valve_openings[scenario as usize];
        let mut pressure_hydraulic = state[base + PRESSURE_H];
        let mut pressure_pneumatic = state[base + PRESSURE_P];
        let mut position = state[base + POSITION];
        let mut velocity = state[base + VELOCITY];
        let mut phase = state[base + PHASE];
        let mut hydraulic_flow = 0.0f32;
        let mut transfer_flow = 0.0f32;
        let mut step = 0u32;

        while step < steps {
            let source = ambient_pressure
                + source_amplitude
                    * source_scale
                    * (0.5f32 + 0.5f32 * (phase * source_angular_frequency).sin());
            hydraulic_flow = hydraulic_conductance
                * valve_opening
                * (source - pressure_hydraulic);
            transfer_flow = pneumatic_conductance * (pressure_hydraulic - pressure_pneumatic);
            pressure_hydraulic += hydraulic_flow / hydraulic_compliance * dt;
            pressure_pneumatic += transfer_flow / pneumatic_compliance * dt;

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
        state[base + FLOW_H] = hydraulic_flow;
        state[base + FLOW_P] = transfer_flow;

        let observation_base = (scenario as usize) * OBSERVATION_STRIDE;
        observations[observation_base + OBS_SOURCE_SCALE] = source_scale;
        observations[observation_base + OBS_VALVE_OPENING] = valve_opening;
        observations[observation_base + OBS_PRESSURE_H] = pressure_hydraulic;
        observations[observation_base + OBS_PRESSURE_P] = pressure_pneumatic;
        observations[observation_base + OBS_FLOW_H] = hydraulic_flow;
        observations[observation_base + OBS_FLOW_P] = transfer_flow;
        observations[observation_base + OBS_POSITION] = position;
        observations[observation_base + OBS_VELOCITY] = velocity;

        if scenario == 0u32 {
            sample[PRESSURE_H] = pressure_hydraulic;
            sample[PRESSURE_P] = pressure_pneumatic;
            sample[FLOW_H] = hydraulic_flow;
            sample[FLOW_P] = transfer_flow;
            sample[POSITION] = position;
            sample[VELOCITY] = velocity;
            sample[PHASE] = valve_opening;
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
        [-1.42, -0.48, -0.32],
        format!(
            "valve={:.0}%  flow h={:.4} m³/s  p→={:.4} m³/s",
            sample.valve_opening * 100.0,
            sample.flow_hydraulic,
            sample.flow_pneumatic
        ),
        Color32::from_rgb(245, 190, 110),
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
    last_sweep: Option<SweepSummary>,
    selected_scenario: usize,
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
            last_sweep: None,
            selected_scenario: 0,
            error: None,
        }
    }

    fn frame(&mut self) -> PhysicsScene {
        match self.gpu.advance(STEPS_PER_FRAME) {
            Ok(sample) => {
                self.error = None;
                self.last = Some(sample);
                match self.gpu.read_sweep() {
                    Ok(summary) => self.last_sweep = Some(summary),
                    Err(error) => self.error = Some(error),
                }
                if self.selected_scenario != 0 {
                    match self.gpu.read_scenario(self.selected_scenario) {
                        Ok(sample) => self.last = Some(sample),
                        Err(error) => self.error = Some(error),
                    }
                }
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
        ctx.label("256 source-amplitude + valve-opening scenarios evolve on the GPU; choose which sampled state to display.");
        ctx.label(format!("Displayed scenario: {}", state.selected_scenario));
        ctx.slider(&mut state.selected_scenario, 0..=SCENARIOS - 1);
        if let Some(error) = &state.error {
            ctx.label(format!("GPU error: {error}"));
        }
        if let Some(sweep) = state.last_sweep {
            ctx.label(format!(
                "GPU sweep: {} scenarios | valve [{:.0}%, {:.0}%] | piston x [{:.4}, {:.4}] m | peak pneumatic {:.1} kPa | peak transfer {:.4} m³/s",
                sweep.scenarios,
                sweep.min_valve_opening * 100.0,
                sweep.max_valve_opening * 100.0,
                sweep.min_position,
                sweep.max_position,
                sweep.max_pressure_pneumatic / 1_000.0,
                sweep.peak_flow_pneumatic,
            ));
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
        assert!(sample.flow_hydraulic.abs() > 0.0);
        assert!(sample.flow_pneumatic.abs() > 0.0);
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
        assert!(sample.flow_hydraulic.abs() < 1.0e-8);
        assert!(sample.flow_pneumatic.abs() < 1.0e-8);
        assert!(sample.position.abs() < 1.0e-6);
        assert!(sample.velocity.abs() < 1.0e-4);
    }

    #[test]
    fn gpu_kernel_emits_a_distinct_observation_for_each_scenario() {
        let mut gpu = HydroGpu::new();
        gpu.advance(20_000).expect("CubeCL WGPU step");
        let summary = gpu.read_sweep().expect("CubeCL WGPU sweep readback");
        assert_eq!(summary.scenarios, SCENARIOS);
        assert!((summary.min_valve_opening - 0.20).abs() < 1.0e-6);
        assert!((summary.max_valve_opening - 1.00).abs() < 1.0e-6);
        assert!(summary.max_position > summary.min_position);
        assert!(summary.max_pressure_pneumatic > AMBIENT_PRESSURE);
        assert!(summary.peak_flow_pneumatic > 0.0);
    }

    #[test]
    fn gpu_observation_selection_changes_the_displayed_scenario() {
        let mut gpu = HydroGpu::new();
        gpu.advance(20_000).expect("CubeCL WGPU step");
        let low = gpu.read_scenario(0).expect("scenario 0 readback");
        let high = gpu
            .read_scenario(SCENARIOS - 1)
            .expect("last scenario readback");
        assert!(high.valve_opening > low.valve_opening);
        assert!(high.pressure_pneumatic > low.pressure_pneumatic);
        assert!(high.flow_pneumatic > low.flow_pneumatic);
        assert!(high.position > low.position);
    }
}
