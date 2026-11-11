//! CellHub simulation: genome, substrate and the wasm-facing world.

mod cell_types;
mod components;
mod genome;
mod terrain;
mod simulation;

pub use components::SIZE;
pub use simulation::Simulation;

#[cfg(all(target_arch = "wasm32", feature = "wasm-threads"))]
pub use simulation::enable_parallel_simulation;
#[cfg(all(target_arch = "wasm32", feature = "wasm-threads"))]
pub use wasm_bindgen_rayon::init_thread_pool;
