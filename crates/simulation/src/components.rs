//! ECS components and world-scale constants.

/// Side length of the square world.
pub const SIZE: f32 = 3072.;
/// Hard population cap.
pub const LIMIT: usize = 20_000;
pub(crate) const SNAPSHOT_STRIDE: usize = 12;
/// Accelerated lifecycle: one original-game hour corresponds to one web second.
pub(crate) const MIN_DIVISION_AGE: f32 = 0.5;

#[derive(Clone, Copy)]
pub(crate) struct Position {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) angle: f32,
}

#[derive(Clone)]
pub(crate) struct Cell {
    pub(crate) id: u32,
    pub(crate) mode: usize,
    pub(crate) mass: f32,
    pub(crate) age: f32,
    pub(crate) generation: u32,
    pub(crate) mutations: u32,
    pub(crate) cooldown: f32,
    pub(crate) mirrored: bool,
    /// Diffusing S1..S4 concentrations, in micromoles per litre.
    pub(crate) signal: [f32; 4],
    /// Cyanide load. Slowly damages the cell and is shared through bonds.
    pub(crate) toxin: f32,
}

/// Drawing and collision radius derived from mass.
pub(crate) fn radius(mass: f32) -> f32 {
    5. + mass.sqrt() * 5.
}
