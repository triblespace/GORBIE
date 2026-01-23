#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! cubecl = { version = "0.9.0", default-features = false, features = ["wgpu", "std"] }
//! egui = "0.33"
//! egui_plot = "0.34"
//! ```

use cubecl::prelude::*;
use cubecl::server::Handle;
use cubecl::wgpu::{WgpuDevice, WgpuRuntime};
#[cfg(feature = "cubecl-cpu")]
use cubecl::cpu::{CpuDevice, CpuRuntime};
use egui::{DragValue, Spinner};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use std::sync::{
    mpsc::{self, TryRecvError},
    Mutex,
};

use GORBIE::cards::DEFAULT_CARD_PADDING;
use GORBIE::prelude::*;

const NODE_NAMES: [&str; 10] = [
    "source",
    "parse",
    "ast",
    "types",
    "solver",
    "layout",
    "render",
    "widgets",
    "cache",
    "io",
];

const EDGES: [(usize, usize); 14] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 4),
    (4, 5),
    (5, 6),
    (6, 7),
    (0, 9),
    (9, 8),
    (8, 2),
    (8, 6),
    (7, 6),
    (1, 8),
    (3, 5),
];

const NODE_COUNT: usize = NODE_NAMES.len();
const EDGE_COUNT: usize = EDGES.len();
const EDGES_FLAT_LEN: usize = EDGE_COUNT * 2;
const SEED_MIX: u32 = 0x9E37_79B9;
const LCG_A: u32 = 1_664_525;
const LCG_C: u32 = 1_013_904_223;

const DEFAULT_BATCH_SIZE: u32 = 512;
const DEFAULT_ANNEAL_STEPS: u32 = 1000;
const DEFAULT_ANNEAL_TEMP: f32 = 8.0;
const DEFAULT_ANNEAL_COOLING: f32 = 0.995;
const MIN_ANNEAL_TEMP: f32 = 0.001;
const MAX_HISTORY: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    Wgpu,
    Cpu,
}

impl Backend {
    fn label(self) -> &'static str {
        match self {
            Backend::Wgpu => "wgpu",
            Backend::Cpu => "cpu",
        }
    }
}

#[derive(Clone, Debug)]
struct BatchResult {
    backend: Backend,
    best_cost: u32,
    best_order: Vec<usize>,
    batch_size: usize,
    elapsed_ms: u128,
    error: Option<String>,
}

impl BatchResult {
    fn idle() -> Self {
        Self {
            backend: Backend::Wgpu,
            best_cost: u32::MAX,
            best_order: Vec::new(),
            batch_size: 0,
            elapsed_ms: 0,
            error: None,
        }
    }

    fn ok(
        backend: Backend,
        best_cost: u32,
        best_order: Vec<usize>,
        batch_size: usize,
        elapsed_ms: u128,
    ) -> Self {
        Self {
            backend,
            best_cost,
            best_order,
            batch_size,
            elapsed_ms,
            error: None,
        }
    }

