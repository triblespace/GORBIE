//! A small GPU-first thermal reservoir experiment.
//!
//! CubeCL advances one thermal reservoir per scenario.  Each scenario has a
//! different constant compute-heat source, while a shared extraction load
//! represents useful heat demand.  The kernel keeps temperature and energy
//! accounting on the device; GORBIE reads only the explicit observation rows.
//!
//! ```text
//! cargo run --release --example thermal_cubecl --features cubecl
//! ```

use std::time::Duration;

use cubecl::client::ComputeClient;
use cubecl::prelude::*;
use cubecl::server::Handle;
use cubecl::wgpu::{WgpuDevice, WgpuRuntime};
use eframe::egui::Color32;

use GORBIE::widgets::{Bounds3, Label3, LegendEntry, Line3, Particle3, PhysicsScene, PhysicsView};
use GORBIE::{notebook, NotebookCtx};

const STATE_STRIDE: usize = 4;
const TEMPERATURE: usize = 0;
const INJECTED_ENERGY: usize = 1;
const EXTRACTED_ENERGY: usize = 2;
const SIM_TIME: usize = 3;

const OBSERVATION_STRIDE: usize = 6;
const OBS_COMPUTE_POWER: usize = 0;
const OBS_TEMPERATURE: usize = 1;
const OBS_INJECTED_ENERGY: usize = 2;
const OBS_EXTRACTED_ENERGY: usize = 3;
const OBS_NET_ENERGY: usize = 4;
const OBS_TIME: usize = 5;

const INITIAL_TEMPERATURE: f32 = 293.15;
const HEAT_CAPACITY: f32 = 4.0e6;
const EXTRACTION_POWER: f32 = 20_000.0;
const COMPUTE_POWER_MAX: f32 = 50_000.0;
const DT: f32 = 0.5;
const SCENARIOS: usize = 256;
const STEPS_PER_FRAME: u32 = 60;

#[derive(Clone, Copy, Debug)]
struct ThermalSample {
    compute_power: f32,
    temperature: f32,
    injected_energy: f32,
    extracted_energy: f32,
    net_energy: f32,
    time: f32,
}

#[derive(Clone, Copy, Debug)]
struct ThermalSummary {
    scenarios: usize,
    min_temperature: f32,
    max_temperature: f32,
    max_compute_power: f32,
    max_energy_residual: f32,
}

struct ThermalGpu {
    _device: WgpuDevice,
    client: ComputeClient<WgpuRuntime>,
    state: Handle,
    compute_powers: Handle,
    observations: Handle,
    scenario_count: usize,
    extraction_power: f32,
}

impl ThermalGpu {
    fn new() -> Self {
        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);

        let mut initial = vec![0.0f32; SCENARIOS * STATE_STRIDE];
        for scenario in 0..SCENARIOS {
            initial[scenario * STATE_STRIDE + TEMPERATURE] = INITIAL_TEMPERATURE;
        }
        let compute_powers: Vec<f32> = (0..SCENARIOS)
            .map(|scenario| COMPUTE_POWER_MAX * scenario as f32 / (SCENARIOS - 1) as f32)
            .collect();

