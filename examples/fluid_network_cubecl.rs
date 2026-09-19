//! A small GPU-first pressure/inertance fluid-network experiment.
//!
//! This is deliberately below CFD: three compliant nodes are connected by
//! inertive, resistive links. CubeCL advances every scenario on the device,
//! while GORBIE reads an explicit observation row for the selected scenario.
//! The model is useful as a substrate test because pressure, flow, storage,
//! and conservation all evolve together instead of being algebraic labels.
//!
//! ```text
//! cargo run --release --example fluid_network_cubecl --features cubecl
//! ```

use std::time::Duration;

use cubecl::client::ComputeClient;
use cubecl::prelude::*;
use cubecl::server::Handle;
use cubecl::wgpu::{WgpuDevice, WgpuRuntime};
use eframe::egui::Color32;

use GORBIE::widgets::{Bounds3, Label3, LegendEntry, Line3, Particle3, PhysicsScene, PhysicsView};
use GORBIE::{notebook, NotebookCtx};

const STATE_STRIDE: usize = 7;
const PRESSURE_1: usize = 0;
const PRESSURE_2: usize = 1;
const FLOW_IN: usize = 2;
const FLOW_LINK: usize = 3;
const FLOW_OUT: usize = 4;
const PHASE: usize = 5;
const SIM_TIME: usize = 6;

const OBSERVATION_STRIDE: usize = 10;
const OBS_SOURCE_SCALE: usize = 0;
const OBS_RESISTANCE_SCALE: usize = 1;
const OBS_SOURCE_PRESSURE: usize = 2;
const OBS_PRESSURE_1: usize = 3;
const OBS_PRESSURE_2: usize = 4;
const OBS_FLOW_IN: usize = 5;
const OBS_FLOW_LINK: usize = 6;
const OBS_FLOW_OUT: usize = 7;
const OBS_STORAGE_RESIDUAL: usize = 8;
const OBS_TIME: usize = 9;

const AMBIENT_PRESSURE: f32 = 100_000.0;
const SOURCE_AMPLITUDE: f32 = 300_000.0;
const SOURCE_ANGULAR_FREQUENCY: f32 = 2.0 * std::f32::consts::PI * 0.8;
const SOURCE_PERIOD: f32 = 1.0 / 0.8;
const NODE_COMPLIANCE: f32 = 4.0e-9;
const INERTANCE_IN: f32 = 2.0e7;
const INERTANCE_LINK: f32 = 3.0e7;
const INERTANCE_OUT: f32 = 2.0e7;
const RESISTANCE_IN: f32 = 4.0e8;
const RESISTANCE_LINK: f32 = 8.0e8;
const RESISTANCE_OUT: f32 = 4.0e8;
const DT: f32 = 1.0e-4;
const SCENARIO_SIDE: usize = 16;
const SCENARIOS: usize = SCENARIO_SIDE * SCENARIO_SIDE;
const STEPS_PER_FRAME: u32 = 1_000;

#[derive(Clone, Copy, Debug)]
struct FluidSample {
    source_scale: f32,
    resistance_scale: f32,
    source_pressure: f32,
    pressure_1: f32,
    pressure_2: f32,
    flow_in: f32,
    flow_link: f32,
    flow_out: f32,
    storage_residual: f32,
    time: f32,
}

#[derive(Clone, Copy, Debug)]
struct FluidSummary {
    scenarios: usize,
    min_source_scale: f32,
    max_source_scale: f32,
    min_resistance_scale: f32,
    max_resistance_scale: f32,
    min_pressure_2: f32,
    max_pressure_2: f32,
    max_abs_flow: f32,
    max_storage_residual: f32,
}

struct FluidGpu {
    _device: WgpuDevice,
    client: ComputeClient<WgpuRuntime>,
    state: Handle,
    source_scales: Handle,
    resistance_scales: Handle,
    observations: Handle,
    scenario_count: usize,
}

impl FluidGpu {
    fn new() -> Self {
        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);

