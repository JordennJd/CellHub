//! Rendering data is testable natively; the wgpu renderer runs in the browser
//! on WebGPU or its WebGL2 fallback.
#[cfg(any(target_arch = "wasm32", test))]
mod scene_data;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::WorldRenderer;
