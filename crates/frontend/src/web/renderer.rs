//! GPU renderer state: resources, frame upload, painting and teardown.

use super::backend::Backend;
use super::error::error;
use super::gpu::{ATTRIBUTES, CELL_STRIDE, LINK_ATTRIBUTES, LINK_STRIDE, buffer, pipeline};
use super::labels::{ATLAS_HEIGHT, ATLAS_WIDTH, rasterize_labels};
use super::material;
use crate::scene_data::{CELL_VALUES, pack_links};
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;

const TERRAIN_SIZE: u32 = 48;
const TERRAIN_STRIDE: u32 = TERRAIN_SIZE * 16;
const TERRAIN_BYTES: u64 = (TERRAIN_STRIDE * TERRAIN_SIZE) as u64;

pub(super) struct RendererState {
    canvas: web_sys::HtmlCanvasElement,
    backend: Backend,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    terrain_pipeline: wgpu::RenderPipeline,
    cell_pipeline: wgpu::RenderPipeline,
    label_pipeline: wgpu::RenderPipeline,
    link_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    terrain: wgpu::Texture,
    cells: wgpu::Buffer,
    capacity: u64,
    count: u32,
    links: wgpu::Buffer,
    link_capacity: u64,
    link_count: u32,
    atlas: wgpu::Texture,
    material: wgpu::Texture,
    view: [f32; 8],
    dirty: bool,
    destroyed: bool,
    gpu_error: Arc<Mutex<Option<String>>>,
    error_handler: Option<js_sys::Function>,
}
impl RendererState {
    pub(super) async fn create(
        canvas: web_sys::HtmlCanvasElement,
        requested: &str,
    ) -> Result<Self, JsValue> {
        let backend = Backend::select(requested).await?;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: match backend {
                Backend::WebGpu => wgpu::Backends::BROWSER_WEBGPU,
                Backend::WebGl => wgpu::Backends::GL,
            },
            ..Default::default()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(error)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                // Default power preference. On PRIME offload laptops (Intel +
                // NVIDIA) "high performance" selects the discrete GPU, but
                // Chrome allocates canvas buffers on the active GPU; importing
                // that buffer into the NVIDIA device fails Dawn's external
                // memory size check, so every frame dies with "Requested
                // allocation size is smaller than the image requires".
                power_preference: wgpu::PowerPreference::None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| {
                error(match backend {
                    Backend::WebGpu => "WebGPU adapter unavailable",
                    Backend::WebGl => "WebGL2 adapter unavailable",
                })
            })?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("CellHub"),
                    required_features: wgpu::Features::empty(),
                    // WebGL2 is GLES 3.0: no storage buffers/compute, 16 KiB
                    // UBOs and 2048 px textures guaranteed. Keep that floor
                    // but allow the adapter's larger canvas resolution.
                    required_limits: match backend {
                        Backend::WebGpu => wgpu::Limits::default(),
                        Backend::WebGl => wgpu::Limits::downlevel_webgl2_defaults()
                            .using_resolution(adapter.limits()),
                    },
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
        // Size the backing store before the first compositing opportunity.
        // Going through a 1x1 surface and resizing on the next rAF can make
        // some Chrome/Dawn versions import a stale shared image.
        let width = canvas.client_width().max(1) as f32;
        let height = canvas.client_height().max(1) as f32;
        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.)
            .clamp(1., 2.);
        let max = device.limits().max_texture_dimension_2d;
        let pw = ((width as f64 * dpr).round() as u32).clamp(1, max);
        let ph = ((height as f64 * dpr).round() as u32).clamp(1, max);
        config.width = pw;
        config.height = ph;
        canvas.set_width(pw);
        canvas.set_height(ph);
        surface.configure(&device, &config);
        let view = [width, height, 1536., 1536., 1., 0., -1., 0.];
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera"),
            contents: bytemuck::cast_slice(&view),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        // A texture instead of a storage buffer: WebGL2 has no SSBOs. The
        // shader reads one texel per terrain cell with textureLoad.
        let terrain = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain"),
            size: wgpu::Extent3d {
                width: TERRAIN_SIZE,
                height: TERRAIN_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &terrain,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &vec![0u8; TERRAIN_BYTES as usize],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TERRAIN_STRIDE),
                rows_per_image: Some(TERRAIN_SIZE),
            },
            wgpu::Extent3d {
                width: TERRAIN_SIZE,
                height: TERRAIN_SIZE,
                depth_or_array_layers: 1,
            },
        );
        let terrain_view = terrain.create_view(&Default::default());
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("static map label atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_WIDTH,
                height: ATLAS_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rasterize_labels()?,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_WIDTH * 4),
                rows_per_image: Some(ATLAS_HEIGHT),
            },
            wgpu::Extent3d {
                width: ATLAS_WIDTH,
                height: ATLAS_HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        let atlas_view = atlas.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let material = material::texture(&device, &queue);
        let material_view = material.create_view(&Default::default());
        let material_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material grain"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
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
                    resource: wgpu::BindingResource::TextureView(&terrain_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&material_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&material_sampler),
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
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("../cell_shapes.wgsl"),
                    "\n",
                    include_str!("../scene.wgsl")
                )
                .into(),
            ),
        });
        let terrain_pipeline = pipeline(
            &device,
            &layout,
            &shader,
            config.format,
            "terrain_vs",
            "terrain_fs",
            None,
        );
        let cell_pipeline = pipeline(
            &device,
            &layout,
            &shader,
            config.format,
            "cell_vs",
            "cell_fs",
            Some((CELL_STRIDE, &ATTRIBUTES)),
        );
        let label_pipeline = pipeline(
            &device,
            &layout,
            &shader,
            config.format,
            "label_vs",
            "label_fs",
            None,
        );
        let link_pipeline = pipeline(
            &device,
            &layout,
            &shader,
            config.format,
            "link_vs",
            "link_fs",
            Some((LINK_STRIDE, &LINK_ATTRIBUTES)),
        );
        let capacity = 1024 * CELL_STRIDE;
        let cells = buffer(
            &device,
            "cell instances",
            capacity,
            wgpu::BufferUsages::VERTEX,
        );
        let link_capacity = 1024 * LINK_STRIDE;
        let links = buffer(
            &device,
            "bond instances",
            link_capacity,
            wgpu::BufferUsages::VERTEX,
        );
        if let Some(e) = device.pop_error_scope().await {
            return Err(error(e));
        }
        Ok(Self {
            canvas,
            backend,
            surface,
            device,
            queue,
            config,
            terrain_pipeline,
            cell_pipeline,
            label_pipeline,
            link_pipeline,
            bind_group,
            uniform,
            terrain,
            cells,
            capacity,
            count: 0,
            links,
            link_capacity,
            link_count: 0,
            atlas,
            material,
            view,
            dirty: true,
            destroyed: false,
            gpu_error,
            error_handler: None,
        })
    }
    pub(super) fn set_frame(
        &mut self,
        cells: &[f32],
        terrain: &[f32],
        links: &[u32],
    ) -> Result<(), JsValue> {
        if self.destroyed {
            return Err(error("Renderer destroyed"));
        }
        if cells.len() % CELL_VALUES != 0
            || terrain.len() != 48 * 48 * 4
            || cells.iter().chain(terrain).any(|v| !v.is_finite())
        {
            return Err(error("Invalid render snapshot"));
        }
        let link_instances = pack_links(cells, links).map_err(error)?;
        let link_bytes: &[u8] = bytemuck::cast_slice(&link_instances);
        if link_bytes.len() as u64 > self.link_capacity {
            let capacity = (link_bytes.len() as u64).next_power_of_two();
            if capacity > self.device.limits().max_buffer_size {
                return Err(error("Bond buffer exceeds adapter limit"));
            }
            self.links.destroy();
            self.links = buffer(
                &self.device,
                "bond instances",
                capacity,
                wgpu::BufferUsages::VERTEX,
            );
            self.link_capacity = capacity;
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
        if !link_bytes.is_empty() {
            self.queue.write_buffer(&self.links, 0, link_bytes);
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.terrain,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(terrain),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TERRAIN_STRIDE),
                rows_per_image: Some(TERRAIN_SIZE),
            },
            wgpu::Extent3d {
                width: TERRAIN_SIZE,
                height: TERRAIN_SIZE,
                depth_or_array_layers: 1,
            },
        );
        self.count = (cells.len() / CELL_VALUES) as u32;
        self.link_count = (links.len() / 2) as u32;
        self.dirty = true;
        Ok(())
    }
    pub(super) fn set_view(&mut self, x: f32, y: f32, zoom: f32, layer: u32, selected: i32) {
        if x.is_finite() && y.is_finite() && zoom.is_finite() && zoom > 0. {
            self.view[2..7].copy_from_slice(&[x, y, zoom, layer as f32, selected as f32]);
            self.dirty = true;
        }
    }
    pub(super) fn render(&mut self) -> Result<(), JsValue> {
        if self.destroyed {
            return Ok(());
        }
        if let Some(e) = self.gpu_error.lock().unwrap().as_ref() {
            return Err(error(e));
        }
        let width = self.canvas.client_width();
        let height = self.canvas.client_height();
        if width <= 0 || height <= 0 {
            // Wait for the resize listener instead of spinning rAF on a
            // zero-sized canvas.
            self.dirty = false;
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
            if self.link_count > 0 {
                pass.set_pipeline(&self.link_pipeline);
                pass.set_vertex_buffer(0, self.links.slice(..));
                pass.draw(0..6, 0..self.link_count);
            }
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
    pub(super) fn destroy(&mut self) {
        if !self.destroyed {
            self.destroyed = true;
            self.cells.destroy();
            self.terrain.destroy();
            self.links.destroy();
            self.uniform.destroy();
            self.atlas.destroy();
            self.material.destroy();
            self.device.destroy();
        }
    }
}

impl RendererState {
    pub(super) fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    pub(super) fn canvas(&self) -> &web_sys::HtmlCanvasElement {
        &self.canvas
    }

    pub(super) fn set_error_handler(&mut self, handler: js_sys::Function) {
        self.error_handler = Some(handler);
    }

    pub(super) fn error_handler(&self) -> Option<js_sys::Function> {
        self.error_handler.clone()
    }

    pub(super) fn needs_repaint(&self) -> bool {
        self.dirty
    }

    pub(super) fn mark_clean(&mut self) {
        self.dirty = false;
    }
}
