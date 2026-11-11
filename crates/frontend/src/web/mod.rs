//! Direct wgpu renderer: terrain, labels, instanced bonds and cells.
//! WebGPU is preferred; the same pipelines and WGSL run on WebGL2 through
//! wgpu's GL backend when WebGPU is unavailable. React owns UI/input; the
//! simulation remains in its Worker.

mod error;
mod backend;
mod gpu;
mod labels;
mod material;
mod renderer;
mod repaint;

use error::js_message;
use renderer::RendererState;
use repaint::RepaintSignal;

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

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

#[wasm_bindgen]
impl WorldRenderer {
    /// `canvas` is only a DOM surface. Rust creates all GPU resources, owns the
    /// animation-frame loop and submits every draw itself. `backend` is
    /// `"auto"`, `"webgpu"` or `"webgl"`; auto probes WebGPU before touching
    /// the canvas, because a canvas cannot switch from a WebGPU context to
    /// WebGL2 later.
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        backend: String,
    ) -> Result<WorldRenderer, JsValue> {
        let state = Rc::new(RefCell::new(RendererState::create(canvas, &backend).await?));
        let signal = RepaintSignal::default();
        let callback = {
            let state: Weak<RefCell<RendererState>> = Rc::downgrade(&state);
            let signal = signal.clone();
            Closure::wrap(Box::new(move || {
                signal.clear_handle();
                let Some(state) = state.upgrade() else {
                    return;
                };
                let mut renderer = state.borrow_mut();
                let result = renderer.render();
                let ok = result.is_ok();
                let reschedule = ok && renderer.needs_repaint();
                let handler = renderer.error_handler();
                if !ok {
                    renderer.mark_clean();
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
        signal.set_callback(function);
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
        observer.observe(state.borrow().canvas());
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

    /// Actual backend chosen by `create`, for the status label and for
    /// WebGPU-only device-lost handling in JS.
    pub fn backend(&self) -> String {
        self.state.borrow().backend_name().to_string()
    }

    pub fn set_error_handler(&mut self, handler: js_sys::Function) {
        self.state.borrow_mut().set_error_handler(handler);
    }

    pub fn set_frame(
        &mut self,
        cells: &[f32],
        terrain: &[f32],
        links: &[u32],
        width: Option<f32>,
        height: Option<f32>,
        circular: Option<bool>,
    ) -> Result<(), JsValue> {
        let side = ((terrain.len() / 4) as f32).sqrt() * 64.;
        self.state.borrow_mut().set_frame(
            cells,
            terrain,
            links,
            width.unwrap_or(side),
            height.unwrap_or(side),
            circular.unwrap_or(false),
        )?;
        self.signal.request();
        Ok(())
    }

    /// Update cell and bond positions while reusing the uploaded terrain.
    pub fn set_geometry(&mut self, cells: &[f32], links: &[u32]) -> Result<(), JsValue> {
        self.state.borrow_mut().set_geometry(cells, links)?;
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
