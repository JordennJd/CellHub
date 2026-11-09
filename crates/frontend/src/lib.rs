//! Direct WebGPU renderer: three draws (terrain, label atlas, instanced cells).
//! React owns UI/input; the simulation remains in its Worker.
#![cfg(target_arch = "wasm32")]

use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;

const TERRAIN_BYTES: u64 = 48 * 48 * 16;
const CELL_STRIDE: u64 = 40;
const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x2];

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}
fn buffer(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    vertex: &str,
    fragment: &str,
    instanced: bool,
) -> wgpu::RenderPipeline {
    let buffers = [wgpu::VertexBufferLayout {
        array_stride: CELL_STRIDE,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: if instanced { &buffers } else { &[] },
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
        cache: None,
    })
}

#[wasm_bindgen]
pub struct WorldRenderer {
    canvas: web_sys::HtmlCanvasElement,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    terrain_pipeline: wgpu::RenderPipeline,
    cell_pipeline: wgpu::RenderPipeline,
    label_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    terrain: wgpu::Buffer,
    cells: wgpu::Buffer,
    capacity: u64,
    count: u32,
    atlas: wgpu::Texture,
    view: [f32; 8],
    dirty: bool,
    destroyed: bool,
    gpu_error: Arc<Mutex<Option<String>>>,
}