    fn failed(backend: Backend, message: impl Into<String>) -> Self {
        Self {
            backend,
            best_cost: u32::MAX,
            best_order: Vec::new(),
            batch_size: 0,
            elapsed_ms: 0,
            error: Some(message.into()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct BatchRequest {
    batch_size: usize,
    seed: u64,
}

struct BatchRunner {
    backend: Backend,
    sender: mpsc::Sender<BatchRequest>,
    receiver: Mutex<mpsc::Receiver<BatchResult>>,
    in_flight: bool,
}

impl BatchRunner {
    fn new_wgpu() -> Self {
        spawn_worker(Backend::Wgpu, wgpu_worker_loop)
    }

    #[cfg(feature = "cubecl-cpu")]
    fn new_cpu() -> Self {
        spawn_worker(Backend::Cpu, cpu_worker_loop)
    }

    fn spawn(&mut self, request: BatchRequest) -> Result<(), BatchResult> {
        if self.in_flight {
            return Ok(());
        }
        if self.sender.send(request).is_err() {
            return Err(BatchResult::failed(self.backend, "worker disconnected"));
        }
        self.in_flight = true;
        Ok(())
    }

    fn poll(&mut self) -> Option<BatchResult> {
        let receiver = self
            .receiver
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match receiver.try_recv() {
            Ok(batch) => {
                self.in_flight = false;
                Some(batch)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.in_flight = false;
                Some(BatchResult::failed(self.backend, "worker disconnected"))
            }
        }
    }

    fn is_running(&self) -> bool {
        self.in_flight
    }
}

struct BatchRunnerState<R: Runtime> {
    backend: Backend,
    _device: R::Device,
    client: ComputeClient<R>,
    edges_handle: Handle,
    edges_len: usize,
}

impl<R: Runtime> BatchRunnerState<R> {
    fn new(backend: Backend, device: R::Device) -> Self {
        let client = R::client(&device);
        let edges_flat = edges_flat();
        let edges_len = edges_flat.len();
        let edges_handle = client.create_from_slice(u32::as_bytes(&edges_flat));
        Self {
            backend,
            _device: device,
            client,
            edges_handle,
            edges_len,
        }
    }

    fn run(&mut self, batch_size: usize, seed: u64) -> BatchResult {
        if batch_size == 0 {
            return BatchResult::failed(self.backend, "batch size must be > 0");
        }

        let start = std::time::Instant::now();
        let seed32 = seed_to_u32(seed);
        if self.backend == Backend::Cpu {
            if let Some((best_cost, best_order)) = best_order_cpu(batch_size, seed32) {
                return BatchResult::ok(
                    self.backend,
                    best_cost,
                    best_order,
                    batch_size,
                    start.elapsed().as_millis(),
                );
            }
            return BatchResult::failed(self.backend, "no valid CPU permutations produced");
        }

        let output_handle = self.client.empty(batch_size * std::mem::size_of::<u32>());
        let orders_len = batch_size * NODE_COUNT;
        let orders_handle = self
            .client
            .empty(orders_len * std::mem::size_of::<u32>());

        unsafe {
            minla_cost_kernel::launch::<R>(
                &self.client,
                CubeCount::new_1d(batch_size as u32),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts::<u32>(&self.edges_handle, self.edges_len, 1),
                ScalarArg::new(seed32),
                ArrayArg::from_raw_parts::<u32>(&output_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&orders_handle, orders_len, 1),
            )
            .expect("minla kernel launch");
        }

        let costs_bytes = self.client.read_one(output_handle);
        let costs = u32::from_bytes(&costs_bytes);

        let mut best_cost = u32::MAX;
        let mut best_idx = 0usize;
        for (idx, cost) in costs.iter().enumerate() {
            if *cost < best_cost {
                best_cost = *cost;
                best_idx = idx;
            }
        }

        let order_stride = NODE_COUNT * std::mem::size_of::<u32>();
        let offset_start = (best_idx * order_stride) as u64;
        let offset_end = (batch_size.saturating_sub(best_idx + 1) * order_stride) as u64;
        let order_handle = orders_handle
            .clone()
            .offset_start(offset_start)
            .offset_end(offset_end);
        let order_bytes = self.client.read_one(order_handle);
        let order = u32::from_bytes(&order_bytes);
        let best_order = order.iter().map(|&val| val as usize).collect();

        BatchResult::ok(
            self.backend,
            best_cost,
            best_order,
            batch_size,
            start.elapsed().as_millis(),
        )
    }
}

fn spawn_worker(
    backend: Backend,
    worker: fn(mpsc::Receiver<BatchRequest>, mpsc::Sender<BatchResult>),
) -> BatchRunner {
    let (request_tx, request_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    std::thread::spawn(move || worker(request_rx, result_tx));
    BatchRunner {
        backend,
        sender: request_tx,
        receiver: Mutex::new(result_rx),
        in_flight: false,
    }
}

fn build_wgpu_runner() -> BatchRunnerState<WgpuRuntime> {
    let device = WgpuDevice::default();
    BatchRunnerState::new(Backend::Wgpu, device)
}

#[cfg(feature = "cubecl-cpu")]
fn build_cpu_runner() -> BatchRunnerState<CpuRuntime> {
    let device = CpuDevice::default();
    BatchRunnerState::new(Backend::Cpu, device)
}

fn process_request<R: Runtime>(
    backend: Backend,
    runner: &mut Option<BatchRunnerState<R>>,
    request: BatchRequest,
    init: fn() -> BatchRunnerState<R>,
) -> BatchResult {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if runner.is_none() {
            *runner = Some(init());
        }
        runner
            .as_mut()
            .expect("runner missing")
            .run(request.batch_size, request.seed)
    }));
    match result {
        Ok(batch) => batch,
        Err(_) => {
            *runner = None;
            BatchResult::failed(
                backend,
                format!("cubecl failed to initialize {} backend", backend.label()),
            )
        }
    }
}

fn wgpu_worker_loop(
    requests: mpsc::Receiver<BatchRequest>,
    results: mpsc::Sender<BatchResult>,
) {
    let mut runner: Option<BatchRunnerState<WgpuRuntime>> = None;
    for request in requests {
        let batch = process_request(Backend::Wgpu, &mut runner, request, build_wgpu_runner);
        let _ = results.send(batch);
    }
}

#[cfg(feature = "cubecl-cpu")]
fn cpu_worker_loop(
    requests: mpsc::Receiver<BatchRequest>,
    results: mpsc::Sender<BatchResult>,
) {
    let mut runner: Option<BatchRunnerState<CpuRuntime>> = None;
    for request in requests {
        let batch = process_request(Backend::Cpu, &mut runner, request, build_cpu_runner);
        let _ = results.send(batch);
    }
}

#[derive(Clone, Debug)]
struct BatchHistory {
    run: usize,
    batch_cost: u32,
    best_cost: u32,
    elapsed_ms: u128,
    backend: Backend,
}

#[derive(Clone, Debug)]
struct BestResult {
    cost: u32,
    order: Vec<usize>,
}

impl BestResult {
    fn new() -> Self {
        Self {
            cost: u32::MAX,
            order: Vec::new(),
        }
    }
}

struct MinlaState {
    runner_wgpu: BatchRunner,
    #[cfg(feature = "cubecl-cpu")]
    runner_cpu: BatchRunner,
    last: BatchResult,
    best: BestResult,
    history: Vec<BatchHistory>,
    auto_run: bool,
    total_samples: usize,
    runs: usize,
}

impl MinlaState {
    fn new() -> Self {
        Self {
            runner_wgpu: BatchRunner::new_wgpu(),
            #[cfg(feature = "cubecl-cpu")]
            runner_cpu: BatchRunner::new_cpu(),
            last: BatchResult::idle(),
            best: BestResult::new(),
            history: Vec::new(),
            auto_run: false,
            total_samples: 0,
            runs: 0,
        }
    }

    fn poll_runners(&mut self) {
        if let Some(batch) = self.runner_wgpu.poll() {
            self.record_batch(&batch);
            self.last = batch;
        }
        #[cfg(feature = "cubecl-cpu")]
        if let Some(batch) = self.runner_cpu.poll() {
            self.record_batch(&batch);
            self.last = batch;
        }
    }

    fn runner(&self, backend: Backend) -> Option<&BatchRunner> {
        match backend {
            Backend::Wgpu => Some(&self.runner_wgpu),
            Backend::Cpu => {
                #[cfg(feature = "cubecl-cpu")]
                {
                    Some(&self.runner_cpu)
                }
                #[cfg(not(feature = "cubecl-cpu"))]
                {
                    None
                }
            }
        }
    }

    fn runner_mut(&mut self, backend: Backend) -> Option<&mut BatchRunner> {
        match backend {
            Backend::Wgpu => Some(&mut self.runner_wgpu),
            Backend::Cpu => {
                #[cfg(feature = "cubecl-cpu")]
                {
                    Some(&mut self.runner_cpu)
                }
                #[cfg(not(feature = "cubecl-cpu"))]
                {
                    None
                }
            }
        }
    }

    fn record_batch(&mut self, batch: &BatchResult) {
        if batch.error.is_some() || batch.batch_size == 0 {
            return;
        }
        self.runs += 1;
        self.total_samples += batch.batch_size;
        if batch.best_cost < self.best.cost {
            self.best.cost = batch.best_cost;
            self.best.order = batch.best_order.clone();
        }
        self.history.push(BatchHistory {
            run: self.runs,
            batch_cost: batch.best_cost,
            best_cost: self.best.cost,
            elapsed_ms: batch.elapsed_ms,
            backend: batch.backend,
        });
        if self.history.len() > MAX_HISTORY {
            let drop = self.history.len() - MAX_HISTORY;
            self.history.drain(0..drop);
        }
    }
}

#[derive(Clone, Debug)]
struct Config {
    batch_size: u32,
    seed: u64,
    backend: Backend,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            seed: 42,
            backend: Backend::Wgpu,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LcgRng {
    state: u64,
}

impl LcgRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        (self.state >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        let value = self.next_u32() as f32;
        value / (u32::MAX as f32 + 1.0)
    }

    fn gen_range(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        (self.next_u32() as usize) % upper
    }
}

#[derive(Clone, Debug)]
struct AnnealConfig {
    steps_per_batch: u32,
    initial_temp: f32,
    cooling: f32,
    seed: u64,
}

impl Default for AnnealConfig {
    fn default() -> Self {
        Self {
            steps_per_batch: DEFAULT_ANNEAL_STEPS,
            initial_temp: DEFAULT_ANNEAL_TEMP,
            cooling: DEFAULT_ANNEAL_COOLING,
            seed: 7,
        }
    }
}

#[derive(Clone, Debug)]
struct AnnealBatch {
    current_order: Vec<usize>,
    current_cost: u32,
    best_order: Vec<usize>,
    best_cost: u32,
    temperature: f32,
    rng: LcgRng,
    last_steps: usize,
    last_accepted: usize,
    last_elapsed_ms: u128,
}

impl AnnealBatch {
    fn from_config(config: &AnnealConfig) -> Self {
        let current_order: Vec<usize> = (0..NODE_COUNT).collect();
        let current_cost = cost_cpu(&current_order);
        Self {
            current_order: current_order.clone(),
            current_cost,
            best_order: current_order,
            best_cost: current_cost,
            temperature: config.initial_temp.max(MIN_ANNEAL_TEMP),
            rng: LcgRng::new(config.seed),
            last_steps: 0,
            last_accepted: 0,
            last_elapsed_ms: 0,
        }
    }
}

#[derive(Clone, Debug)]
struct AnnealHistory {
    run: usize,
    current_cost: u32,
    best_cost: u32,
}

struct AnnealState {
    config: AnnealConfig,
    run: ComputedState<AnnealBatch>,
    history: Vec<AnnealHistory>,
    auto_run: bool,
    runs: usize,
    total_steps: usize,
    total_accepted: usize,
}

impl AnnealState {
    fn new() -> Self {
        let config = AnnealConfig::default();
        let batch = AnnealBatch::from_config(&config);
        Self {
            config,
            run: ComputedState::new(batch),
            history: Vec::new(),
            auto_run: false,
            runs: 0,
            total_steps: 0,
            total_accepted: 0,
        }
    }

    fn reset(&mut self) {
        let batch = AnnealBatch::from_config(&self.config);
        self.run.set(batch);
        self.history.clear();
        self.runs = 0;
        self.total_steps = 0;
        self.total_accepted = 0;
    }

    fn record_batch(&mut self, batch: &AnnealBatch) {
        if batch.last_steps == 0 {
            return;
        }
        self.runs += 1;
        self.total_steps += batch.last_steps;
        self.total_accepted += batch.last_accepted;
        self.history.push(AnnealHistory {
            run: self.runs,
            current_cost: batch.current_cost,
            best_cost: batch.best_cost,
        });
        if self.history.len() > MAX_HISTORY {
            let drop = self.history.len() - MAX_HISTORY;
            self.history.drain(0..drop);
        }
    }
}

fn anneal_batch(mut batch: AnnealBatch, steps: usize, cooling: f32) -> AnnealBatch {
    let start = std::time::Instant::now();
    let mut accepted = 0usize;
    let mut steps_done = 0usize;
    let cooling = cooling.clamp(0.90, 0.9999);
    let len = batch.current_order.len();

    if len >= 2 && steps > 0 {
        for _ in 0..steps {
            let i = batch.rng.gen_range(len);
            let mut j = batch.rng.gen_range(len - 1);
            if j >= i {
                j += 1;
            }

            batch.current_order.swap(i, j);
            let candidate_cost = cost_cpu(&batch.current_order);
            let delta = candidate_cost as f32 - batch.current_cost as f32;
            let accept = if delta <= 0.0 {
                true
            } else if batch.temperature <= MIN_ANNEAL_TEMP {
                false
            } else {
                let probability = (-delta / batch.temperature).exp();
                batch.rng.next_f32() < probability
            };

            if accept {
                batch.current_cost = candidate_cost;
                accepted += 1;
                if candidate_cost < batch.best_cost {
                    batch.best_cost = candidate_cost;
                    batch.best_order = batch.current_order.clone();
                }
            } else {
                batch.current_order.swap(i, j);
            }

            batch.temperature = (batch.temperature * cooling).max(MIN_ANNEAL_TEMP);
            steps_done += 1;
        }
    }

    batch.last_steps = steps_done;
    batch.last_accepted = accepted;
    batch.last_elapsed_ms = start.elapsed().as_millis();
    batch
}

#[cube(launch)]
fn minla_cost_kernel(
    edges: &Array<u32>,
    seed: u32,
    output: &mut Array<u32>,
    orders: &mut Array<u32>,
) {
    let candidate = ABSOLUTE_POS;
    let mut state = seed ^ candidate as u32;
    state = state * SEED_MIX;
    let mut order = Array::<u32>::new(NODE_COUNT);
    #[unroll]
    for index in 0..NODE_COUNT {
        order[index] = index as u32;
    }

    #[unroll]
    for i in 0..NODE_COUNT {
        state = state * LCG_A + LCG_C;
        let remaining = NODE_COUNT - i;
        let j = (state % remaining as u32) as usize + i;
        let tmp = order[i];
        order[i] = order[j];
        order[j] = tmp;
    }

    let mut positions = Array::<u32>::new(NODE_COUNT);
    #[unroll]
    for pos in 0..NODE_COUNT {
        let node = order[pos] as usize;
        positions[node] = pos as u32;
    }

    let mut cost = 0u32;
    #[unroll]
    for edge in 0..EDGE_COUNT {
        let edge_index = edge * 2;
        let u = edges[edge_index] as usize;
        let v = edges[edge_index + 1] as usize;
        let pu = positions[u];
        let pv = positions[v];
        let diff = if pu > pv { pu - pv } else { pv - pu };
        cost += diff;
    }

    output[candidate] = cost;

    let base = candidate * NODE_COUNT;
    #[unroll]
    for idx in 0..NODE_COUNT {
        orders[base + idx] = order[idx];
    }
}

fn edges_flat() -> Vec<u32> {
    let mut flat = Vec::with_capacity(EDGES_FLAT_LEN);
    for (u, v) in EDGES {
        flat.push(u as u32);
        flat.push(v as u32);
    }
    flat
}

fn seed_to_u32(seed: u64) -> u32 {
    let low = seed as u32;
    let high = (seed >> 32) as u32;
    low ^ high.wrapping_mul(SEED_MIX)
}

fn format_order(order: &[usize]) -> String {
    order
        .iter()
        .map(|&idx| NODE_NAMES.get(idx).copied().unwrap_or("unknown"))
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn cost_cpu(order: &[usize]) -> u32 {
    let mut positions = vec![0u32; NODE_COUNT];
    for (pos, &node) in order.iter().enumerate() {
        positions[node] = pos as u32;
    }

    let mut cost = 0u32;
    for (u, v) in EDGES {
        let pu = positions[u];
        let pv = positions[v];
        cost += if pu > pv { pu - pv } else { pv - pu };
    }
    cost
}

fn cost_from_u32_order(order: &[u32]) -> u32 {
    let mut positions = [0u32; NODE_COUNT];
    for (pos, &node) in order.iter().enumerate() {
        positions[node as usize] = pos as u32;
    }
    let mut cost = 0u32;
    for (u, v) in EDGES {
        let pu = positions[u];
        let pv = positions[v];
        cost += if pu > pv { pu - pv } else { pv - pu };
    }
    cost
}

fn best_order_cpu(batch_size: usize, seed32: u32) -> Option<(u32, Vec<usize>)> {
    if batch_size == 0 {
        return None;
    }
    let mut best_cost = u32::MAX;
    let mut best_order = [0u32; NODE_COUNT];
    for candidate in 0..batch_size {
        let mut state = seed32 ^ candidate as u32;
        state = state.wrapping_mul(SEED_MIX);
        let mut order = [0u32; NODE_COUNT];
        for index in 0..NODE_COUNT {
            order[index] = index as u32;
        }
        for i in 0..NODE_COUNT {
            state = state.wrapping_mul(LCG_A).wrapping_add(LCG_C);
            let remaining = NODE_COUNT - i;
            let j = (state % remaining as u32) as usize + i;
            order.swap(i, j);
        }
        let cost = cost_from_u32_order(&order);
        if cost < best_cost {
            best_cost = cost;
            best_order = order;
        }
    }
    if best_cost == u32::MAX {
        return None;
    }
    Some((
        best_cost,
        best_order.iter().map(|&val| val as usize).collect(),
    ))
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    nb.view(|ui| {
        md!(
            ui,
            r#"# MinLA with CubeCL
This notebook explores Minimum Linear Arrangement (MinLA) using CubeCL.

We keep the search deliberately simple:
- The backend generates random permutations from the seed.
- The backend (wgpu or cpu) scores a batch in parallel.
- The notebook keeps the best order seen so far.

A separate card runs a CPU simulated annealing loop for comparison.

The first run can be slow while CubeCL builds shaders. Enable the CPU backend
by building with `--features cubecl-cpu`."#
        );
    });

    let baseline_order: Vec<usize> = (0..NODE_COUNT).collect();
    let baseline_cost = cost_cpu(&baseline_order);

    let config = nb.state("config", Config::default(), move |ui, config| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label("Batch configuration");
            let cpu_enabled = cfg!(feature = "cubecl-cpu");
            if !cpu_enabled && config.backend == Backend::Cpu {
                config.backend = Backend::Wgpu;
            }
            ui.add(widgets::Slider::new(&mut config.batch_size, 64..=4096).text("batch"));
            ui.horizontal(|ui| {
                ui.label("backend");
                ui.radio_value(&mut config.backend, Backend::Wgpu, "wgpu");
                ui.add_enabled_ui(cpu_enabled, |ui| {
                    ui.radio_value(&mut config.backend, Backend::Cpu, "cpu");
                });
                if !cpu_enabled {
                    ui.label("(enable with --features cubecl-cpu)");
                }
            });
            ui.horizontal(|ui| {
                ui.label("seed");
                ui.add(DragValue::new(&mut config.seed).speed(1));
            });
            widgets::markdown(
                ui,
                &format!(
                    "Baseline cost (identity order): `{}` for `{}` nodes.",
                    baseline_cost,
                    NODE_COUNT
                ),
            );
        });
    });

