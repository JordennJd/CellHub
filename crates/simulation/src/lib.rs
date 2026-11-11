//! CellHub simulation: genome, substrate and the wasm-facing world.

mod components;
mod cell_types;
mod genome;
mod terrain;
mod simulation;

pub use components::SIZE;
pub use simulation::Simulation;
