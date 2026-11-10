//! Direct WebGPU renderer: terrain, labels, instanced bonds and cells.
//! React owns UI/input; the simulation remains in its Worker.
use crate::scene_data::pack_links;

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;

const TERRAIN_BYTES: u64 = 48 * 48 * 16;
const CELL_STRIDE: u64 = 40;
const LINK_STRIDE: u64 = 24;
const ATLAS_WIDTH: u32 = 512;
const ATLAS_HEIGHT: u32 = 128;
const LABELS: [&str; 4] = [
    "СОЛНЕЧНОЕ МЕЛКОВОДЬЕ",
    "ПИТАТЕЛЬНЫЙ ПОЯС",
    "ТЁМНЫЕ ГЛУБИНЫ",
    "РАДИАЦИОННЫЙ КАРМАН",
];
const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x2];
const LINK_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2];

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}
fn js_message(e: &JsValue) -> String {
    e.as_string()
        .or_else(|| {
            e.dyn_ref::<js_sys::Error>()
                .map(|e| String::from(e.message()))
        })
        .unwrap_or_else(|| format!("{e:?}"))
}

/// Rasterize the four static map labels once, in Rust, into the atlas that the
/// fragment shader samples. The browser only ever uploads the resulting bytes
/// to the GPU; no JavaScript canvas is involved in drawing.
fn rasterize_labels() -> Result<Vec<u8>, JsValue> {
    use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
    let font = FontRef::try_from_slice(epaint_default_fonts::HACK_REGULAR).map_err(error)?;
    let scale = PxScale::from(20.);
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();
    let mut pixels = vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize];
    for (row, label) in LABELS.iter().enumerate() {
        let baseline = row as f32 * 32. + ascent;
        let mut pen = 0.;
        for ch in label.chars() {
            let id = font.glyph_id(ch);
            let glyph = id.with_scale_and_position(scale, point(pen, baseline));
            if let Some(outline) = font.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                outline.draw(|gx, gy, coverage| {
                    let x = bounds.min.x as i32 + gx as i32;
                    let y = bounds.min.y as i32 + gy as i32;
                    if x < 0 || y < 0 || x >= ATLAS_WIDTH as i32 || y >= ATLAS_HEIGHT as i32 {
                        return;
                    }
                    let index = ((y as u32 * ATLAS_WIDTH + x as u32) * 4) as usize;
                    let src = coverage.clamp(0., 1.);
                    let dst = pixels[index + 3] as f32 / 255.;
                    pixels[index] = 255;
                    pixels[index + 1] = 255;
                    pixels[index + 2] = 255;
                    pixels[index + 3] = ((src + dst * (1. - src)).clamp(0., 1.) * 255.) as u8;
                });
            }
            pen += scaled.h_advance(id) + 3.;
        }
    }
    Ok(pixels)
}

/// One `requestAnimationFrame` at a time. Rust owns this signal, so JS only
/// reports "new data/view exists"; it never drives the WebGPU paint call.
#[derive(Default, Clone)]
struct RepaintSignal {
    callback: Rc<RefCell<Option<js_sys::Function>>>,
    handle: Rc<Cell<Option<i32>>>,
}
impl RepaintSignal {
    fn request(&self) {
        if self.handle.get().is_some() {
            return;
        }
        let Some(window) = web_sys::window() else {
            return;
        };
        let callback = self.callback.borrow();
        if let Some(callback) = callback.as_ref() {
            if let Ok(handle) = window.request_animation_frame(callback) {
                self.handle.set(Some(handle));
            }
        }
    }
    fn cancel(&self) {
        if let Some(handle) = self.handle.take() {
            if let Some(window) = web_sys::window() {
                let _ = window.cancel_animation_frame(handle);
            }
        }
        self.callback.borrow_mut().take();
    }
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
    instance_layout: Option<(u64, &[wgpu::VertexAttribute])>,
) -> wgpu::RenderPipeline {
    let buffers = [wgpu::VertexBufferLayout {
        array_stride: instance_layout.map_or(CELL_STRIDE, |v| v.0),
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: instance_layout.map_or(&ATTRIBUTES[..], |v| v.1),
    }];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: if instance_layout.is_some() {
                &buffers
            } else {
                &[]
            },
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

struct RendererState {
    canvas: web_sys::HtmlCanvasElement,
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
    terrain: wgpu::Buffer,
    cells: wgpu::Buffer,
    capacity: u64,
    count: u32,
    links: wgpu::Buffer,
    link_capacity: u64,
    link_count: u32,
    atlas: wgpu::Texture,
    view: [f32; 8],
    dirty: bool,
    destroyed: bool,
    gpu_error: Arc<Mutex<Option<String>>>,
    error_handler: Option<js_sys::Function>,
}

#[wasm_bindgen]
pub struct WorldRenderer {
    state: Rc<RefCell<RendererState>>,
    signal: RepaintSignal,
    window: Option<web_sys::Window>,
    resize: Option<Closure<dyn FnMut()>>,
    observer: Option<web_sys::ResizeObserver>,
    _observer_callback: Option<Closure<dyn FnMut(js_sys::Array, web_sys::ResizeObserver)>>,
    // Keeping the closure alive also keeps the wasm-bindgen trampoline valid
    // for the whole lifetime of the renderer.
    _raf: Closure<dyn FnMut()>,
}

impl RendererState {
    async fn create(canvas: web_sys::HtmlCanvasElement) -> Result<Self, JsValue> {
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
        let terrain = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain"),
            contents: &vec![0u8; TERRAIN_BYTES as usize],
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
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
            view,
            dirty: true,
            destroyed: false,
            gpu_error,
            error_handler: None,
        })
    }
    pub fn set_frame(
        &mut self,
        cells: &[f32],
        terrain: &[f32],
        links: &[u32],
    ) -> Result<(), JsValue> {
        if self.destroyed {
            return Err(error("Renderer destroyed"));
        }
        if cells.len() % 10 != 0
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
        self.queue
            .write_buffer(&self.terrain, 0, bytemuck::cast_slice(terrain));
        self.count = (cells.len() / 10) as u32;
        self.link_count = (links.len() / 2) as u32;
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
    pub fn destroy(&mut self) {
        if !self.destroyed {
            self.destroyed = true;
            self.cells.destroy();
            self.terrain.destroy();
            self.links.destroy();
            self.uniform.destroy();
            self.atlas.destroy();
            self.device.destroy();
        }
    }
}

