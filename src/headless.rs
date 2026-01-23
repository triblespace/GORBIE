use crate::themes::{industrial_dark, industrial_fonts, industrial_light};
use crate::{HeadlessCaptureConfig, NotebookCore, NOTEBOOK_MIN_HEIGHT};
use dark_light::Mode;
use eframe::egui;
use egui_wgpu::wgpu;
use std::path::{Path, PathBuf};

type HeadlessResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const HEADLESS_PNG_PPI: f32 = 254.0;

pub(super) fn run_headless(mut core: NotebookCore, config: HeadlessCaptureConfig) -> HeadlessResult<()> {
    let mut runner = HeadlessWgpuRunner::new(config)?;
    runner.capture_cards(&mut core)
}

struct HeadlessWgpuRunner {
    output_dir: PathBuf,
    card_width: f32,
    ctx: egui::Context,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
    target_format: wgpu::TextureFormat,
    pixels_per_point: f32,
    target: Option<TargetBuffers>,
    time_seconds: f64,
}

impl HeadlessWgpuRunner {
    fn new(config: HeadlessCaptureConfig) -> HeadlessResult<Self> {
        std::fs::create_dir_all(&config.output_dir)?;

        let ctx = egui::Context::default();
        ctx.set_fonts(industrial_fonts());
        ctx.set_style_of(egui::Theme::Light, industrial_light());
        ctx.set_style_of(egui::Theme::Dark, industrial_dark());
        let theme = match dark_light::detect() {
            Ok(Mode::Light) => egui::ThemePreference::Light,
            Ok(Mode::Dark) => egui::ThemePreference::Dark,
            Ok(Mode::Unspecified) | Err(_) => egui::ThemePreference::Dark,
        };
        ctx.set_theme(theme);

        let instance = wgpu::Instance::default();
        let adapter_options = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        };
        let adapter = match pollster::block_on(instance.request_adapter(&adapter_options)) {
            Ok(adapter) => adapter,
            Err(err) => {
                let fallback_options = wgpu::RequestAdapterOptions {
                    power_preference: adapter_options.power_preference,
                    compatible_surface: adapter_options.compatible_surface,
                    force_fallback_adapter: true,
                };
                pollster::block_on(instance.request_adapter(&fallback_options)).map_err(
                    |fallback_err| {
                        format!(
                            "headless adapter request failed: {err}; fallback failed: {fallback_err}"
                        )
                    },
                )?
            }
        };
        let device_desc = wgpu::DeviceDescriptor {
            label: Some("gorbie_headless_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::default(),
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&device_desc))?;

        let target_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let renderer = egui_wgpu::Renderer::new(
            &device,
            target_format,
            egui_wgpu::RendererOptions::default(),
        );

        Ok(Self {
            output_dir: config.output_dir,
            card_width: config.card_width,
            ctx,
            device,
            queue,
            renderer,
            target_format,
            pixels_per_point: config.pixels_per_point,
            target: None,
            time_seconds: 0.0,
        })
    }

    fn capture_cards(&mut self, core: &mut NotebookCore) -> HeadlessResult<()> {
        let mut index = 0;
        loop {
            let (mut output, measured_height) =
                self.run_frame(core, index, NOTEBOOK_MIN_HEIGHT)?;
            let Some(measured_height) = measured_height else {
                break;
            };

            let mut textures_delta = egui::TexturesDelta::default();
            textures_delta.append(std::mem::take(&mut output.textures_delta));
            let desired_height = measured_height.max(1.0);
            let (mut output, final_height) = if height_close(NOTEBOOK_MIN_HEIGHT, desired_height) {
                (output, desired_height)
            } else {
                let (mut output, measured_height) =
                    self.run_frame(core, index, desired_height)?;
                let Some(measured_height) = measured_height else {
                    break;
                };
                textures_delta.append(std::mem::take(&mut output.textures_delta));
                (output, measured_height.max(1.0))
            };

            output.textures_delta = textures_delta;
            let image = self.render_output(output, egui::vec2(self.card_width, final_height))?;
            self.save_capture(index, &image)?;
            index += 1;
        }
        Ok(())
    }

    fn run_frame(
        &mut self,
        core: &mut NotebookCore,
        index: usize,
        height: f32,
    ) -> HeadlessResult<(egui::FullOutput, Option<f32>)> {
        let screen_rect = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(self.card_width, height),
        );
        let mut raw_input = egui::RawInput {
            screen_rect: Some(screen_rect),
            max_texture_side: Some(self.device.limits().max_texture_dimension_2d as usize),
            time: Some(self.time_seconds),
            ..Default::default()
        };
        {
            let viewport = raw_input.viewports.entry(raw_input.viewport_id).or_default();
            viewport.native_pixels_per_point = Some(self.pixels_per_point);
            viewport.inner_rect = Some(screen_rect);
            viewport.outer_rect = Some(screen_rect);
        }
        self.time_seconds += f64::from(raw_input.predicted_dt);

        let mut notebook = core.build_notebook();
        let mut measured_height: Option<f32> = None;
        let output = self.ctx.run(raw_input, |ctx| {
            measured_height = core.draw_card(ctx, &mut notebook, index, self.card_width);
        });
        Ok((output, measured_height))
    }

    fn render_output(
        &mut self,
        output: egui::FullOutput,
        size_points: egui::Vec2,
    ) -> HeadlessResult<RenderedImage> {
        let egui::FullOutput {
            textures_delta,
            shapes,
            pixels_per_point,
            ..
        } = output;
        let width = (size_points.x * pixels_per_point).round().max(1.0) as u32;
        let height = (size_points.y * pixels_per_point).round().max(1.0) as u32;
        self.ensure_target(width, height)?;

        for (id, delta) in &textures_delta.set {
            self.renderer
                .update_texture(&self.device, &self.queue, *id, delta);
        }

        let clipped_primitives =
            self.ctx
                .tessellate(shapes, pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gorbie_headless_encoder"),
            });
        let mut callbacks = self.renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        let clear = color32_to_wgpu(self.ctx.style().visuals.window_fill);
        let target = self.target.as_ref().expect("target ensured");
        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gorbie_headless_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let mut render_pass = render_pass.forget_lifetime();
            self.renderer
                .render(&mut render_pass, &clipped_primitives, &screen_descriptor);
        }
        target.copy_to_buffer(&mut encoder);

        callbacks.push(encoder.finish());
        self.queue.submit(callbacks);
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());

        let pixels = target.readback(&self.device)?;
        for id in &textures_delta.free {
            self.renderer.free_texture(id);
        }
        Ok(RenderedImage {
            width,
            height,
            pixels,
        })
    }

    fn ensure_target(&mut self, width: u32, height: u32) -> HeadlessResult<()> {
        let needs_resize = self
            .target
            .as_ref()
            .map_or(true, |target| target.dims.width != width || target.dims.height != height);
        if needs_resize {
            self.target = Some(TargetBuffers::new(
                &self.device,
                width,
                height,
                self.target_format,
            ));
        }
        Ok(())
    }

    fn save_capture(&self, index: usize, image: &RenderedImage) -> HeadlessResult<()> {
        let filename = format!("card_{:04}.png", index + 1);
        let path = self.output_dir.join(filename);
        write_png_rgba(
            &path,
            image.width,
            image.height,
            &image.pixels,
        )?;
        Ok(())
    }
}

