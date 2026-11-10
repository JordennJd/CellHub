//! CellHub simulation: genome, substrate and the wasm-facing world.

mod components;
mod genome;
mod terrain;
mod simulation;

pub use components::SIZE;
pub use simulation::Simulation;