        let mut initial = vec![0.0f32; SCENARIOS * STATE_STRIDE];
        for scenario in 0..SCENARIOS {
            let base = scenario * STATE_STRIDE;
            initial[base + PRESSURE_1] = AMBIENT_PRESSURE;
            initial[base + PRESSURE_2] = AMBIENT_PRESSURE;
        }
        let source_scales: Vec<f32> = (0..SCENARIOS)
            .map(|scenario| {
                let source_index = scenario % SCENARIO_SIDE;
                0.60 + 0.80 * source_index as f32 / (SCENARIO_SIDE - 1) as f32
            })
            .collect();
        let resistance_scales: Vec<f32> = (0..SCENARIOS)
            .map(|scenario| {
                let resistance_index = scenario / SCENARIO_SIDE;
                0.50 + 1.50 * resistance_index as f32 / (SCENARIO_SIDE - 1) as f32
            })
            .collect();

        Self {
            _device: device,
            state: client.create_from_slice(f32::as_bytes(&initial)),
            source_scales: client.create_from_slice(f32::as_bytes(&source_scales)),
            resistance_scales: client.create_from_slice(f32::as_bytes(&resistance_scales)),
            observations: client.empty(SCENARIOS * OBSERVATION_STRIDE * std::mem::size_of::<f32>()),
            client,
            scenario_count: SCENARIOS,
        }
    }

    fn advance(&mut self, steps: u32) -> Result<FluidSample, String> {
        if steps == 0 {
            return Err("GPU step count must be nonzero".into());
        }
        unsafe {
            fluid_network_step_kernel::launch::<WgpuRuntime>(
                &self.client,
                CubeCount::new_1d(self.scenario_count as u32),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts(self.state.clone(), self.scenario_count * STATE_STRIDE),
                ArrayArg::from_raw_parts(self.source_scales.clone(), self.scenario_count),
                ArrayArg::from_raw_parts(self.resistance_scales.clone(), self.scenario_count),
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
                NODE_COMPLIANCE,
                INERTANCE_IN,
                INERTANCE_LINK,
                INERTANCE_OUT,
                RESISTANCE_IN,
                RESISTANCE_LINK,
                RESISTANCE_OUT,
                SOURCE_PERIOD,
            );
        }
        self.read_scenario(0)
    }

    fn read_scenario(&self, scenario: usize) -> Result<FluidSample, String> {
        self.read_observations(scenario).map(|(_, sample)| sample)
    }

    fn read_sweep(&self) -> Result<FluidSummary, String> {
        self.read_observations(0).map(|(summary, _)| summary)
    }

    fn read_observations(
        &self,
        selected_scenario: usize,
    ) -> Result<(FluidSummary, FluidSample), String> {
        if selected_scenario >= self.scenario_count {
            return Err(format!(
                "scenario {selected_scenario} is outside 0..{}",
                self.scenario_count
            ));
        }
        let bytes = self
            .client
            .read_one(self.observations.clone())
            .map_err(|error| format!("GPU fluid readback failed: {error:?}"))?;
        let values = f32::from_bytes(&bytes);
        let expected = self.scenario_count * OBSERVATION_STRIDE;
        if values.len() != expected {
            return Err(format!(
                "GPU fluid readback returned {} floats, expected {expected}",
                values.len()
            ));
        }

        let mut min_source_scale = f32::INFINITY;
        let mut max_source_scale = f32::NEG_INFINITY;
        let mut min_resistance_scale = f32::INFINITY;
        let mut max_resistance_scale = f32::NEG_INFINITY;
        let mut min_pressure_2 = f32::INFINITY;
        let mut max_pressure_2 = f32::NEG_INFINITY;
        let mut max_abs_flow: f32 = 0.0;
        let mut max_storage_residual: f32 = 0.0;
        for observation in values.chunks_exact(OBSERVATION_STRIDE) {
            min_source_scale = min_source_scale.min(observation[OBS_SOURCE_SCALE]);
            max_source_scale = max_source_scale.max(observation[OBS_SOURCE_SCALE]);
            min_resistance_scale = min_resistance_scale.min(observation[OBS_RESISTANCE_SCALE]);
            max_resistance_scale = max_resistance_scale.max(observation[OBS_RESISTANCE_SCALE]);
            min_pressure_2 = min_pressure_2.min(observation[OBS_PRESSURE_2]);
            max_pressure_2 = max_pressure_2.max(observation[OBS_PRESSURE_2]);
            max_abs_flow = max_abs_flow
                .max(observation[OBS_FLOW_IN].abs())
                .max(observation[OBS_FLOW_LINK].abs())
                .max(observation[OBS_FLOW_OUT].abs());
            max_storage_residual =
                max_storage_residual.max(observation[OBS_STORAGE_RESIDUAL].abs());
        }

        let selected = &values
            [selected_scenario * OBSERVATION_STRIDE..(selected_scenario + 1) * OBSERVATION_STRIDE];
        Ok((
            FluidSummary {
                scenarios: self.scenario_count,
                min_source_scale,
                max_source_scale,
                min_resistance_scale,
                max_resistance_scale,
                min_pressure_2,
                max_pressure_2,
                max_abs_flow,
                max_storage_residual,
            },
            FluidSample {
                source_scale: selected[OBS_SOURCE_SCALE],
                resistance_scale: selected[OBS_RESISTANCE_SCALE],
                source_pressure: selected[OBS_SOURCE_PRESSURE],
                pressure_1: selected[OBS_PRESSURE_1],
                pressure_2: selected[OBS_PRESSURE_2],
                flow_in: selected[OBS_FLOW_IN],
                flow_link: selected[OBS_FLOW_LINK],
                flow_out: selected[OBS_FLOW_OUT],
                storage_residual: selected[OBS_STORAGE_RESIDUAL],
                time: selected[OBS_TIME],
            },
        ))
    }
}

