#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! cubecl = { version = "0.9.0", default-features = false, features = ["wgpu", "cpu", "std"] }
//! egui = "0.33"
//! ```

use cubecl::cpu::{CpuDevice, CpuRuntime};
use cubecl::prelude::*;
use cubecl::wgpu::{WgpuDevice, WgpuRuntime};
use GORBIE::prelude::*;

#[cube(launch)]
fn shuffle_kernel(orders: &mut Array<u32>) {
    let mut order = Array::<u32>::new(2usize);
    order[0] = 0;
    order[1] = 1;

    let i = ABSOLUTE_POS & 1;
    let tmp = order[i];
    order[i] = 9;
    order[i ^ 1] = tmp;
    orders[0] = order[0];
    orders[1] = order[1];
}

fn run_kernel<R: Runtime>(client: &ComputeClient<R>) -> [u32; 2] {
    let orders_handle = client.empty(2 * std::mem::size_of::<u32>());

    unsafe {
        shuffle_kernel::launch::<R>(
            client,
            CubeCount::new_1d(1),
            CubeDim::new_1d(1),
            ArrayArg::from_raw_parts::<u32>(&orders_handle, 2, 1),
        )
        .expect("shuffle kernel launch");
    }

    let orders_bytes = client.read_one(orders_handle);
    let orders = u32::from_bytes(&orders_bytes);
    [orders[0], orders[1]]
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    let wgpu = {
        let device = WgpuDevice::default();
        let client = WgpuRuntime::client(&device);
        run_kernel(&client)
    };
    let cpu = {
        let device = CpuDevice::default();
        let client = CpuRuntime::client(&device);
        run_kernel(&client)
    };

    nb.view(|ui| {
        md!(
            ui,
            r#"# CubeCL CPU repro (minimal)
Kernel under test:
```rust
#[cube(launch)]
fn shuffle_kernel(orders: &mut Array<u32>) {{
    let mut order = Array::<u32>::new(2usize);
    order[0] = 0;
    order[1] = 1;

    let i = ABSOLUTE_POS & 1;
    let tmp = order[i];
    order[i] = 9;
    order[i ^ 1] = tmp;

    orders[0] = order[0];
    orders[1] = order[1];
}}
```

Expected output for batch 1 (candidate 0):
```
Input: [0, 1]
Output: [9, 0]
```

Observed on cubecl-cpu:
```
Input: [0, 1]
Output: [9, 9]
```"#
        );
    });

    nb.view(move |ui| {
        ui.label("CubeCL CPU repro (batch 1)");
        ui.label("Input: [0, 1]");
        ui.label(format!("wgpu output: {:?}", wgpu));
        ui.label(format!("cpu output: {:?}", cpu));
    });
}