#[wasm_bindgen]
impl WorldRenderer {
    /// `canvas` is only a DOM surface. Rust creates all GPU resources, owns the
    /// animation-frame loop and submits every WebGPU command itself.
    pub async fn create(canvas: web_sys::HtmlCanvasElement) -> Result<WorldRenderer, JsValue> {
        let state = Rc::new(RefCell::new(RendererState::create(canvas).await?));
        let signal = RepaintSignal::default();
        let callback = {
            let state: Weak<RefCell<RendererState>> = Rc::downgrade(&state);
            let signal = signal.clone();
            Closure::wrap(Box::new(move || {
                signal.handle.set(None);
                let Some(state) = state.upgrade() else {
                    return;
                };
                let mut renderer = state.borrow_mut();
                let result = renderer.render();
                let ok = result.is_ok();
                let reschedule = ok && renderer.dirty;
                let handler = renderer.error_handler.clone();
                if !ok {
                    renderer.dirty = false;
                }
                drop(renderer);
                match result {
                    Ok(()) => {
                        if reschedule {
                            signal.request();
                        }
                    }
                    Err(e) => {
                        let message = js_message(&e);
                        if let Some(handler) = handler {
                            let _ = handler.call1(
                                &JsValue::NULL,
                                &JsValue::from_str(&format!("Renderer: {message}")),
                            );
                        } else {
                            web_sys::console::error_1(&JsValue::from_str(&message));
                        }
                    }
                }
            }) as Box<dyn FnMut()>)
        };
        let function: js_sys::Function = callback
            .as_ref()
            .unchecked_ref::<js_sys::Function>()
            .clone();
        *signal.callback.borrow_mut() = Some(function);
        let window = web_sys::window();
        let resize = Some(Closure::wrap(Box::new({
            let signal = signal.clone();
            move || signal.request()
        }) as Box<dyn FnMut()>));
        if let (Some(window), Some(resize)) = (&window, &resize) {
            window.add_event_listener_with_callback(
                "resize",
                resize.as_ref().unchecked_ref::<js_sys::Function>(),
            )?;
        }
        // A layout change can resize the canvas without a window resize (for
        // example when the editor gains an archetype). The old JS rAF loop
        // polled for that; the observer keeps the Rust-driven loop correct.
        let observer_callback = Closure::wrap(Box::new({
            let signal = signal.clone();
            move |_: js_sys::Array, _: web_sys::ResizeObserver| signal.request()
        })
            as Box<dyn FnMut(js_sys::Array, web_sys::ResizeObserver)>);
        let observer = web_sys::ResizeObserver::new(
            observer_callback
                .as_ref()
                .unchecked_ref::<js_sys::Function>(),
        )?;
        observer.observe(&state.borrow().canvas);
        let renderer = Self {
            state,
            signal: signal.clone(),
            window,
            resize,
            observer: Some(observer),
            _observer_callback: Some(observer_callback),
            _raf: callback,
        };
        renderer.signal.request();
        Ok(renderer)
    }
    pub fn set_error_handler(&mut self, handler: js_sys::Function) {
        self.state.borrow_mut().error_handler = Some(handler);
    }
    pub fn set_frame(
        &mut self,
        cells: &[f32],
        terrain: &[f32],
        links: &[u32],
    ) -> Result<(), JsValue> {
        self.state.borrow_mut().set_frame(cells, terrain, links)?;
        self.signal.request();
        Ok(())
    }
    pub fn set_view(&mut self, x: f32, y: f32, zoom: f32, layer: u32, selected: i32) {
        self.state
            .borrow_mut()
            .set_view(x, y, zoom, layer, selected);
        self.signal.request();
    }
    /// Manual synchronous paint, mainly for tests and GPU readback fixtures.
    /// The app never calls this: the Rust animation-frame loop does.
    pub fn render(&mut self) -> Result<(), JsValue> {
        self.state.borrow_mut().render()
    }
    pub fn destroy(&mut self) {
        self.signal.cancel();
        if let (Some(window), Some(resize)) = (self.window.take(), self.resize.take()) {
            let _ = window.remove_event_listener_with_callback(
                "resize",
                resize.as_ref().unchecked_ref::<js_sys::Function>(),
            );
        }
        if let Some(observer) = self.observer.take() {
            observer.disconnect();
        }
        self._observer_callback.take();
        self.state.borrow_mut().destroy();
    }
}

impl Drop for WorldRenderer {
    fn drop(&mut self) {
        self.destroy();
    }
}