#[cube(launch)]
fn fluid_network_step_kernel(
    state: &mut Array<f32>,
    source_scales: &Array<f32>,
    resistance_scales: &Array<f32>,
    observations: &mut Array<f32>,
    scenario_count: u32,
    steps: u32,
    dt: f32,
    ambient_pressure: f32,
    source_amplitude: f32,
    source_angular_frequency: f32,
    node_compliance: f32,
    inertance_in: f32,
    inertance_link: f32,
    inertance_out: f32,
    resistance_in: f32,
    resistance_link: f32,
    resistance_out: f32,
    source_period: f32,
) {
    let scenario = ABSOLUTE_POS as u32;
    if scenario < scenario_count {
        let base = scenario as usize * STATE_STRIDE;
        let source_scale = source_scales[scenario as usize];
        let resistance_scale = resistance_scales[scenario as usize];
        let mut pressure_1 = state[base + PRESSURE_1];
        let mut pressure_2 = state[base + PRESSURE_2];
        let mut flow_in = state[base + FLOW_IN];
        let mut flow_link = state[base + FLOW_LINK];
        let mut flow_out = state[base + FLOW_OUT];
        let mut phase = state[base + PHASE];
        let mut sim_time = state[base + SIM_TIME];
        let mut source_pressure = ambient_pressure;
        let mut storage_residual = 0.0f32;
        let mut step = 0u32;

        while step < steps {
            source_pressure = ambient_pressure
                + source_amplitude
                    * source_scale
                    * (0.5f32 + 0.5f32 * (phase * source_angular_frequency).sin());
            flow_in +=
                ((source_pressure - pressure_1) - resistance_in * flow_in) / inertance_in * dt;
            flow_link += ((pressure_1 - pressure_2)
                - resistance_link * resistance_scale * flow_link)
                / inertance_link
                * dt;
            flow_out += ((pressure_2 - ambient_pressure)
                - resistance_out * resistance_scale * flow_out)
                / inertance_out
                * dt;
            let pressure_1_delta = (flow_in - flow_link) / node_compliance * dt;
            let pressure_2_delta = (flow_link - flow_out) / node_compliance * dt;
            pressure_1 += pressure_1_delta;
            pressure_2 += pressure_2_delta;
            storage_residual =
                (flow_in - flow_out) - node_compliance * (pressure_1_delta + pressure_2_delta) / dt;
            sim_time += dt;
            phase += dt;
            if phase >= source_period {
                phase -= source_period;
            }
            step += 1u32;
        }

        state[base + PRESSURE_1] = pressure_1;
        state[base + PRESSURE_2] = pressure_2;
        state[base + FLOW_IN] = flow_in;
        state[base + FLOW_LINK] = flow_link;
        state[base + FLOW_OUT] = flow_out;
        state[base + PHASE] = phase;
        state[base + SIM_TIME] = sim_time;

        let observation_base = scenario as usize * OBSERVATION_STRIDE;
        observations[observation_base + OBS_SOURCE_SCALE] = source_scale;
        observations[observation_base + OBS_RESISTANCE_SCALE] = resistance_scale;
        observations[observation_base + OBS_SOURCE_PRESSURE] = source_pressure;
        observations[observation_base + OBS_PRESSURE_1] = pressure_1;
        observations[observation_base + OBS_PRESSURE_2] = pressure_2;
        observations[observation_base + OBS_FLOW_IN] = flow_in;
        observations[observation_base + OBS_FLOW_LINK] = flow_link;
        observations[observation_base + OBS_FLOW_OUT] = flow_out;
        observations[observation_base + OBS_STORAGE_RESIDUAL] = storage_residual;
        observations[observation_base + OBS_TIME] = sim_time;
    }
}