        Self {
            _device: device,
            state: client.create_from_slice(f32::as_bytes(&initial)),
            compute_powers: client.create_from_slice(f32::as_bytes(&compute_powers)),
            observations: client.empty(
                SCENARIOS * OBSERVATION_STRIDE * std::mem::size_of::<f32>(),
            ),
            client,
            scenario_count: SCENARIOS,
            extraction_power: EXTRACTION_POWER,
        }
    }

    fn advance(&mut self, steps: u32) -> Result<ThermalSample, String> {
        if steps == 0 {
            return Err("GPU step count must be nonzero".into());
        }
        let state_len = self.scenario_count * STATE_STRIDE;
        unsafe {
            thermal_step_kernel::launch::<WgpuRuntime>(
                &self.client,
                CubeCount::new_1d(self.scenario_count as u32),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts(self.state.clone(), state_len),
                ArrayArg::from_raw_parts(self.compute_powers.clone(), self.scenario_count),
                ArrayArg::from_raw_parts(
                    self.observations.clone(),
                    self.scenario_count * OBSERVATION_STRIDE,
                ),
                self.scenario_count as u32,
                steps,
                DT,
                INITIAL_TEMPERATURE,
                HEAT_CAPACITY,
                self.extraction_power,
            );
        }
        self.read_scenario(0)
    }

    fn read_scenario(&self, scenario: usize) -> Result<ThermalSample, String> {
        self.read_observations(scenario).map(|(_, sample)| sample)
    }

    fn read_sweep(&self) -> Result<ThermalSummary, String> {
        self.read_observations(0).map(|(summary, _)| summary)
    }

    fn read_observations(
        &self,
        selected_scenario: usize,
    ) -> Result<(ThermalSummary, ThermalSample), String> {
        if selected_scenario >= self.scenario_count {
            return Err(format!(
                "scenario {selected_scenario} is outside 0..{}",
                self.scenario_count
            ));
        }
        let bytes = self
            .client
            .read_one(self.observations.clone())
            .map_err(|error| format!("GPU thermal readback failed: {error:?}"))?;
        let values = f32::from_bytes(&bytes);
        let expected = self.scenario_count * OBSERVATION_STRIDE;
        if values.len() != expected {
            return Err(format!(
                "GPU thermal readback returned {} floats, expected {expected}",
                values.len()
            ));
        }

        let mut min_temperature = f32::INFINITY;
        let mut max_temperature = f32::NEG_INFINITY;
        let mut max_compute_power = f32::NEG_INFINITY;
        // Keep the residual visible: f32 energy accumulation introduces a
        // small, measurable roundoff error over long sweeps rather than an
        // excuse to silently drop the conservation check.
        let mut max_energy_residual: f32 = 0.0;
        for observation in values.chunks_exact(OBSERVATION_STRIDE) {
            let expected_net =
                (observation[OBS_TEMPERATURE] - INITIAL_TEMPERATURE) * HEAT_CAPACITY;
            min_temperature = min_temperature.min(observation[OBS_TEMPERATURE]);
            max_temperature = max_temperature.max(observation[OBS_TEMPERATURE]);
            max_compute_power = max_compute_power.max(observation[OBS_COMPUTE_POWER]);
            max_energy_residual = max_energy_residual
                .max((observation[OBS_NET_ENERGY] - expected_net).abs());
        }

        let selected = &values[selected_scenario * OBSERVATION_STRIDE
            ..(selected_scenario + 1) * OBSERVATION_STRIDE];
        Ok((
            ThermalSummary {
                scenarios: self.scenario_count,
                min_temperature,
                max_temperature,
                max_compute_power,
                max_energy_residual,
            },
            ThermalSample {
                compute_power: selected[OBS_COMPUTE_POWER],
                temperature: selected[OBS_TEMPERATURE],
                injected_energy: selected[OBS_INJECTED_ENERGY],
                extracted_energy: selected[OBS_EXTRACTED_ENERGY],
                net_energy: selected[OBS_NET_ENERGY],
                time: selected[OBS_TIME],
            },
        ))
    }
}

#[cube(launch)]
fn thermal_step_kernel(
    state: &mut Array<f32>,
    compute_powers: &Array<f32>,
    observations: &mut Array<f32>,
    scenario_count: u32,
    steps: u32,
    dt: f32,
    initial_temperature: f32,
    heat_capacity: f32,
    extraction_power: f32,
) {
    let scenario = ABSOLUTE_POS as u32;
    if scenario < scenario_count {
        let base = scenario as usize * STATE_STRIDE;
        let compute_power = compute_powers[scenario as usize];
        let mut temperature = state[base + TEMPERATURE];
        let mut injected_energy = state[base + INJECTED_ENERGY];
        let mut extracted_energy = state[base + EXTRACTED_ENERGY];
        let mut sim_time = state[base + SIM_TIME];
        let mut step = 0u32;

        while step < steps {
            injected_energy += compute_power * dt;
            extracted_energy += extraction_power * dt;
            temperature =
                initial_temperature + (injected_energy - extracted_energy) / heat_capacity;
            sim_time += dt;
            step += 1u32;
        }

        state[base + TEMPERATURE] = temperature;
        state[base + INJECTED_ENERGY] = injected_energy;
        state[base + EXTRACTED_ENERGY] = extracted_energy;
        state[base + SIM_TIME] = sim_time;

        let observation_base = scenario as usize * OBSERVATION_STRIDE;
        observations[observation_base + OBS_COMPUTE_POWER] = compute_power;
        observations[observation_base + OBS_TEMPERATURE] = temperature;
        observations[observation_base + OBS_INJECTED_ENERGY] = injected_energy;
        observations[observation_base + OBS_EXTRACTED_ENERGY] = extracted_energy;
        observations[observation_base + OBS_NET_ENERGY] = injected_energy - extracted_energy;
        observations[observation_base + OBS_TIME] = sim_time;
    }
}

