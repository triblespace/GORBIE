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
use egui::{DragValue, Spinner};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use std::collections::{HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
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

const PRESET_EDGES: [(usize, usize); 14] = [
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

const DEFAULT_NODE_COUNT: usize = NODE_NAMES.len();
const DEFAULT_EDGE_COUNT: usize = PRESET_EDGES.len();
const MAX_NODE_COUNT: usize = 4096;
const MAX_EDGE_COUNT: usize = 65_536;
const SEED_MIX: u32 = 0x9E37_79B9;
const LCG_A: u32 = 1_664_525;
const LCG_C: u32 = 1_013_904_223;

const MIN_BATCH_SIZE: u32 = 1;
const DEFAULT_BATCH_SIZE: u32 = MIN_BATCH_SIZE;
const MAX_BATCH_SIZE: u32 = 4096;
const DEFAULT_ANNEAL_STEPS: u32 = 1000;
const MIN_ANNEAL_STEPS: u32 = 1;
const MAX_ANNEAL_STEPS: u32 = 20_000;
const SA_ADAPT_INTERVAL: u32 = 32;
const INV_U32_MAX_PLUS1: f32 = 1.0 / 4_294_967_296.0;
const DEFAULT_ANNEAL_TEMP: f32 = 8.0;
const DEFAULT_ANNEAL_COOLING: f32 = 0.995;
const MIN_ANNEAL_TEMP: f32 = 0.001;
const MAX_HISTORY: usize = 200;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum GraphPattern {
    Preset,
    Line,
    Ring,
    Star,
    Random,
}

impl GraphPattern {
    fn label(self) -> &'static str {
        match self {
            GraphPattern::Preset => "Preset",
            GraphPattern::Line => "Line",
            GraphPattern::Ring => "Ring",
            GraphPattern::Star => "Star",
            GraphPattern::Random => "Random",
        }
    }

    fn all() -> [GraphPattern; 5] {
        [
            GraphPattern::Preset,
            GraphPattern::Line,
            GraphPattern::Ring,
            GraphPattern::Star,
            GraphPattern::Random,
        ]
    }
}

#[derive(Clone, Debug)]
struct GraphConfig {
    pattern: GraphPattern,
    node_count: usize,
    edge_count: usize,
    seed: u64,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            pattern: GraphPattern::Preset,
            node_count: DEFAULT_NODE_COUNT,
            edge_count: DEFAULT_EDGE_COUNT,
            seed: 1,
        }
    }
}

#[derive(Clone, Debug)]
struct GraphData {
    node_count: usize,
    edge_count: usize,
    edges: Vec<(usize, usize)>,
    edges_flat: Vec<u32>,
    adj_offsets: Vec<u32>,
    adj_list: Vec<u32>,
    id: u64,
}

#[derive(Clone, Debug)]
struct BatchResult {
    graph_id: u64,
    best_cost: u32,
    best_order: Vec<usize>,
    batch_size: usize,
    elapsed_ms: u128,
    error: Option<String>,
}

impl BatchResult {
    fn idle() -> Self {
        Self {
            graph_id: 0,
            best_cost: u32::MAX,
            best_order: Vec::new(),
            batch_size: 0,
            elapsed_ms: 0,
            error: None,
        }
    }

    fn ok(
        graph_id: u64,
        best_cost: u32,
        best_order: Vec<usize>,
        batch_size: usize,
        elapsed_ms: u128,
    ) -> Self {
        Self {
            graph_id,
            best_cost,
            best_order,
            batch_size,
            elapsed_ms,
            error: None,
        }
    }

    fn failed(message: impl Into<String>) -> Self {
        Self {
            graph_id: 0,
            best_cost: u32::MAX,
            best_order: Vec::new(),
            batch_size: 0,
            elapsed_ms: 0,
            error: Some(message.into()),
        }
    }
}

#[derive(Clone, Debug)]
struct BatchRequest {
    batch_size: usize,
    seed: u64,
    graph: GraphData,
}

#[derive(Clone, Debug)]
struct AnnealResult {
    graph_id: u64,
    best_cost: u32,
    batch_size: usize,
    steps: u32,
    elapsed_ms: u128,
    error: Option<String>,
}

impl AnnealResult {
    fn idle() -> Self {
        Self {
            graph_id: 0,
            best_cost: u32::MAX,
            batch_size: 0,
            steps: 0,
            elapsed_ms: 0,
            error: None,
        }
    }

    fn ok(
        graph_id: u64,
        best_cost: u32,
        batch_size: usize,
        steps: u32,
        elapsed_ms: u128,
    ) -> Self {
        Self {
            graph_id,
            best_cost,
            batch_size,
            steps,
            elapsed_ms,
            error: None,
        }
    }

    fn failed(message: impl Into<String>) -> Self {
        Self {
            graph_id: 0,
            best_cost: u32::MAX,
            batch_size: 0,
            steps: 0,
            elapsed_ms: 0,
            error: Some(message.into()),
        }
    }
}

#[derive(Clone, Debug)]
struct AnnealRequest {
    batch_size: usize,
    steps: u32,
    seed: u64,
    reset: bool,
    initial_temp: f32,
    cooling: f32,
    reheat_enabled: bool,
    reheat_temp: f32,
    reheat_plateau_steps: u32,
    adaptive_cooling: bool,
    target_acceptance: f32,
    cooling_adjust: f32,
    graph: GraphData,
}

struct BatchRunner {
    sender: mpsc::Sender<BatchRequest>,
    receiver: Mutex<mpsc::Receiver<BatchResult>>,
    in_flight: bool,
}

impl BatchRunner {
    fn new_wgpu() -> Self {
        spawn_worker(wgpu_worker_loop)
    }

    fn spawn(&mut self, request: BatchRequest) -> Result<(), BatchResult> {
        if self.in_flight {
            return Ok(());
        }
        if self.sender.send(request).is_err() {
            return Err(BatchResult::failed("worker disconnected"));
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
                Some(BatchResult::failed("worker disconnected"))
            }
        }
    }

    fn is_running(&self) -> bool {
        self.in_flight
    }
}

struct AnnealRunner {
    sender: mpsc::Sender<AnnealRequest>,
    receiver: Mutex<mpsc::Receiver<AnnealResult>>,
    in_flight: bool,
}

impl AnnealRunner {
    fn new_wgpu() -> Self {
        spawn_anneal_worker(wgpu_anneal_worker_loop)
    }

    fn spawn(&mut self, request: AnnealRequest) -> Result<(), AnnealResult> {
        if self.in_flight {
            return Ok(());
        }
        if self.sender.send(request).is_err() {
            return Err(AnnealResult::failed("worker disconnected"));
        }
        self.in_flight = true;
        Ok(())
    }

    fn poll(&mut self) -> Option<AnnealResult> {
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
                Some(AnnealResult::failed("worker disconnected"))
            }
        }
    }

    fn is_running(&self) -> bool {
        self.in_flight
    }
}

struct BatchRunnerState<R: Runtime> {
    _device: R::Device,
    client: ComputeClient<R>,
    edges_handle: Handle,
    graph_id: u64,
    node_count: usize,
    edge_count: usize,
}

impl<R: Runtime> BatchRunnerState<R> {
    fn new(device: R::Device, graph: &GraphData) -> Self {
        let client = R::client(&device);
        let edges_handle = client.create_from_slice(u32::as_bytes(&graph.edges_flat));
        Self {
            _device: device,
            client,
            edges_handle,
            graph_id: graph.id,
            node_count: graph.node_count,
            edge_count: graph.edge_count,
        }
    }