fn scene(sample: FluidSample) -> PhysicsScene {
    let mut scene = PhysicsScene::default();
    let pressure_mix = ((sample.pressure_2 - AMBIENT_PRESSURE) / SOURCE_AMPLITUDE).clamp(0.0, 1.0);
    let color = Color32::from_rgb(
        (70.0 + pressure_mix * 185.0) as u8,
        (175.0 - pressure_mix * 90.0) as u8,
        235,
    );
    scene.wire_box(
        Bounds3 {
            min: [-1.40, -0.34, -0.28],
            max: [-0.65, 0.34, 0.28],
        },
        Color32::from_rgb(90, 150, 230),
    );
    scene.wire_box(
        Bounds3 {
            min: [-0.35, -0.34, -0.28],
            max: [0.35, 0.34, 0.28],
        },
        color,
    );
    scene.wire_box(
        Bounds3 {
            min: [0.65, -0.34, -0.28],
            max: [1.40, 0.34, 0.28],
        },
        Color32::from_rgb(190, 135, 225),
    );
    scene.lines.push(Line3::new(
        [-0.65, 0.0, 0.0],
        [-0.35, 0.0, 0.0],
        Color32::from_rgb(245, 175, 85),
    ));
    scene.lines.push(Line3::new(
        [0.35, 0.0, 0.0],
        [0.65, 0.0, 0.0],
        Color32::from_rgb(245, 175, 85),
    ));
    scene
        .particles
        .push(Particle3::new([-1.02, 0.0, 0.0], 0.13, color));
    scene
        .particles
        .push(Particle3::new([0.0, 0.0, 0.0], 0.13, color));
    scene
        .particles
        .push(Particle3::new([1.02, 0.0, 0.0], 0.13, color));
    scene.labels.push(Label3::new(
        [-1.37, 0.46, -0.28],
        format!("source {:.1} kPa", sample.source_pressure / 1_000.0),
        Color32::from_rgb(110, 190, 240),
    ));
    scene.labels.push(Label3::new(
        [-0.31, 0.46, -0.28],
        format!("node 1 {:.1} kPa", sample.pressure_1 / 1_000.0),
        color,
    ));
    scene.labels.push(Label3::new(
        [0.67, 0.46, -0.28],
        format!("node 2 {:.1} kPa", sample.pressure_2 / 1_000.0),
        Color32::from_rgb(205, 165, 245),
    ));
    scene.labels.push(Label3::new(
        [-1.37, -0.50, -0.28],
        format!(
            "q in/link/out {:.5}/{:.5}/{:.5} m³/s",
            sample.flow_in, sample.flow_link, sample.flow_out
        ),
        Color32::from_rgb(245, 190, 110),
    ));
    scene.labels.push(Label3::new(
        [0.67, -0.50, -0.28],
        format!(
            "source {:.2}×  R {:.2}×  storage {:.2e}  t={:.2}s",
            sample.source_scale, sample.resistance_scale, sample.storage_residual, sample.time
        ),
        Color32::from_rgb(245, 190, 110),
    ));
    scene.legend.push(LegendEntry::new(
        "CubeCL fluid state: pressure → inertive flow → storage",
        color,
    ));
    scene.units = "Pa, m³/s".into();
    scene
}

struct FluidNotebook {
    gpu: FluidGpu,
    camera: PhysicsView,
    last: Option<FluidSample>,
    last_sweep: Option<FluidSummary>,
    selected_scenario: usize,
    error: Option<String>,
}