    nb.state("minla", MinlaState::new(), move |ui, state| {
        let (batch_size, seed, backend) = {
            let config = config.read(ui);
            (
                config.batch_size.max(1) as usize,
                config.seed,
                config.backend,
            )
        };

        state.poll_runners();

        let running = state
            .runner(backend)
            .map(|runner| runner.is_running())
            .unwrap_or(false);
        let mut spawn_requested = false;
        let mut bump_seed = false;
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label("Search");
            ui.horizontal(|ui| {
                let toggle_label = if state.auto_run {
                    "Auto-run: on"
                } else {
                    "Auto-run: off"
                };
                ui.add(widgets::ToggleButton::new(&mut state.auto_run, toggle_label));
                if ui
                    .add_enabled(!running, widgets::Button::new("Run once"))
                    .clicked()
                {
                    spawn_requested = true;
                }
                if running {
                    ui.add(Spinner::new());
                    ui.ctx().request_repaint();
                }
            });

            let last = &state.last;
            if let Some(error) = &last.error {
                widgets::markdown(
                    ui,
                    &format!("Backend error ({}): {error}", last.backend.label()),
                );
            } else if last.batch_size > 0 {
                widgets::markdown(
                    ui,
                    &format!(
                        "Last batch ({}): `{}` candidates in `{}` ms. Best in batch: `{}`.",
                        last.backend.label(),
                        last.batch_size,
                        last.elapsed_ms,
                        last.best_cost
                    ),
                );
            } else {
                widgets::markdown(ui, "No batch yet. Click \"Run once\" to start.");
            }

            if state.best.cost != u32::MAX && !state.best.order.is_empty() {
                widgets::markdown(
                    ui,
                    &format!(
                        "Best overall: `{}` ({} runs, {} samples).",
                        state.best.cost, state.runs, state.total_samples
                    ),
                );
                ui.label(format_order(&state.best.order));
            }

            if !state.history.is_empty() {
                ui.separator();
                ui.label("Performance and improvement");
                ui.columns(2, |columns| {
                    let perf_wgpu: Vec<[f64; 2]> = state
                        .history
                        .iter()
                        .filter(|entry| entry.backend == Backend::Wgpu)
                        .map(|entry| [entry.run as f64, entry.elapsed_ms as f64])
                        .collect();
                    let perf_cpu: Vec<[f64; 2]> = state
                        .history
                        .iter()
                        .filter(|entry| entry.backend == Backend::Cpu)
                        .map(|entry| [entry.run as f64, entry.elapsed_ms as f64])
                        .collect();

                    Plot::new("minla_perf")
                        .height(160.0)
                        .legend(Legend::default())
                        .show(&mut columns[0], |plot_ui| {
                            if !perf_wgpu.is_empty() {
                                plot_ui.line(Line::new("wgpu", PlotPoints::from(perf_wgpu)));
                            }
                            if !perf_cpu.is_empty() {
                                plot_ui.line(Line::new("cpu", PlotPoints::from(perf_cpu)));
                            }
                        });

                    let best_points: Vec<[f64; 2]> = state
                        .history
                        .iter()
                        .map(|entry| [entry.run as f64, entry.best_cost as f64])
                        .collect();
                    let batch_points: Vec<[f64; 2]> = state
                        .history
                        .iter()
                        .map(|entry| [entry.run as f64, entry.batch_cost as f64])
                        .collect();

                    Plot::new("minla_cost")
                        .height(160.0)
                        .legend(Legend::default())
                        .show(&mut columns[1], |plot_ui| {
                            if !best_points.is_empty() {
                                plot_ui.line(Line::new("best", PlotPoints::from(best_points)));
                            }
                            if !batch_points.is_empty() {
                                plot_ui.line(Line::new("batch", PlotPoints::from(batch_points)));
                            }
                        });
                });
            }
        });