fn scene(sample: ThermalSample) -> PhysicsScene {
    let mut scene = PhysicsScene::default();
    let temperature_mix = ((sample.temperature - INITIAL_TEMPERATURE) / 4.0).clamp(0.0, 1.0);
    let reservoir_color = Color32::from_rgb(
        (70.0 + temperature_mix * 185.0) as u8,
        (170.0 - temperature_mix * 100.0) as u8,
        240,
    );
    scene.wire_box(
        Bounds3 {
            min: [-1.25, -0.40, -0.30],
            max: [0.30, 0.40, 0.30],
        },
        reservoir_color,
    );
    scene.wire_box(
        Bounds3 {
            min: [0.70, -0.30, -0.22],
            max: [1.35, 0.30, 0.22],
        },
        Color32::from_rgb(170, 130, 230),
    );
    scene.lines.push(Line3::new(
        [0.30, 0.0, 0.0],
        [0.70, 0.0, 0.0],
        Color32::from_rgb(245, 170, 80),
    ));
    scene.particles.push(Particle3::new(
        [-0.45, 0.0, 0.0],
        0.16,
        reservoir_color,
    ));
    scene.labels.push(Label3::new(
        [-1.20, 0.52, -0.30],
        format!("reservoir {:.2} K", sample.temperature),
        reservoir_color,
    ));
    scene.labels.push(Label3::new(
        [0.72, 0.42, -0.22],
        format!("compute {:.1} kW", sample.compute_power / 1_000.0),
        Color32::from_rgb(205, 165, 245),
    ));
    scene.labels.push(Label3::new(
        [-1.20, -0.52, -0.30],
        format!(
            "in {:.2} / out {:.2} / net {:.2} kJ  t={:.1} s",
            sample.injected_energy / 1_000.0,
            sample.extracted_energy / 1_000.0,
            sample.net_energy / 1_000.0,
            sample.time
        ),
        Color32::from_rgb(245, 190, 110),
    ));
    scene.legend.push(LegendEntry::new(
        "CubeCL thermal state: compute heat → reservoir",
        reservoir_color,
    ));
    scene.units = "K".into();
    scene
}

struct ThermalNotebook {
    gpu: ThermalGpu,
    camera: PhysicsView,
    last: Option<ThermalSample>,
    last_sweep: Option<ThermalSummary>,
    selected_scenario: usize,
    error: Option<String>,
}

impl ThermalNotebook {
    fn new() -> Self {
        Self {
            gpu: ThermalGpu::new(),
            camera: PhysicsView::default()
                .height(390.0)
                .bounds(Bounds3 {
                    min: [-1.5, -0.7, -0.6],
                    max: [1.5, 0.7, 0.6],
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
    nb.state_with("thermal-cubecl", ThermalNotebook::new, |ctx, state| {
        let scene = state.frame();
        ctx.heading("CubeCL thermal reservoir");
        ctx.label("256 compute-heat scenarios evolve on the GPU; choose which sampled state to display.");
        ctx.label(format!("Displayed scenario: {}", state.selected_scenario));
        ctx.slider(&mut state.selected_scenario, 0..=SCENARIOS - 1);
        if let Some(error) = &state.error {
            ctx.label(format!("GPU error: {error}"));
        }
        if let Some(sweep) = state.last_sweep {
            ctx.label(format!(
                "GPU sweep: {} scenarios | T [{:.2}, {:.2}] K | compute max {:.1} kW | energy residual {:.3} J",
                sweep.scenarios,
                sweep.min_temperature,
                sweep.max_temperature,
                sweep.max_compute_power / 1_000.0,
                sweep.max_energy_residual,
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
    fn gpu_kernel_matches_analytical_energy_balance() {
        let mut gpu = ThermalGpu::new();
        let steps = 2_000;
        gpu.advance(steps).expect("CubeCL WGPU step");
        let scenario = SCENARIOS - 1;
        let sample = gpu.read_scenario(scenario).expect("thermal observation");
        let compute_power = COMPUTE_POWER_MAX;
        let elapsed = DT * steps as f32;
        let expected_temperature =
            INITIAL_TEMPERATURE + (compute_power - EXTRACTION_POWER) * elapsed / HEAT_CAPACITY;
        assert!(
            (sample.temperature - expected_temperature).abs() < 1.0e-3,
            "GPU T={} expected={} residual={} net={} time={}",
            sample.temperature,
            expected_temperature,
            sample.temperature - expected_temperature,
            sample.net_energy,
            sample.time
        );
        assert!((sample.net_energy - (compute_power - EXTRACTION_POWER) * elapsed).abs() < 32.0);
    }

    #[test]
    fn gpu_kernel_keeps_zero_power_reservoir_at_equilibrium() {
        let mut gpu = ThermalGpu::new();
        gpu.extraction_power = 0.0;
        gpu.advance(2_000).expect("CubeCL WGPU step");
        let sample = gpu.read_scenario(0).expect("thermal observation");
        assert!((sample.temperature - INITIAL_TEMPERATURE).abs() < 1.0e-5);
        assert!(sample.net_energy.abs() < 1.0e-3);
    }

    #[test]
    fn gpu_kernel_emits_a_compute_power_sweep() {
        let mut gpu = ThermalGpu::new();
        gpu.advance(2_000).expect("CubeCL WGPU step");
        let summary = gpu.read_sweep().expect("thermal sweep");
        assert_eq!(summary.scenarios, SCENARIOS);
        assert!(summary.max_temperature > summary.min_temperature);
        assert!((summary.max_compute_power - COMPUTE_POWER_MAX).abs() < 1.0e-3);
        assert!(
            summary.max_energy_residual < 128.0,
            "maximum GPU energy residual was {} J",
            summary.max_energy_residual
        );
    }
}
