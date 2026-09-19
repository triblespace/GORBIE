//! Headless CubeCL → TribleSpace snapshot proof for the Erlkonig substrate.
//!
//! CubeCL advances four pressure/compliance scenarios on the GPU. Only an
//! explicit observation plan crosses the device boundary; those sampled
//! quantities are appended as semantic frame/value entities to a persistent
//! native TribleSpace pile.
//!
//! ```text
//! cargo run --release --example fluid_network_snapshot \
//!   --features cubecl,triblespace -- /path/to/erlkonig-snapshots.pile
//! ```

use std::path::PathBuf;

use GORBIE::simulation_snapshots::{SnapshotValue, SnapshotWriter, new_identity};
use cubecl::client::ComputeClient;
use cubecl::prelude::*;
use cubecl::server::Handle;
use cubecl::wgpu::{WgpuDevice, WgpuRuntime};

const STATE_STRIDE: usize = 3;
const PRESSURE: usize = 0;
const FLOW: usize = 1;
const TIME: usize = 2;
const OBS_STRIDE: usize = 4;
const OBS_PRESSURE: usize = 0;
const OBS_FLOW: usize = 1;
const OBS_TIME: usize = 2;
const OBS_ENERGY: usize = 3;
const SCENARIOS: usize = 4;
const DT: f32 = 1.0e-4;
const STEPS_PER_FRAME: u32 = 1_000;
const FRAME_COUNT: u64 = 12;
const AMBIENT_PRESSURE: f32 = 100_000.0;
const SOURCE_AMPLITUDE: f32 = 280_000.0;
const SOURCE_ANGULAR_FREQUENCY: f32 = 2.0 * std::f32::consts::PI * 0.8;
const NODE_COMPLIANCE: f32 = 4.0e-9;
const INERTANCE: f32 = 2.0e7;
const RESISTANCE: f32 = 4.0e8;

struct FluidGpu {
    _device: WgpuDevice,
    client: ComputeClient<WgpuRuntime>,
    state: Handle,
    source_scales: Handle,
    observations: Handle,
}

impl FluidGpu {
    fn new() -> Self {
        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);
        let mut initial = vec![0.0f32; SCENARIOS * STATE_STRIDE];
        for scenario in 0..SCENARIOS {
            initial[scenario * STATE_STRIDE + PRESSURE] = AMBIENT_PRESSURE;
        }
        let source_scales = vec![0.6, 0.9, 1.2, 1.5];
        Self {
            _device: device,
            state: client.create_from_slice(f32::as_bytes(&initial)),
            source_scales: client.create_from_slice(f32::as_bytes(&source_scales)),
            observations: client.empty(SCENARIOS * OBS_STRIDE * std::mem::size_of::<f32>()),
            client,
        }
    }

    fn advance(&mut self, steps: u32) -> Result<Vec<[f32; OBS_STRIDE]>, String> {
        unsafe {
            fluid_snapshot_kernel::launch::<WgpuRuntime>(
                &self.client,
                CubeCount::new_1d(SCENARIOS as u32),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts(self.state.clone(), SCENARIOS * STATE_STRIDE),
                ArrayArg::from_raw_parts(self.source_scales.clone(), SCENARIOS),
                ArrayArg::from_raw_parts(self.observations.clone(), SCENARIOS * OBS_STRIDE),
                SCENARIOS as u32,
                steps,
                DT,
                AMBIENT_PRESSURE,
                SOURCE_AMPLITUDE,
                SOURCE_ANGULAR_FREQUENCY,
                NODE_COMPLIANCE,
                INERTANCE,
                RESISTANCE,
            );
        }
        let bytes = self
            .client
            .read_one(self.observations.clone())
            .map_err(|error| format!("read CubeCL observations: {error:?}"))?;
        let values = f32::from_bytes(&bytes);
        if values.len() != SCENARIOS * OBS_STRIDE {
            return Err(format!(
                "CubeCL returned {} values, expected {}",
                values.len(),
                SCENARIOS * OBS_STRIDE
            ));
        }
        Ok(values
            .chunks_exact(OBS_STRIDE)
            .map(|row| [row[0], row[1], row[2], row[3]])
            .collect())
    }
}