        if !running && state.auto_run {
            spawn_requested = true;
        }
        if spawn_requested && !running {
            bump_seed = true;
            match state.runner_mut(backend) {
                Some(runner) => {
                    if let Err(error) = runner.spawn(BatchRequest { batch_size, seed }) {
                        state.last = error;
                    }
                }
                None => {
                    state.last = BatchResult::failed(
                        backend,
                        "cpu backend disabled; build with --features cubecl-cpu",
                    );
                }
            }
            ui.ctx().request_repaint();
        }
        if bump_seed {
            let mut config = config.read_mut(ui);
            config.seed = config.seed.wrapping_add(1);
        }
    });

    nb.state("anneal", AnnealState::new(), move |ui, state| {
        if state.run.poll() {
            let batch = state.run.value().clone();
            state.record_batch(&batch);
        }

        let running = state.run.is_running();
        let mut spawn_requested = false;
        let mut reset_requested = false;

        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label("Simulated annealing (CPU)");
            ui.horizontal(|ui| {
                ui.label("steps");
                ui.add(
                    widgets::Slider::new(&mut state.config.steps_per_batch, 100..=20_000)
                        .text("per batch"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("initial temp");
                ui.add(
                    widgets::Slider::new(&mut state.config.initial_temp, 0.1..=30.0)
                        .text("start"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("cooling");
                ui.add(
                    widgets::Slider::new(&mut state.config.cooling, 0.90..=0.9999)
                        .text("per step"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("seed");
                ui.add(DragValue::new(&mut state.config.seed).speed(1));
            });
            ui.label("Reset applies the seed and initial temperature.");

            ui.horizontal(|ui| {
                let toggle_label = if state.auto_run {
                    "Auto-run: on"
                } else {
                    "Auto-run: off"
                };
                ui.add(widgets::ToggleButton::new(&mut state.auto_run, toggle_label));
                if ui
                    .add_enabled(!running, widgets::Button::new("Run once"))
                    .clicked()
                {
                    spawn_requested = true;
                }
                if ui
                    .add_enabled(!running, widgets::Button::new("Reset"))
                    .clicked()
                {
                    reset_requested = true;
                }
                if running {
                    ui.add(Spinner::new());
                    ui.ctx().request_repaint();
                }
            });

            let batch = state.run.value();
            if batch.last_steps > 0 {
                let acceptance =
                    batch.last_accepted as f32 / batch.last_steps as f32 * 100.0;
                widgets::markdown(
                    ui,
                    &format!(
                        "Last batch: `{}` steps in `{}` ms, `{}` accepted ({:.1}%), temp `{:.3}`.",
                        batch.last_steps,
                        batch.last_elapsed_ms,
                        batch.last_accepted,
                        acceptance,
                        batch.temperature
                    ),
                );
            } else {
                widgets::markdown(ui, "No annealing run yet.");
            }

            if !batch.current_order.is_empty() {
                widgets::markdown(
                    ui,
                    &format!("Current cost: `{}`.", batch.current_cost),
                );
                ui.label(format_order(&batch.current_order));
            }
            if !batch.best_order.is_empty() {
                widgets::markdown(ui, &format!("Best cost: `{}`.", batch.best_cost));
                ui.label(format_order(&batch.best_order));
            }

            if !state.history.is_empty() {
                let current_points: Vec<[f64; 2]> = state
                    .history
                    .iter()
                    .map(|entry| [entry.run as f64, entry.current_cost as f64])
                    .collect();
                let best_points: Vec<[f64; 2]> = state
                    .history
                    .iter()
                    .map(|entry| [entry.run as f64, entry.best_cost as f64])
                    .collect();

                Plot::new("anneal_cost")
                    .height(160.0)
                    .legend(Legend::default())
                    .show(ui, |plot_ui| {
                        if !current_points.is_empty() {
                            plot_ui.line(Line::new(
                                "current",
                                PlotPoints::from(current_points),
                            ));
                        }
                        if !best_points.is_empty() {
                            plot_ui.line(Line::new("best", PlotPoints::from(best_points)));
                        }
                    });
            }
        });

        if reset_requested && !running {
            state.reset();
        }
        if !running && state.auto_run {
            spawn_requested = true;
        }
        if spawn_requested && !running {
            let steps = state.config.steps_per_batch.max(1) as usize;
            let cooling = state.config.cooling;
            let batch = state.run.value().clone();
            state
                .run
                .spawn(move || anneal_batch(batch, steps, cooling));
            ui.ctx().request_repaint();
        }
    });
}