struct TargetBuffers {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    buffer: wgpu::Buffer,
    dims: BufferDimensions,
}

impl TargetBuffers {
    fn new(device: &wgpu::Device, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        let dims = BufferDimensions::new(width, height);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gorbie_headless_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[format],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gorbie_headless_readback"),
            size: dims.buffer_size(),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            texture,
            view,
            buffer,
            dims,
        }
    }

    fn copy_to_buffer(&self, encoder: &mut wgpu::CommandEncoder) {
        let size = wgpu::Extent3d {
            width: self.dims.width,
            height: self.dims.height,
            depth_or_array_layers: 1,
        };
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.dims.padded_bytes_per_row),
                    rows_per_image: Some(self.dims.height),
                },
            },
            size,
        );
    }

    fn readback(&self, device: &wgpu::Device) -> HeadlessResult<Vec<u8>> {
        let buffer_slice = self.buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        receiver
            .recv()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "map channel closed"))??;

        let data = buffer_slice.get_mapped_range();
        let mut pixels = Vec::with_capacity(
            self.dims.unpadded_bytes_per_row as usize * self.dims.height as usize,
        );
        // Strip row padding required by wgpu's copy alignment.
        for chunk in data.chunks(self.dims.padded_bytes_per_row as usize) {
            pixels.extend_from_slice(&chunk[..self.dims.unpadded_bytes_per_row as usize]);
        }
        drop(data);
        self.buffer.unmap();
        Ok(pixels)
    }
}

#[derive(Clone, Copy)]
struct BufferDimensions {
    width: u32,
    height: u32,
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32,
}

impl BufferDimensions {
    fn new(width: u32, height: u32) -> Self {
        let bytes_per_pixel = 4;
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = ((unpadded_bytes_per_row + alignment - 1) / alignment) * alignment;
        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
        }
    }

    fn buffer_size(&self) -> u64 {
        self.padded_bytes_per_row as u64 * self.height as u64
    }
}

struct RenderedImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn write_png_rgba(
    path: &Path,
    width: u32,
    height: u32,
    data: &[u8],
) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let pixels_per_meter = (HEADLESS_PNG_PPI / 0.0254).round().max(1.0) as u32;
    encoder.set_pixel_dims(Some(png::PixelDimensions {
        xppu: pixels_per_meter,
        yppu: pixels_per_meter,
        unit: png::Unit::Meter,
    }));
    let mut writer = encoder
        .write_header()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
    writer
        .write_image_data(data)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
}

fn color32_to_wgpu(color: egui::Color32) -> wgpu::Color {
    wgpu::Color {
        r: f64::from(color.r()) / 255.0,
        g: f64::from(color.g()) / 255.0,
        b: f64::from(color.b()) / 255.0,
        a: f64::from(color.a()) / 255.0,
    }
}

fn height_close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.5
}