#[cube(launch)]
fn fluid_snapshot_kernel(
    state: &mut Array<f32>,
    source_scales: &Array<f32>,
    observations: &mut Array<f32>,
    scenario_count: u32,
    steps: u32,
    dt: f32,
    ambient_pressure: f32,
    source_amplitude: f32,
    source_angular_frequency: f32,
    node_compliance: f32,
    inertance: f32,
    resistance: f32,
) {
    let scenario = ABSOLUTE_POS as u32;
    if scenario < scenario_count {
        let base = scenario as usize * STATE_STRIDE;
        let source_scale = source_scales[scenario as usize];
        let mut pressure = state[base + PRESSURE];
        let mut flow = state[base + FLOW];
        let mut sim_time = state[base + TIME];
        let mut step = 0u32;
        while step < steps {
            let source = ambient_pressure
                + source_amplitude
                    * source_scale
                    * (0.5f32 + 0.5f32 * (sim_time * source_angular_frequency).sin());
            flow += ((source - pressure) - resistance * flow) / inertance * dt;
            pressure += flow / node_compliance * dt;
            sim_time += dt;
            step += 1u32;
        }
        state[base + PRESSURE] = pressure;
        state[base + FLOW] = flow;
        state[base + TIME] = sim_time;
        let stored_energy = 0.5f32
            * node_compliance
            * (pressure - ambient_pressure)
            * (pressure - ambient_pressure);
        let flow_energy = 0.5f32 * inertance * flow * flow;
        let observation_base = scenario as usize * OBS_STRIDE;
        observations[observation_base + OBS_PRESSURE] = pressure;
        observations[observation_base + OBS_FLOW] = flow;
        observations[observation_base + OBS_TIME] = sim_time;
        observations[observation_base + OBS_ENERGY] = stored_energy + flow_energy;
    }
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("erlkonig-simulation-snapshots.pile"));
    let run_id = new_identity();
    let scenario_id = new_identity();
    let subject_id = new_identity();
    let mut writer = SnapshotWriter::open(
        &path,
        run_id,
        "fluid-network-cubecl-snapshot",
        scenario_id,
        "source-pressure-compliance-sweep",
    )
    .expect("open snapshot writer");
    let mut gpu = FluidGpu::new();

    for frame_index in 0..FRAME_COUNT {
        let observations = gpu.advance(STEPS_PER_FRAME).expect("CubeCL frame");
        let observation = observations[0];
        let frame = writer
            .append_frame(
                frame_index,
                observation[OBS_TIME] as f64,
                &[
                    SnapshotValue {
                        subject: Some(subject_id),
                        quantity: "pressure",
                        value: observation[OBS_PRESSURE] as f64,
                        unit: "Pa",
                    },
                    SnapshotValue {
                        subject: Some(subject_id),
                        quantity: "flow",
                        value: observation[OBS_FLOW] as f64,
                        unit: "m3/s",
                    },
                    SnapshotValue {
                        subject: None,
                        quantity: "stored_energy",
                        value: observation[OBS_ENERGY] as f64,
                        unit: "J",
                    },
                ],
            )
            .expect("append semantic frame");
        println!(
            "frame {:>2} t={:>6.3}s p={:>9.1}Pa q={:>9.6}m3/s E={:>8.3}J values={} id={}",
            frame.index,
            frame.time,
            observation[OBS_PRESSURE],
            observation[OBS_FLOW],
            observation[OBS_ENERGY],
            frame.values,
            frame.id,
        );
    }
    writer.close().expect("close snapshot pile");
    println!("persisted {FRAME_COUNT} frames to {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubecl_snapshot_kernel_emits_finite_frames() {
        let mut gpu = FluidGpu::new();
        let observations = gpu.advance(STEPS_PER_FRAME).expect("CubeCL frame");
        assert_eq!(observations.len(), SCENARIOS);
        assert!(
            observations
                .iter()
                .all(|row| row.iter().all(|value| value.is_finite()))
        );
        assert!(observations[0][OBS_PRESSURE] > AMBIENT_PRESSURE);
        assert!(observations[0][OBS_ENERGY] > 0.0);
    }
}