impl FluidNotebook {
    fn new() -> Self {
        Self {
            gpu: FluidGpu::new(),
            camera: PhysicsView::default().height(390.0).bounds(Bounds3 {
                min: [-1.6, -0.7, -0.6],
                max: [1.6, 0.7, 0.6],
            }),
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
        self.last.map(scene).unwrap_or_default()
    }
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    nb.state_with("fluid-network-cubecl", FluidNotebook::new, |ctx, state| {
        let scene = state.frame();
        ctx.heading("CubeCL pressure/inertance fluid network");
        ctx.label(format!(
            "{} source × resistance scenarios evolve on the GPU; {:.2} s source period; choose a sampled network state.",
            SCENARIOS, SOURCE_PERIOD
        ));
        ctx.label(format!("Displayed scenario: {}", state.selected_scenario));
        ctx.slider(&mut state.selected_scenario, 0..=SCENARIOS - 1);
        if let Some(error) = &state.error {
            ctx.label(format!("GPU error: {error}"));
        }
        if let Some(sweep) = state.last_sweep {
            ctx.label(format!(
                "GPU sweep: {} scenarios | source [{:.2}, {:.2}]× | R [{:.2}, {:.2}]× | node-2 pressure [{:.1}, {:.1}] kPa | flow {:.5} m³/s | storage residual {:.2e}",
                sweep.scenarios,
                sweep.min_source_scale,
                sweep.max_source_scale,
                sweep.min_resistance_scale,
                sweep.max_resistance_scale,
                sweep.min_pressure_2 / 1_000.0,
                sweep.max_pressure_2 / 1_000.0,
                sweep.max_abs_flow,
                sweep.max_storage_residual,
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
    fn gpu_kernel_keeps_ambient_equilibrium_without_source() {
        let mut gpu = FluidGpu::new();
        gpu.source_scales = gpu
            .client
            .create_from_slice(f32::as_bytes(&vec![0.0; SCENARIOS]));
        gpu.advance(20_000).expect("CubeCL WGPU step");
        let sample = gpu.read_scenario(0).expect("fluid observation");
        assert!((sample.pressure_1 - AMBIENT_PRESSURE).abs() < 1.0);
        assert!((sample.pressure_2 - AMBIENT_PRESSURE).abs() < 1.0);
        assert!(sample.flow_in.abs() < 1.0e-6);
        assert!(sample.flow_link.abs() < 1.0e-6);
        assert!(sample.flow_out.abs() < 1.0e-6);
    }

    #[test]
    fn gpu_kernel_propagates_pressure_through_the_network() {
        let mut gpu = FluidGpu::new();
        gpu.advance(20_000).expect("CubeCL WGPU step");
        let sample = gpu.read_scenario(0).expect("fluid observation");
        assert!(sample.pressure_1 > AMBIENT_PRESSURE);
        assert!(sample.pressure_2 > AMBIENT_PRESSURE);
        assert!(sample.flow_in > 0.0);
        assert!(sample.flow_link > 0.0);
        assert!(sample.flow_out > 0.0);
    }

    #[test]
    fn gpu_kernel_emits_a_source_and_resistance_sweep() {
        let mut gpu = FluidGpu::new();
        gpu.advance(20_000).expect("CubeCL WGPU step");
        let summary = gpu.read_sweep().expect("fluid sweep");
        assert_eq!(summary.scenarios, SCENARIOS);
        assert!((summary.min_source_scale - 0.60).abs() < 1.0e-6);
        assert!((summary.max_source_scale - 1.40).abs() < 1.0e-6);
        assert!((summary.min_resistance_scale - 0.50).abs() < 1.0e-6);
        assert!((summary.max_resistance_scale - 2.00).abs() < 1.0e-6);
        assert!(summary.max_pressure_2 > summary.min_pressure_2);
        assert!(summary.max_abs_flow > 0.0);
        assert!(summary.max_storage_residual < 1.0e-5);
    }

    #[test]
    fn gpu_observation_selection_changes_network_state() {
        let mut gpu = FluidGpu::new();
        gpu.advance(20_000).expect("CubeCL WGPU step");
        let low_resistance = gpu.read_scenario(0).expect("low-resistance scenario");
        let high_resistance = gpu
            .read_scenario(SCENARIOS - SCENARIO_SIDE)
            .expect("high-resistance scenario");
        assert!(high_resistance.resistance_scale > low_resistance.resistance_scale);
        assert!((high_resistance.pressure_2 - low_resistance.pressure_2).abs() > 1.0);
        assert!((high_resistance.flow_link - low_resistance.flow_link).abs() > 1.0e-7);
    }
}