    fn run(&mut self, graph: &GraphData, batch_size: usize, seed: u64) -> BatchResult {
        if batch_size == 0 {
            return BatchResult::failed("batch size must be > 0");
        }
        if graph.node_count == 0 {
            return BatchResult::failed("graph must have at least one node");
        }

        self.sync_graph(graph);

        let start = std::time::Instant::now();
        let seed32 = seed_to_u32(seed);

        let output_handle = self.client.empty(batch_size * std::mem::size_of::<u32>());
        let orders_len = match batch_size.checked_mul(self.node_count) {
            Some(len) if len > 0 => len,
            _ => return BatchResult::failed("batch too large for graph size"),
        };
        let orders_handle = self
            .client
            .empty(orders_len * std::mem::size_of::<u32>());
        let positions_handle = self
            .client
            .empty(orders_len * std::mem::size_of::<u32>());

        unsafe {
            minla_cost_kernel::launch::<R>(
                &self.client,
                CubeCount::new_1d(batch_size as u32),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts::<u32>(&self.edges_handle, graph.edges_flat.len(), 1),
                ScalarArg::new(self.node_count as u32),
                ScalarArg::new(self.edge_count as u32),
                ScalarArg::new(seed32),
                ArrayArg::from_raw_parts::<u32>(&output_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(&orders_handle, orders_len, 1),
                ArrayArg::from_raw_parts::<u32>(&positions_handle, orders_len, 1),
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

        let order_stride = self.node_count * std::mem::size_of::<u32>();
        let offset_start = (best_idx * order_stride) as u64;
        let offset_end = (batch_size.saturating_sub(best_idx + 1) * order_stride) as u64;
        let order_handle = orders_handle
            .clone()
            .offset_start(offset_start)
            .offset_end(offset_end);
        let order_bytes = self.client.read_one(order_handle);
        let order = u32::from_bytes(&order_bytes);
        let best_order = order
            .iter()
            .take(self.node_count)
            .map(|&val| val as usize)
            .collect();

        BatchResult::ok(
            graph.id,
            best_cost,
            best_order,
            batch_size,
            start.elapsed().as_millis(),
        )
    }

    fn sync_graph(&mut self, graph: &GraphData) {
        if self.graph_id == graph.id {
            return;
        }
        self.edges_handle = self
            .client
            .create_from_slice(u32::as_bytes(&graph.edges_flat));
        self.graph_id = graph.id;
        self.node_count = graph.node_count;
        self.edge_count = graph.edge_count;
    }
}

struct AnnealRunnerState<R: Runtime> {
    _device: R::Device,
    client: ComputeClient<R>,
    edges_handle: Handle,
    adj_offsets_handle: Handle,
    adj_list_handle: Handle,
    graph_id: u64,
    node_count: usize,
    edge_count: usize,
    batch_size: usize,
    orders_handle: Option<Handle>,
    positions_handle: Option<Handle>,
    best_orders_handle: Option<Handle>,
    current_costs_handle: Option<Handle>,
    best_costs_handle: Option<Handle>,
    temperatures_handle: Option<Handle>,
    cooling_handle: Option<Handle>,
    rng_states_handle: Option<Handle>,
    stagnant_steps_handle: Option<Handle>,
}

impl<R: Runtime> AnnealRunnerState<R> {
    fn new(device: R::Device, graph: &GraphData) -> Self {
        let client = R::client(&device);
        let edges_handle = client.create_from_slice(u32::as_bytes(&graph.edges_flat));
        let adj_offsets_handle = client.create_from_slice(u32::as_bytes(&graph.adj_offsets));
        let adj_list_handle = client.create_from_slice(u32::as_bytes(&graph.adj_list));
        Self {
            _device: device,
            client,
            edges_handle,
            adj_offsets_handle,
            adj_list_handle,
            graph_id: graph.id,
            node_count: graph.node_count,
            edge_count: graph.edge_count,
            batch_size: 0,
            orders_handle: None,
            positions_handle: None,
            best_orders_handle: None,
            current_costs_handle: None,
            best_costs_handle: None,
            temperatures_handle: None,
            cooling_handle: None,
            rng_states_handle: None,
            stagnant_steps_handle: None,
        }
    }

    fn run(&mut self, request: &AnnealRequest) -> AnnealResult {
        if request.batch_size == 0 {
            return AnnealResult::failed("batch size must be > 0");
        }
        if request.graph.node_count == 0 {
            return AnnealResult::failed("graph must have at least one node");
        }
        if let Err(error) = self.ensure_state(request) {
            return error;
        }

        let start = std::time::Instant::now();
        let steps = request.steps.max(1);
        let reheat_temp = request.reheat_temp;
        let target_acceptance = request.target_acceptance.clamp(0.05, 0.95);
        let cooling_adjust = request.cooling_adjust.clamp(0.0001, 0.05);
        let reheat_plateau_steps = request.reheat_plateau_steps;

        let orders_handle = self.orders_handle.as_ref().expect("orders handle");
        let positions_handle = self.positions_handle.as_ref().expect("positions handle");
        let best_orders_handle = self.best_orders_handle.as_ref().expect("best orders handle");
        let current_costs_handle =
            self.current_costs_handle.as_ref().expect("current costs handle");
        let best_costs_handle = self.best_costs_handle.as_ref().expect("best costs handle");
        let temperatures_handle =
            self.temperatures_handle.as_ref().expect("temperatures handle");
        let cooling_handle = self.cooling_handle.as_ref().expect("cooling handle");
        let rng_states_handle = self.rng_states_handle.as_ref().expect("rng states handle");
        let stagnant_steps_handle =
            self.stagnant_steps_handle.as_ref().expect("stagnant steps handle");
        let orders_len = request.batch_size * self.node_count;

        unsafe {
            minla_sa_kernel::launch::<R>(
                &self.client,
                CubeCount::new_1d(request.batch_size as u32),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts::<u32>(
                    &self.adj_offsets_handle,
                    request.graph.adj_offsets.len(),
                    1,
                ),
                ArrayArg::from_raw_parts::<u32>(
                    &self.adj_list_handle,
                    request.graph.adj_list.len(),
                    1,
                ),
                ScalarArg::new(self.node_count as u32),
                ScalarArg::new(steps),
                ScalarArg::new(if request.reheat_enabled { 1u32 } else { 0u32 }),
                ScalarArg::new(reheat_temp),
                ScalarArg::new(reheat_plateau_steps),
                ScalarArg::new(if request.adaptive_cooling { 1u32 } else { 0u32 }),
                ScalarArg::new(target_acceptance),
                ScalarArg::new(cooling_adjust),
                ArrayArg::from_raw_parts::<u32>(orders_handle, orders_len, 1),
                ArrayArg::from_raw_parts::<u32>(positions_handle, orders_len, 1),
                ArrayArg::from_raw_parts::<u32>(best_orders_handle, orders_len, 1),
                ArrayArg::from_raw_parts::<u32>(current_costs_handle, request.batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(best_costs_handle, request.batch_size, 1),
                ArrayArg::from_raw_parts::<f32>(temperatures_handle, request.batch_size, 1),
                ArrayArg::from_raw_parts::<f32>(cooling_handle, request.batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(rng_states_handle, request.batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(stagnant_steps_handle, request.batch_size, 1),
            )
            .expect("minla SA kernel launch");
        }

        let costs_bytes = self.client.read_one(best_costs_handle.clone());
        let costs = u32::from_bytes(&costs_bytes);

        let mut best_cost = u32::MAX;
        for cost in costs.iter() {
            if *cost < best_cost {
                best_cost = *cost;
            }
        }

        AnnealResult::ok(
            request.graph.id,
            best_cost,
            request.batch_size,
            steps,
            start.elapsed().as_millis(),
        )
    }

    fn sync_graph(&mut self, graph: &GraphData) -> bool {
        if self.graph_id == graph.id {
            return false;
        }
        self.edges_handle = self
            .client
            .create_from_slice(u32::as_bytes(&graph.edges_flat));
        self.adj_offsets_handle = self
            .client
            .create_from_slice(u32::as_bytes(&graph.adj_offsets));
        self.adj_list_handle = self
            .client
            .create_from_slice(u32::as_bytes(&graph.adj_list));
        self.graph_id = graph.id;
        self.node_count = graph.node_count;
        self.edge_count = graph.edge_count;
        true
    }

    fn ensure_state(&mut self, request: &AnnealRequest) -> Result<(), AnnealResult> {
        let graph_changed = self.sync_graph(&request.graph);
        let needs_init = request.reset
            || graph_changed
            || self.batch_size != request.batch_size
            || self.orders_handle.is_none();

        if !needs_init {
            return Ok(());
        }

        let batch_size = request.batch_size;
        let order_len = match batch_size.checked_mul(self.node_count) {
            Some(len) if len > 0 => len,
            _ => return Err(AnnealResult::failed("batch too large for graph size")),
        };
        let bytes_u32 = std::mem::size_of::<u32>();
        let bytes_f32 = std::mem::size_of::<f32>();
        let order_bytes = match order_len.checked_mul(bytes_u32) {
            Some(bytes) => bytes,
            None => return Err(AnnealResult::failed("order buffer too large")),
        };
        let batch_bytes_u32 = match batch_size.checked_mul(bytes_u32) {
            Some(bytes) => bytes,
            None => return Err(AnnealResult::failed("batch buffer too large")),
        };
        let batch_bytes_f32 = match batch_size.checked_mul(bytes_f32) {
            Some(bytes) => bytes,
            None => return Err(AnnealResult::failed("batch buffer too large")),
        };

        self.orders_handle = Some(self.client.empty(order_bytes));
        self.positions_handle = Some(self.client.empty(order_bytes));
        self.best_orders_handle = Some(self.client.empty(order_bytes));
        self.current_costs_handle = Some(self.client.empty(batch_bytes_u32));
        self.best_costs_handle = Some(self.client.empty(batch_bytes_u32));
        self.temperatures_handle = Some(self.client.empty(batch_bytes_f32));
        self.cooling_handle = Some(self.client.empty(batch_bytes_f32));
        self.rng_states_handle = Some(self.client.empty(batch_bytes_u32));
        self.stagnant_steps_handle = Some(self.client.empty(batch_bytes_u32));
        self.batch_size = batch_size;

        let seed32 = seed_to_u32(request.seed);
        let mut initial_temp = request.initial_temp;
        if initial_temp < MIN_ANNEAL_TEMP {
            initial_temp = MIN_ANNEAL_TEMP;
        }
        let mut cooling = request.cooling;
        if cooling < 0.90 {
            cooling = 0.90;
        } else if cooling > 0.9999 {
            cooling = 0.9999;
        }

        let orders_handle = self.orders_handle.as_ref().expect("orders handle");
        let positions_handle = self.positions_handle.as_ref().expect("positions handle");
        let best_orders_handle = self.best_orders_handle.as_ref().expect("best orders handle");
        let current_costs_handle = self
            .current_costs_handle
            .as_ref()
            .expect("current costs handle");
        let best_costs_handle = self.best_costs_handle.as_ref().expect("best costs handle");
        let temperatures_handle = self
            .temperatures_handle
            .as_ref()
            .expect("temperatures handle");
        let cooling_handle = self.cooling_handle.as_ref().expect("cooling handle");
        let rng_states_handle = self.rng_states_handle.as_ref().expect("rng states handle");
        let stagnant_steps_handle = self
            .stagnant_steps_handle
            .as_ref()
            .expect("stagnant steps handle");

        unsafe {
            minla_sa_init_kernel::launch::<R>(
                &self.client,
                CubeCount::new_1d(batch_size as u32),
                CubeDim::new_1d(1),
                ArrayArg::from_raw_parts::<u32>(
                    &self.edges_handle,
                    request.graph.edges_flat.len(),
                    1,
                ),
                ScalarArg::new(self.node_count as u32),
                ScalarArg::new(self.edge_count as u32),
                ScalarArg::new(seed32),
                ScalarArg::new(initial_temp),
                ScalarArg::new(cooling),
                ArrayArg::from_raw_parts::<u32>(orders_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(positions_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(best_orders_handle, order_len, 1),
                ArrayArg::from_raw_parts::<u32>(current_costs_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(best_costs_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<f32>(temperatures_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<f32>(cooling_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(rng_states_handle, batch_size, 1),
                ArrayArg::from_raw_parts::<u32>(stagnant_steps_handle, batch_size, 1),
            )
            .expect("minla SA init kernel launch");
        }

        Ok(())
    }
}

fn spawn_worker(
    worker: fn(mpsc::Receiver<BatchRequest>, mpsc::Sender<BatchResult>),
) -> BatchRunner {
    let (request_tx, request_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    std::thread::spawn(move || worker(request_rx, result_tx));
    BatchRunner {
        sender: request_tx,
        receiver: Mutex::new(result_rx),
        in_flight: false,
    }
}

fn spawn_anneal_worker(
    worker: fn(mpsc::Receiver<AnnealRequest>, mpsc::Sender<AnnealResult>),
) -> AnnealRunner {
    let (request_tx, request_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    std::thread::spawn(move || worker(request_rx, result_tx));
    AnnealRunner {
        sender: request_tx,
        receiver: Mutex::new(result_rx),
        in_flight: false,
    }
}

fn build_wgpu_runner(graph: &GraphData) -> BatchRunnerState<WgpuRuntime> {
    let device = WgpuDevice::default();
    BatchRunnerState::new(device, graph)
}

fn build_wgpu_anneal_runner(graph: &GraphData) -> AnnealRunnerState<WgpuRuntime> {
    let device = WgpuDevice::default();
    AnnealRunnerState::new(device, graph)
}

fn process_request<R: Runtime>(
    runner: &mut Option<BatchRunnerState<R>>,
    request: BatchRequest,
    init: fn(&GraphData) -> BatchRunnerState<R>,
) -> BatchResult {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if runner.is_none() {
            *runner = Some(init(&request.graph));
        }
        runner
            .as_mut()
            .expect("runner missing")
            .run(&request.graph, request.batch_size, request.seed)
    }));
    match result {
        Ok(batch) => batch,
        Err(_) => {
            *runner = None;
            BatchResult::failed("cubecl failed to initialize wgpu backend")
        }
    }
}

fn process_anneal_request<R: Runtime>(
    runner: &mut Option<AnnealRunnerState<R>>,
    request: AnnealRequest,
    init: fn(&GraphData) -> AnnealRunnerState<R>,
) -> AnnealResult {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if runner.is_none() {
            *runner = Some(init(&request.graph));
        }
        runner
            .as_mut()
            .expect("runner missing")
            .run(&request)
    }));
    match result {
        Ok(batch) => batch,
        Err(_) => {
            *runner = None;
            AnnealResult::failed("cubecl failed to initialize wgpu backend")
        }
    }
}

fn wgpu_worker_loop(
    requests: mpsc::Receiver<BatchRequest>,
    results: mpsc::Sender<BatchResult>,
) {
    let mut runner: Option<BatchRunnerState<WgpuRuntime>> = None;
    for request in requests {
        let batch = process_request(&mut runner, request, build_wgpu_runner);
        let _ = results.send(batch);
    }
}

fn wgpu_anneal_worker_loop(
    requests: mpsc::Receiver<AnnealRequest>,
    results: mpsc::Sender<AnnealResult>,
) {
    let mut runner: Option<AnnealRunnerState<WgpuRuntime>> = None;
    for request in requests {
        let batch = process_anneal_request(&mut runner, request, build_wgpu_anneal_runner);
        let _ = results.send(batch);
    }
}

#[derive(Clone, Debug)]
struct BatchHistory {
    run: usize,
    batch_cost: u32,
    best_cost: u32,
    elapsed_ms: u128,
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
    last: BatchResult,
    best: BestResult,
    history: Vec<BatchHistory>,
    auto_run: bool,
    total_samples: usize,
    runs: usize,
    graph_id: u64,
}

impl MinlaState {
    fn new() -> Self {
        Self {
            runner_wgpu: BatchRunner::new_wgpu(),
            last: BatchResult::idle(),
            best: BestResult::new(),
            history: Vec::new(),
            auto_run: false,
            total_samples: 0,
            runs: 0,
            graph_id: 0,
        }
    }

    fn poll_runners(&mut self) {
        if let Some(batch) = self.runner_wgpu.poll() {
            if batch.error.is_some() {
                self.last = batch;
                return;
            }
            if batch.graph_id == self.graph_id {
                self.record_batch(&batch);
                self.last = batch;
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
        });
        if self.history.len() > MAX_HISTORY {
            let drop = self.history.len() - MAX_HISTORY;
            self.history.drain(0..drop);
        }
    }

    fn sync_graph(&mut self, graph: &GraphData) {
        if self.graph_id == graph.id {
            return;
        }
        self.graph_id = graph.id;
        self.last = BatchResult::idle();
        self.best = BestResult::new();
        self.history.clear();
        self.runs = 0;
        self.total_samples = 0;
    }
}

#[derive(Clone, Debug)]
struct Config {
    batch_size: u32,
    seed: u64,
    graph: GraphConfig,
    graph_data: GraphData,
    difficulty: f32,
    use_difficulty: bool,
    auto_batch: bool,
    target_batch_ms: u32,
}

impl Default for Config {
    fn default() -> Self {
        let mut graph = GraphConfig::default();
        let difficulty = 0.35;
        let use_difficulty = true;
        if use_difficulty {
            apply_difficulty(&mut graph, difficulty);
        }
        normalize_graph_config(&mut graph);
        let graph_data = build_graph(&graph);
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            seed: 42,
            graph,
            graph_data,
            difficulty,
            use_difficulty,
            auto_batch: true,
            target_batch_ms: 60,
        }
    }
}

impl Config {
    fn sync_graph(&mut self) {
        if self.use_difficulty {
            apply_difficulty(&mut self.graph, self.difficulty);
        }
        normalize_graph_config(&mut self.graph);
        let id = graph_id(&self.graph);
        if self.graph_data.id != id {
            self.graph_data = build_graph(&self.graph);
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

    fn gen_range(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        (self.next_u32() as usize) % upper
    }
}

fn graph_id(config: &GraphConfig) -> u64 {
    let mut hasher = DefaultHasher::new();
    config.pattern.hash(&mut hasher);
    config.node_count.hash(&mut hasher);
    config.edge_count.hash(&mut hasher);
    config.seed.hash(&mut hasher);
    hasher.finish()
}

fn max_edges(node_count: usize) -> usize {
    node_count.saturating_sub(1) * node_count / 2
}

fn normalize_graph_config(config: &mut GraphConfig) {
    if config.pattern == GraphPattern::Preset {
        config.node_count = DEFAULT_NODE_COUNT;
        config.edge_count = DEFAULT_EDGE_COUNT;
        return;
    }
    config.node_count = config.node_count.clamp(2, MAX_NODE_COUNT);
    let max_edges = max_edges(config.node_count).min(MAX_EDGE_COUNT);
    config.edge_count = config.edge_count.min(max_edges);
}

fn base_edges_for_pattern(pattern: GraphPattern, node_count: usize) -> usize {
    match pattern {
        GraphPattern::Preset => PRESET_EDGES.len(),
        GraphPattern::Line => node_count.saturating_sub(1),
        GraphPattern::Ring => {
            if node_count > 2 {
                node_count
            } else {
                node_count.saturating_sub(1)
            }
        }
        GraphPattern::Star => node_count.saturating_sub(1),
        GraphPattern::Random => 0,
    }
}

fn apply_difficulty(config: &mut GraphConfig, difficulty: f32) {
    let d = difficulty.clamp(0.0, 1.0);
    let pattern = if d < 0.25 {
        GraphPattern::Line
    } else if d < 0.5 {
        GraphPattern::Ring
    } else if d < 0.75 {
        GraphPattern::Star
    } else {
        GraphPattern::Random
    };
    config.pattern = pattern;

    let min_nodes = 4usize;
    let max_nodes = MAX_NODE_COUNT;
    let nodes = min_nodes
        + ((max_nodes - min_nodes) as f32 * d.powf(1.4)).round() as usize;
    config.node_count = nodes.clamp(2, MAX_NODE_COUNT);

    let max_edges = max_edges(config.node_count).min(MAX_EDGE_COUNT);
    let density = 0.15 + 0.7 * d.powf(1.6);
    let target_edges = (max_edges as f32 * density).round() as usize;
    let base_edges = base_edges_for_pattern(pattern, config.node_count);
    config.edge_count = target_edges.max(base_edges).min(max_edges);
}

fn add_edge(
    edges: &mut Vec<(usize, usize)>,
    seen: &mut HashSet<(usize, usize)>,
    u: usize,
    v: usize,
) -> bool {
    if u == v {
        return false;
    }
    let (a, b) = if u < v { (u, v) } else { (v, u) };
    if seen.insert((a, b)) {
        edges.push((a, b));
        true
    } else {
        false
    }
}

fn build_graph(config: &GraphConfig) -> GraphData {
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    let node_count = config.node_count;

    match config.pattern {
        GraphPattern::Preset => {
            for &(u, v) in PRESET_EDGES.iter() {
                add_edge(&mut edges, &mut seen, u, v);
            }
        }
        GraphPattern::Line => {
            for i in 0..node_count.saturating_sub(1) {
                add_edge(&mut edges, &mut seen, i, i + 1);
            }
        }
        GraphPattern::Ring => {
            for i in 0..node_count.saturating_sub(1) {
                add_edge(&mut edges, &mut seen, i, i + 1);
            }
            if node_count > 2 {
                add_edge(&mut edges, &mut seen, 0, node_count - 1);
            }
        }
        GraphPattern::Star => {
            if node_count > 1 {
                for i in 1..node_count {
                    add_edge(&mut edges, &mut seen, 0, i);
                }
            }
        }
        GraphPattern::Random => {}
    }

    if edges.len() > config.edge_count {
        edges.truncate(config.edge_count);
    } else if edges.len() < config.edge_count && node_count > 1 {
        let max_edges = max_edges(node_count).min(MAX_EDGE_COUNT);
        let mut rng = LcgRng::new(config.seed ^ 0x9E37_79B9_7F4A_7C15);
        while edges.len() < config.edge_count && edges.len() < max_edges {
            let u = rng.gen_range(node_count);
            let mut v = rng.gen_range(node_count - 1);
            if v >= u {
                v += 1;
            }
            add_edge(&mut edges, &mut seen, u, v);
        }
    }

    let mut edges_flat = Vec::with_capacity(edges.len() * 2);
    for (u, v) in &edges {
        edges_flat.push(*u as u32);
        edges_flat.push(*v as u32);
    }

    let mut degrees = vec![0usize; node_count];
    for &(u, v) in &edges {
        degrees[u] += 1;
        degrees[v] += 1;
    }
    let mut adj_offsets = Vec::with_capacity(node_count + 1);
    adj_offsets.push(0u32);
    for i in 0..node_count {
        let next = adj_offsets[i] + degrees[i] as u32;
        adj_offsets.push(next);
    }
    let total_adj = *adj_offsets.last().unwrap_or(&0) as usize;
    let mut adj_list = vec![0u32; total_adj];
    let mut cursor: Vec<usize> = adj_offsets.iter().map(|&offset| offset as usize).collect();
    for &(u, v) in &edges {
        let idx = cursor[u];
        adj_list[idx] = v as u32;
        cursor[u] += 1;
        let idx = cursor[v];
        adj_list[idx] = u as u32;
        cursor[v] += 1;
    }

    let id = graph_id(config);
    GraphData {
        node_count,
        edge_count: edges.len(),
        edges,
        edges_flat,
        adj_offsets,
        adj_list,
        id,
    }
}


#[derive(Clone, Debug)]
struct AnnealConfig {
    steps_per_batch: u32,
    initial_temp: f32,
    cooling: f32,
    seed: u64,
    reheat_enabled: bool,
    reheat_temp: f32,
    reheat_plateau_batches: u32,
    adaptive_cooling: bool,
    target_acceptance: f32,
    cooling_adjust: f32,
}

impl Default for AnnealConfig {
    fn default() -> Self {
        Self {
            steps_per_batch: DEFAULT_ANNEAL_STEPS,
            initial_temp: DEFAULT_ANNEAL_TEMP,
            cooling: DEFAULT_ANNEAL_COOLING,
            seed: 7,
            reheat_enabled: true,
            reheat_temp: DEFAULT_ANNEAL_TEMP,
            reheat_plateau_batches: 12,
            adaptive_cooling: true,
            target_acceptance: 0.3,
            cooling_adjust: 0.002,
        }
    }
}

fn estimate_initial_temp(graph: &GraphData, seed: u64, target_acceptance: f32) -> f32 {
    let node_count = graph.node_count.max(2);
    let mut order: Vec<usize> = (0..node_count).collect();
    let base_cost = cost_cpu(&order, graph);
    let mut rng = LcgRng::new(seed ^ 0x9E37_79B9_7F4A_7C15);
    let sample_count = node_count.min(64).max(8);
    let mut sum_delta = 0u64;
    let mut count = 0u32;

    for _ in 0..sample_count {
        let i = rng.gen_range(node_count);
        let mut j = rng.gen_range(node_count - 1);
        if j >= i {
            j += 1;
        }
        order.swap(i, j);
        let candidate_cost = cost_cpu(&order, graph);
        order.swap(i, j);
        if candidate_cost > base_cost {
            sum_delta += (candidate_cost - base_cost) as u64;
            count += 1;
        }
    }

    let avg_delta = if count > 0 {
        sum_delta as f32 / count as f32
    } else {
        1.0
    };
    let target = target_acceptance.clamp(0.05, 0.95);
    let denom = -target.ln();
    let temp = if denom > 0.0 { avg_delta / denom } else { avg_delta };
    temp.clamp(0.1, 100_000.0)
}

fn auto_tune_anneal_config(config: &mut AnnealConfig, graph: &GraphData) {
    config.steps_per_batch = MIN_ANNEAL_STEPS;
    config.target_acceptance = 0.3;
    config.cooling_adjust = 0.002;
    config.adaptive_cooling = true;
    config.reheat_enabled = true;
    config.cooling = DEFAULT_ANNEAL_COOLING;
    config.seed = graph.id ^ 0xD1B5_4A32_D192_ED03;
    config.initial_temp = estimate_initial_temp(graph, config.seed, config.target_acceptance);
    config.reheat_temp = config.initial_temp;
    config.reheat_plateau_batches = 12;
}

fn adjust_batch_size(current: u32, elapsed_ms: u128, target_ms: u32) -> u32 {
    if elapsed_ms == 0 {
        return current.saturating_mul(2).clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE);
    }
    let target = target_ms.max(1) as f64;
    let elapsed = elapsed_ms as f64;
    let ratio = target / elapsed;
    if (0.9..=1.1).contains(&ratio) {
        return current.clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE);
    }
    let factor = ratio.clamp(0.5, 2.0);
    let next = (current as f64 * factor).round() as u32;
    next.clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE)
}

fn adjust_steps_per_batch(current: u32, elapsed_ms: u128, target_ms: u32) -> u32 {
    if elapsed_ms == 0 {
        return current.saturating_mul(2).clamp(MIN_ANNEAL_STEPS, MAX_ANNEAL_STEPS);
    }
    let target = target_ms.max(1) as f64;
    let elapsed = elapsed_ms as f64;
    let ratio = target / elapsed;
    if (0.9..=1.1).contains(&ratio) {
        return current.clamp(MIN_ANNEAL_STEPS, MAX_ANNEAL_STEPS);
    }
    let factor = ratio.clamp(0.5, 2.0);
    let next = (current as f64 * factor).round() as u32;
    next.clamp(MIN_ANNEAL_STEPS, MAX_ANNEAL_STEPS)
}

#[derive(Clone, Debug)]
struct AnnealHistory {
    run: usize,
    batch_cost: u32,
    best_cost: u32,
    elapsed_ms: u128,
}

struct AnnealState {
    config: AnnealConfig,
    runner: AnnealRunner,
    last: AnnealResult,
    best_cost: u32,
    history: Vec<AnnealHistory>,
    auto_run: bool,
    runs: usize,
    total_steps: usize,
    total_chains: usize,
    graph: GraphData,
    graph_id: u64,
    pending_reset: bool,
    force_reinit: bool,
}

impl AnnealState {
    fn new() -> Self {
        let mut config = AnnealConfig::default();
        let mut graph_config = GraphConfig::default();
        normalize_graph_config(&mut graph_config);
        let graph = build_graph(&graph_config);
        auto_tune_anneal_config(&mut config, &graph);
        Self {
            config,
            runner: AnnealRunner::new_wgpu(),
            last: AnnealResult::idle(),
            best_cost: u32::MAX,
            history: Vec::new(),
            auto_run: false,
            runs: 0,
            total_steps: 0,
            total_chains: 0,
            graph_id: graph.id,
            graph,
            pending_reset: false,
            force_reinit: true,
        }
    }

    fn reset(&mut self) {
        auto_tune_anneal_config(&mut self.config, &self.graph);
        self.last = AnnealResult::idle();
        self.best_cost = u32::MAX;
        self.history.clear();
        self.runs = 0;
        self.total_steps = 0;
        self.total_chains = 0;
        self.pending_reset = false;
        self.force_reinit = true;
    }

    fn sync_graph(&mut self, graph: &GraphData) {
        if self.graph_id == graph.id {
            return;
        }
        self.graph = graph.clone();
        self.graph_id = graph.id;
        self.pending_reset = true;
        self.last = AnnealResult::idle();
        self.best_cost = u32::MAX;
        self.history.clear();
        self.runs = 0;
        self.total_steps = 0;
        self.total_chains = 0;
        self.force_reinit = true;
    }

    fn maybe_reset(&mut self) {
        if self.pending_reset && !self.runner.is_running() {
            self.reset();
            self.pending_reset = false;
        }
    }

    fn poll_runner(&mut self, target_ms: u32) {
        if let Some(batch) = self.runner.poll() {
            if batch.error.is_some() {
                self.last = batch;
                return;
            }
            if batch.graph_id == self.graph_id {
                self.record_batch(&batch, target_ms);
                self.last = batch;
            }
        }
    }

    fn record_batch(&mut self, batch: &AnnealResult, target_ms: u32) {
        if batch.steps == 0 || batch.graph_id != self.graph_id {
            return;
        }
        self.runs += 1;
        self.total_steps += batch.steps as usize * batch.batch_size;
        self.total_chains += batch.batch_size;
        if batch.best_cost < self.best_cost {
            self.best_cost = batch.best_cost;
        }
        self.history.push(AnnealHistory {
            run: self.runs,
            batch_cost: batch.best_cost,
            best_cost: self.best_cost,
            elapsed_ms: batch.elapsed_ms,
        });
        if self.history.len() > MAX_HISTORY {
            let drop = self.history.len() - MAX_HISTORY;
            self.history.drain(0..drop);
        }
        if batch.elapsed_ms > 0 && batch.steps > 0 {
            self.config.steps_per_batch = adjust_steps_per_batch(
                batch.steps,
                batch.elapsed_ms,
                target_ms,
            );
        }
    }
}

#[cube(launch)]
fn minla_cost_kernel(
    edges: &Array<u32>,
    node_count: u32,
    edge_count: u32,
    seed: u32,
    output: &mut Array<u32>,
    orders: &mut Array<u32>,
    positions: &mut Array<u32>,
) {
    let candidate = ABSOLUTE_POS;
    let mut state = seed ^ candidate as u32;
    state = state * SEED_MIX;
    let node_count = node_count as usize;
    let edge_count = edge_count as usize;
    if node_count == 0 {
        output[candidate] = 0;
    } else {
        let base = candidate * node_count;
        for index in 0..node_count {
            orders[base + index] = index as u32;
        }

        if node_count > 1 {
            for i in 0..node_count {
                state = state * LCG_A + LCG_C;
                let remaining = node_count - i;
                let j = (state % remaining as u32) as usize + i;
                let left = base + i;
                let right = base + j;
                let tmp = orders[left];
                orders[left] = orders[right];
                orders[right] = tmp;
            }
        }

        for pos in 0..node_count {
            let node = orders[base + pos] as usize;
            positions[base + node] = pos as u32;
        }

        let mut cost = 0u32;
        for edge in 0..edge_count {
            let edge_index = edge * 2;
            let u = edges[edge_index] as usize;
            let v = edges[edge_index + 1] as usize;
            let pu = positions[base + u];
            let pv = positions[base + v];
            let diff = if pu > pv { pu - pv } else { pv - pu };
            cost += diff;
        }

        output[candidate] = cost;
    }
}

#[cube(launch)]
fn minla_sa_init_kernel(
    edges: &Array<u32>,
    node_count: u32,
    edge_count: u32,
    seed: u32,
    initial_temp: f32,
    cooling: f32,
    orders: &mut Array<u32>,
    positions: &mut Array<u32>,
    best_orders: &mut Array<u32>,
    current_costs: &mut Array<u32>,
    best_costs: &mut Array<u32>,
    temperatures: &mut Array<f32>,
    cooling_state: &mut Array<f32>,
    rng_states: &mut Array<u32>,
    stagnant_steps: &mut Array<u32>,
) {
    let candidate = ABSOLUTE_POS;
    let node_count = node_count as usize;
    let edge_count = edge_count as usize;
    let mut temp = initial_temp;
    if temp < MIN_ANNEAL_TEMP {
        temp = MIN_ANNEAL_TEMP;
    }
    let mut cooling = cooling;
    if cooling < 0.90 {
        cooling = 0.90;
    } else if cooling > 0.9999 {
        cooling = 0.9999;
    }

    if node_count == 0 {
        current_costs[candidate] = 0;
        best_costs[candidate] = 0;
        temperatures[candidate] = temp;
        cooling_state[candidate] = cooling;
        rng_states[candidate] = seed;
        stagnant_steps[candidate] = 0;
    } else {
        let mut state = seed ^ candidate as u32;
        state = state * SEED_MIX;
        let base = candidate * node_count;

        for index in 0..node_count {
            orders[base + index] = index as u32;
        }

        if node_count > 1 {
            for i in 0..node_count {
                state = state * LCG_A + LCG_C;
                let remaining = node_count - i;
                let j = (state % remaining as u32) as usize + i;
                let left = base + i;
                let right = base + j;
                let tmp = orders[left];
                orders[left] = orders[right];
                orders[right] = tmp;
            }
        }

        for pos in 0..node_count {
            let node = orders[base + pos] as usize;
            positions[base + node] = pos as u32;
        }

        let mut cost = 0u32;
        for edge in 0..edge_count {
            let edge_index = edge * 2;
            let u = edges[edge_index] as usize;
            let v = edges[edge_index + 1] as usize;
            let pu = positions[base + u];
            let pv = positions[base + v];
            let diff = if pu > pv { pu - pv } else { pv - pu };
            cost += diff;
        }

        current_costs[candidate] = cost;
        best_costs[candidate] = cost;
        temperatures[candidate] = temp;
        cooling_state[candidate] = cooling;
        rng_states[candidate] = state;
        stagnant_steps[candidate] = 0;

        for idx in 0..node_count {
            best_orders[base + idx] = orders[base + idx];
        }
    }
}

#[cube(launch)]
fn minla_sa_kernel(
    adj_offsets: &Array<u32>,
    adj_list: &Array<u32>,
    node_count: u32,
    steps: u32,
    reheat_enabled: u32,
    reheat_temp: f32,
    reheat_plateau_steps: u32,
    adaptive_cooling: u32,
    target_acceptance: f32,
    cooling_adjust: f32,
    orders: &mut Array<u32>,
    positions: &mut Array<u32>,
    best_orders: &mut Array<u32>,
    current_costs: &mut Array<u32>,
    best_costs: &mut Array<u32>,
    temperatures: &mut Array<f32>,
    cooling_state: &mut Array<f32>,
    rng_states: &mut Array<u32>,
    stagnant_steps: &mut Array<u32>,
) {
    let candidate = ABSOLUTE_POS;
    let node_count = node_count as usize;
    let steps = steps as usize;
    if node_count == 0 {
        current_costs[candidate] = 0;
        best_costs[candidate] = 0;
    } else {
        let base = candidate * node_count;
        let mut state = rng_states[candidate];
        let mut current_cost = current_costs[candidate];
        let mut best_cost = best_costs[candidate];
        let mut temperature = temperatures[candidate];
        if temperature < MIN_ANNEAL_TEMP {
            temperature = MIN_ANNEAL_TEMP;
        }
        let mut cooling = cooling_state[candidate];
        if cooling < 0.90 {
            cooling = 0.90;
        } else if cooling > 0.9999 {
            cooling = 0.9999;
        }

        let reheat_enabled = reheat_enabled != 0;
        let adaptive_cooling = adaptive_cooling != 0;
        let mut target_acceptance = target_acceptance;
        if target_acceptance < 0.05 {
            target_acceptance = 0.05;
        } else if target_acceptance > 0.95 {
            target_acceptance = 0.95;
        }
        let mut cooling_adjust = cooling_adjust;
        if cooling_adjust < 0.0001 {
            cooling_adjust = 0.0001;
        } else if cooling_adjust > 0.05 {
            cooling_adjust = 0.05;
        }
        let mut reheat_temp = reheat_temp;
        if reheat_temp < MIN_ANNEAL_TEMP {
            reheat_temp = MIN_ANNEAL_TEMP;
        }
        let mut interval_steps: u32 = 0;
        let mut interval_accepts: u32 = 0;
        let mut stagnant = stagnant_steps[candidate];

        if node_count > 1 && steps > 0 {
            for _ in 0..steps {
                state = state * LCG_A + LCG_C;
                let i = (state % node_count as u32) as usize;
                state = state * LCG_A + LCG_C;
                let mut j = (state % (node_count as u32 - 1)) as usize;
                if j >= i {
                    j += 1;
                }

                let left = base + i;
                let right = base + j;
                let node_i = orders[left];
                let node_j = orders[right];
                orders[left] = node_j;
                orders[right] = node_i;
                positions[base + node_j as usize] = i as u32;
                positions[base + node_i as usize] = j as u32;

                let node_i_usize = node_i as usize;
                let node_j_usize = node_j as usize;
                let pos_i = i as u32;
                let pos_j = j as u32;
                let mut delta_cost: i32 = 0;

                let start_i = adj_offsets[node_i_usize] as usize;
                let end_i = adj_offsets[node_i_usize + 1] as usize;
                for idx in start_i..end_i {
                    let neighbor = adj_list[idx] as usize;
                    if neighbor != node_j_usize {
                        let pos_n = positions[base + neighbor];
                        let old = if pos_i > pos_n { pos_i - pos_n } else { pos_n - pos_i };
                        let new = if pos_j > pos_n { pos_j - pos_n } else { pos_n - pos_j };
                        delta_cost += new as i32 - old as i32;
                    }
                }

                let start_j = adj_offsets[node_j_usize] as usize;
                let end_j = adj_offsets[node_j_usize + 1] as usize;
                for idx in start_j..end_j {
                    let neighbor = adj_list[idx] as usize;
                    if neighbor != node_i_usize {
                        let pos_n = positions[base + neighbor];
                        let old = if pos_j > pos_n { pos_j - pos_n } else { pos_n - pos_j };
                        let new = if pos_i > pos_n { pos_i - pos_n } else { pos_n - pos_i };
                        delta_cost += new as i32 - old as i32;
                    }
                }

                let mut candidate_cost = current_cost as i32 + delta_cost;
                if candidate_cost < 0 {
                    candidate_cost = 0;
                }
                let candidate_cost = candidate_cost as u32;

                let delta = candidate_cost as f32 - current_cost as f32;
                let mut accept = delta <= 0.0;
                if !accept && temperature > MIN_ANNEAL_TEMP {
                    let probability = (-delta / temperature).exp();
                    state = state * LCG_A + LCG_C;
                    let rand = state as f32 * INV_U32_MAX_PLUS1;
                    accept = rand < probability;
                }

                if accept {
                    current_cost = candidate_cost;
                    interval_accepts += 1;
                    if candidate_cost < best_cost {
                        best_cost = candidate_cost;
                        stagnant = 0;
                        for idx in 0..node_count {
                            best_orders[base + idx] = orders[base + idx];
                        }
                    } else if stagnant < u32::MAX {
                        stagnant += 1;
                    }
                } else {
                    orders[left] = node_i;
                    orders[right] = node_j;
                    positions[base + node_i as usize] = i as u32;
                    positions[base + node_j as usize] = j as u32;
                    if stagnant < u32::MAX {
                        stagnant += 1;
                    }
                }

                interval_steps += 1;
                temperature = temperature * cooling;
                if temperature < MIN_ANNEAL_TEMP {
                    temperature = MIN_ANNEAL_TEMP;
                }

                if adaptive_cooling && interval_steps >= SA_ADAPT_INTERVAL {
                    let acceptance = interval_accepts as f32 / interval_steps as f32;
                    if acceptance > target_acceptance + 0.05 {
                        cooling -= cooling_adjust;
                        if cooling < 0.90 {
                            cooling = 0.90;
                        }
                    } else if acceptance < target_acceptance - 0.05 {
                        cooling += cooling_adjust;
                        if cooling > 0.9999 {
                            cooling = 0.9999;
                        }
                    }
                    interval_steps = 0;
                    interval_accepts = 0;
                }

                if reheat_enabled {
                    let hit_plateau = reheat_plateau_steps > 0 && stagnant >= reheat_plateau_steps;
                    if temperature <= MIN_ANNEAL_TEMP || hit_plateau {
                        temperature = reheat_temp;
                        stagnant = 0;
                        for idx in 0..node_count {
                            orders[base + idx] = best_orders[base + idx];
                        }
                        for pos in 0..node_count {
                            let node = orders[base + pos] as usize;
                            positions[base + node] = pos as u32;
                        }
                        current_cost = best_cost;
                    }
                }
            }
        }

        current_costs[candidate] = current_cost;
        best_costs[candidate] = best_cost;
        temperatures[candidate] = temperature;
        cooling_state[candidate] = cooling;
        rng_states[candidate] = state;
        stagnant_steps[candidate] = stagnant;
    }
}

fn seed_to_u32(seed: u64) -> u32 {
    let low = seed as u32;
    let high = (seed >> 32) as u32;
    low ^ high.wrapping_mul(SEED_MIX)
}

fn cost_cpu(order: &[usize], graph: &GraphData) -> u32 {
    let mut positions = vec![0u32; graph.node_count.max(1)];
    for (pos, &node) in order.iter().take(graph.node_count).enumerate() {
        positions[node] = pos as u32;
    }

    let mut cost = 0u32;
    for &(u, v) in &graph.edges {
        let pu = positions[u];
        let pv = positions[v];
        cost += if pu > pv { pu - pv } else { pv - pu };
    }
    cost
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
- The wgpu backend scores a batch in parallel.
- The notebook keeps the best order seen so far.

A separate card runs a GPU simulated annealing loop for comparison.

The first run can be slow while CubeCL builds shaders."#
        );
    });

    let config = nb.state("config", Config::default(), move |ui, config| {
        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label("Batch configuration");
            ui.checkbox(&mut config.auto_batch, "Adaptive batch size");
            ui.add_enabled(
                !config.auto_batch,
                widgets::Slider::new(&mut config.batch_size, MIN_BATCH_SIZE..=MAX_BATCH_SIZE)
                    .text("batch"),
            );
            if config.auto_batch {
                ui.horizontal(|ui| {
                    ui.label("target");
                    ui.add(
                        widgets::Slider::new(&mut config.target_batch_ms, 10..=500).text("ms"),
                    );
                });
            }
            ui.label("backend: wgpu");
            ui.horizontal(|ui| {
                ui.label("seed");
                ui.add(DragValue::new(&mut config.seed).speed(1));
            });
            ui.separator();
            ui.label("Graph");
            ui.horizontal(|ui| {
                ui.label("difficulty");
                ui.add(widgets::Slider::new(&mut config.difficulty, 0.0..=1.0).text("level"));
            });
            ui.checkbox(&mut config.use_difficulty, "Use difficulty slider");

            let manual = !config.use_difficulty;
            if manual {
                egui::ComboBox::from_id_salt("minla_graph_pattern")
                    .selected_text(config.graph.pattern.label())
                    .show_ui(ui, |ui| {
                        for pattern in GraphPattern::all() {
                            ui.selectable_value(
                                &mut config.graph.pattern,
                                pattern,
                                pattern.label(),
                            );
                        }
                    });

                let preset = config.graph.pattern == GraphPattern::Preset;
                if preset {
                    config.graph.node_count = DEFAULT_NODE_COUNT;
                    config.graph.edge_count = DEFAULT_EDGE_COUNT;
                }
                ui.add_enabled(
                    !preset,
                    widgets::Slider::new(&mut config.graph.node_count, 2..=MAX_NODE_COUNT)
                        .text("nodes"),
                );
                let max_edges = max_edges(config.graph.node_count).min(MAX_EDGE_COUNT);
                if config.graph.edge_count > max_edges {
                    config.graph.edge_count = max_edges;
                }
                ui.add_enabled(
                    !preset,
                    widgets::Slider::new(&mut config.graph.edge_count, 0..=max_edges)
                        .text("edges"),
                );
            } else {
                ui.label("Pattern, nodes, and edges follow the difficulty curve.");
            }

            ui.horizontal(|ui| {
                ui.label("seed");
                ui.add(DragValue::new(&mut config.graph.seed).speed(1));
                if ui
                    .add_enabled(!(manual && config.graph.pattern == GraphPattern::Preset), widgets::Button::new("Regenerate"))
                    .clicked()
                {
                    config.graph.seed = config.graph.seed.wrapping_add(1);
                }
            });
            if manual && config.graph.pattern == GraphPattern::Preset {
                ui.label("Preset keeps the original 10-node example graph.");
            }

            config.sync_graph();

            if config.use_difficulty {
                ui.label(format!(
                    "pattern: {}, nodes: {}, edges: {}",
                    config.graph.pattern.label(),
                    config.graph.node_count,
                    config.graph.edge_count
                ));
            }

            let baseline_order: Vec<usize> = (0..config.graph_data.node_count).collect();
            let baseline_cost = cost_cpu(&baseline_order, &config.graph_data);
            widgets::markdown(
                ui,
                &format!(
                    "Baseline cost (identity order): `{}` for `{}` nodes and `{}` edges.",
                    baseline_cost,
                    config.graph_data.node_count,
                    config.graph_data.edge_count
                ),
            );
        });
    });

    nb.state("minla", MinlaState::new(), move |ui, state| {
        let graph = { config.read(ui).graph_data.clone() };

        state.sync_graph(&graph);
        state.poll_runners();

        let running = state.runner_wgpu.is_running();
        if !running {
            let mut config = config.read_mut(ui);
            if config.auto_batch
                && state.last.error.is_none()
                && state.last.batch_size > 0
                && state.last.elapsed_ms > 0
            {
                let base = state
                    .last
                    .batch_size
                    .min(u32::MAX as usize) as u32;
                config.batch_size = adjust_batch_size(
                    base,
                    state.last.elapsed_ms,
                    config.target_batch_ms,
                );
            }
            config.batch_size = config.batch_size.clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE);
        }

        let (batch_size, seed) = {
            let config = config.read(ui);
            (config.batch_size.max(1) as usize, config.seed)
        };
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
                widgets::markdown(ui, &format!("Backend error: {error}"));
            } else if last.batch_size > 0 {
                let throughput = if last.elapsed_ms > 0 {
                    last.batch_size as f64 / last.elapsed_ms as f64
                } else {
                    0.0
                };
                widgets::markdown(
                    ui,
                    &format!(
                        "Last batch: `{}` candidates in `{}` ms ({:.2} candidates/ms). Best in batch: `{}`.",
                        last.batch_size,
                        last.elapsed_ms,
                        throughput,
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
            }

            if !state.history.is_empty() {
                ui.separator();
                ui.label("Performance and improvement");
                ui.columns(2, |columns| {
                    let perf_points: Vec<[f64; 2]> = state
                        .history
                        .iter()
                        .map(|entry| [entry.run as f64, entry.elapsed_ms as f64])
                        .collect();

                    Plot::new("minla_perf")
                        .height(160.0)
                        .legend(Legend::default())
                        .show(&mut columns[0], |plot_ui| {
                            if !perf_points.is_empty() {
                                plot_ui.line(Line::new("wgpu", PlotPoints::from(perf_points)));
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
            if let Err(error) = state
                .runner_wgpu
                .spawn(BatchRequest {
                    batch_size,
                    seed,
                    graph: graph.clone(),
                })
            {
                state.last = error;
            }
            ui.ctx().request_repaint();
        }
        if bump_seed {
            let mut config = config.read_mut(ui);
            config.seed = config.seed.wrapping_add(1);
        }
    });

    nb.state("anneal", AnnealState::new(), move |ui, state| {
        let (graph, target_ms) = {
            let config = config.read(ui);
            (config.graph_data.clone(), config.target_batch_ms)
        };

        state.sync_graph(&graph);
        state.poll_runner(target_ms);
        state.maybe_reset();

        let running = state.runner.is_running();
        let mut spawn_requested = false;
        let mut reset_requested = false;

        with_padding(ui, DEFAULT_CARD_PADDING, |ui| {
            ui.label("Simulated annealing (GPU)");
            ui.label(format!(
                "Auto-tuned parameters (target ~{} ms/batch).",
                target_ms
            ));
            ui.label(format!(
                "Steps/chain: {}, initial temp: {:.2}, base cooling: {:.4}.",
                state.config.steps_per_batch, state.config.initial_temp, state.config.cooling
            ));
            ui.label(format!(
                "Target acceptance: {:.2}, cooling adjust: {:.4}.",
                state.config.target_acceptance, state.config.cooling_adjust
            ));
            ui.label(format!(
                "Reheat temp: {:.2}, plateau: {} batches.",
                state.config.reheat_temp, state.config.reheat_plateau_batches
            ));
            ui.label("Reset re-tunes parameters for the current graph.");

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

            let last = &state.last;
            if let Some(error) = &last.error {
                widgets::markdown(ui, &format!("Backend error: {error}"));
            } else if last.batch_size > 0 {
                let chains_per_s = if last.elapsed_ms > 0 {
                    last.batch_size as f64 * 1000.0 / last.elapsed_ms as f64
                } else {
                    0.0
                };
                let steps_per_s = if last.elapsed_ms > 0 {
                    last.batch_size as f64 * last.steps as f64 * 1000.0
                        / last.elapsed_ms as f64
                } else {
                    0.0
                };
                widgets::markdown(
                    ui,
                    &format!(
                        "Last batch: `{}` chains × `{}` steps in `{}` ms ({:.2} chains/s, {:.0} steps/s). Best: `{}`.",
                        last.batch_size,
                        last.steps,
                        last.elapsed_ms,
                        chains_per_s,
                        steps_per_s,
                        last.best_cost
                    ),
                );
            } else {
                widgets::markdown(ui, "No annealing run yet.");
            }

            if state.best_cost != u32::MAX {
                if state.history.len() >= 2 {
                    let first = &state.history[0];
                    let last_entry = &state.history[state.history.len() - 1];
                    let mut total_ms = 0u128;
                    for entry in &state.history {
                        total_ms += entry.elapsed_ms;
                    }
                    if total_ms > 0 && last_entry.best_cost < first.best_cost {
                        let improvement = (first.best_cost - last_entry.best_cost) as f64;
                        let improvement_per_s = improvement * 1000.0 / total_ms as f64;
                        ui.label(format!(
                            "Best improvement rate: {:.2} cost/s over last {} runs.",
                            improvement_per_s,
                            state.history.len()
                        ));
                    }
                }
                widgets::markdown(
                    ui,
                    &format!(
                        "Best overall: `{}` ({} runs, {} chains, {} steps).",
                        state.best_cost,
                        state.runs,
                        state.total_chains,
                        state.total_steps
                    ),
                );
            }

            if !state.history.is_empty() {
                let batch_points: Vec<[f64; 2]> = state
                    .history
                    .iter()
                    .map(|entry| [entry.run as f64, entry.batch_cost as f64])
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
                        if !batch_points.is_empty() {
                            plot_ui.line(Line::new("batch", PlotPoints::from(batch_points)));
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
            let batch_size = {
                let config = config.read(ui);
                config.batch_size.max(1) as usize
            };
            let steps = state
                .config
                .steps_per_batch
                .clamp(MIN_ANNEAL_STEPS, MAX_ANNEAL_STEPS);
            let plateau_steps = state
                .config
                .reheat_plateau_batches
                .saturating_mul(steps);
            let seed = state.config.seed;
            let reset = state.force_reinit;
            if let Err(error) = state.runner.spawn(AnnealRequest {
                batch_size,
                steps,
                seed,
                reset,
                initial_temp: state.config.initial_temp,
                cooling: state.config.cooling,
                reheat_enabled: state.config.reheat_enabled,
                reheat_temp: state.config.reheat_temp,
                reheat_plateau_steps: plateau_steps,
                adaptive_cooling: state.config.adaptive_cooling,
                target_acceptance: state.config.target_acceptance,
                cooling_adjust: state.config.cooling_adjust,
                graph: graph.clone(),
            }) {
                state.last = error;
            }
            state.force_reinit = false;
            ui.ctx().request_repaint();
        }
    });
}