#[wasm_bindgen]
impl WorldRenderer {
    pub async fn create(canvas: web_sys::HtmlCanvasElement) -> Result<WorldRenderer, JsValue> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(error)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| error("WebGPU adapter unavailable"))?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("CellHub"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .map_err(error)?;
        let gpu_error = Arc::new(Mutex::new(None));
        let reported = gpu_error.clone();
        device.on_uncaptured_error(Box::new(move |e| {
            *reported.lock().unwrap() = Some(e.to_string());
        }));
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut config = surface
            .get_default_config(&adapter, 1, 1)
            .ok_or_else(|| error("No compatible WebGPU surface format"))?;
        // Match CSS/Pixi color values without an extra sRGB framebuffer conversion.
        config.format = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(|f| !f.is_srgb())
            .unwrap_or(config.format);
        canvas.set_width(1);
        canvas.set_height(1);
        surface.configure(&device, &config);
        let view = [1., 1., 1536., 1536., 1., 0., -1., 0.];
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera"),
            contents: bytemuck::cast_slice(&view),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let terrain = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain"),
            contents: &vec![0u8; TERRAIN_BYTES as usize],
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("static map label atlas"),
            size: wgpu::Extent3d {
                width: 512,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bindings = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene"),
            layout: &bindings,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: terrain.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene layout"),
            bind_group_layouts: &[&bindings],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("CellHub WGSL"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let terrain_pipeline = pipeline(
            &device,
            &layout,
            &shader,
            config.format,
            "terrain_vs",
            "terrain_fs",
            false,
        );
        let cell_pipeline = pipeline(
            &device,
            &layout,
            &shader,
            config.format,
            "cell_vs",
            "cell_fs",
            true,
        );
        let label_pipeline = pipeline(
            &device,
            &layout,
            &shader,
            config.format,
            "label_vs",
            "label_fs",
            false,
        );
        let capacity = 1024 * CELL_STRIDE;
        let cells = buffer(
            &device,
            "cell instances",
            capacity,
            wgpu::BufferUsages::VERTEX,
        );
        if let Some(e) = device.pop_error_scope().await {
            return Err(error(e));
        }
        Ok(Self {
            canvas,
            surface,
            device,
            queue,
            config,
            terrain_pipeline,
            cell_pipeline,
            label_pipeline,
            bind_group,
            uniform,
            terrain,
            cells,
            capacity,
            count: 0,
            atlas,
            view,
            dirty: true,
            destroyed: false,
            gpu_error,
        })
    }
    pub fn set_frame(&mut self, cells: &[f32], terrain: &[f32]) -> Result<(), JsValue> {
        if self.destroyed {
            return Err(error("Renderer destroyed"));
        }
        if cells.len() % 10 != 0
            || terrain.len() != 48 * 48 * 4
            || cells.iter().chain(terrain).any(|v| !v.is_finite())
        {
            return Err(error("Invalid render snapshot"));
        }
        let bytes: &[u8] = bytemuck::cast_slice(cells);
        if bytes.len() as u64 > self.capacity {
            let capacity = (bytes.len() as u64).next_power_of_two();
            if capacity > self.device.limits().max_buffer_size {
                return Err(error("Cell buffer exceeds adapter limit"));
            }
            self.cells.destroy();
            self.cells = buffer(
                &self.device,
                "cell instances",
                capacity,
                wgpu::BufferUsages::VERTEX,
            );
            self.capacity = capacity;
        }
        if !bytes.is_empty() {
            self.queue.write_buffer(&self.cells, 0, bytes);
        }
        self.queue
            .write_buffer(&self.terrain, 0, bytemuck::cast_slice(terrain));
        self.count = (cells.len() / 10) as u32;
        self.dirty = true;
        Ok(())
    }
    /// Static text rasterized once into an atlas, then placed/drawn by the GPU.
    pub fn set_labels(&mut self, rgba: &[u8]) -> Result<(), JsValue> {
        if self.destroyed || rgba.len() != 512 * 128 * 4 {
            return Err(error("Invalid label atlas"));
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(512 * 4),
                rows_per_image: Some(128),
            },
            wgpu::Extent3d {
                width: 512,
                height: 128,
                depth_or_array_layers: 1,
            },
        );
        self.dirty = true;
        Ok(())
    }
    pub fn set_view(&mut self, x: f32, y: f32, zoom: f32, layer: u32, selected: i32) {
        if x.is_finite() && y.is_finite() && zoom.is_finite() && zoom > 0. {
            self.view[2..7].copy_from_slice(&[x, y, zoom, layer as f32, selected as f32]);
            self.dirty = true;
        }
    }
    pub fn render(&mut self) -> Result<(), JsValue> {
        if self.destroyed {
            return Ok(());
        }
        if let Some(e) = self.gpu_error.lock().unwrap().as_ref() {
            return Err(error(e));
        }
        let width = self.canvas.client_width();
        let height = self.canvas.client_height();
        if width <= 0 || height <= 0 {
            return Ok(());
        }
        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.)
            .clamp(1., 2.);
        let max = self.device.limits().max_texture_dimension_2d;
        let pw = ((width as f64 * dpr).round() as u32).clamp(1, max);
        let ph = ((height as f64 * dpr).round() as u32).clamp(1, max);
        if self.config.width != pw
            || self.config.height != ph
            || self.view[0] != width as f32
            || self.view[1] != height as f32
        {
            self.canvas.set_width(pw);
            self.canvas.set_height(ph);
            self.config.width = pw;
            self.config.height = ph;
            self.surface.configure(&self.device, &self.config);
            self.view[0] = width as f32;
            self.view[1] = height as f32;
            self.dirty = true;
        }
        if !self.dirty {
            return Ok(());
        }
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(e) => return Err(error(e)),
        };
        self.queue
            .write_buffer(&self.uniform, 0, bytemuck::cast_slice(&self.view));
        let target = frame.texture.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("CellHub frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 9. / 255.,
                            g: 20. / 255.,
                            b: 17. / 255.,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_pipeline(&self.terrain_pipeline);
            pass.draw(0..6, 0..1);
            pass.set_pipeline(&self.label_pipeline);
            pass.draw(0..6, 0..4);
            if self.count > 0 {
                pass.set_pipeline(&self.cell_pipeline);
                pass.set_vertex_buffer(0, self.cells.slice(..));
                pass.draw(0..6, 0..self.count);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        self.dirty = false;
        Ok(())
    }
    pub fn destroy(&mut self) {
        if !self.destroyed {
            self.destroyed = true;
            self.cells.destroy();
            self.terrain.destroy();
            self.uniform.destroy();
            self.atlas.destroy();
            self.device.destroy();
        }
    }
}
