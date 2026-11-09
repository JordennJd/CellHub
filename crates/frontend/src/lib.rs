//! Rendering data is testable natively; the WebGPU renderer runs in the browser.
#[cfg(any(target_arch = "wasm32", test))]
mod scene_data;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::WorldRenderer;
