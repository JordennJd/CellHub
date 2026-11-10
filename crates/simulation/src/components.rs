//! ECS components and world-scale constants.

/// Side length of the square world.
pub const SIZE: f32 = 3072.;
/// Hard population cap.
pub const LIMIT: usize = 20_000;
pub(crate) const SNAPSHOT_STRIDE: usize = 12;
/// Accelerated lifecycle: one original-game hour corresponds to one web second.
pub(crate) const MIN_DIVISION_AGE: f32 = 0.5;
pub(crate) const INITIAL_NITROGEN: f32 = 1.;
pub(crate) const NITROGEN_DIVISION: f32 = 0.2;
pub(crate) const NITROGEN_DEATH: f32 = 0.1;

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
    pub(crate) nitrogen: f32,
    pub(crate) age: f32,
    pub(crate) generation: u32,
    pub(crate) mutations: u32,
    pub(crate) cooldown: f32,
    pub(crate) mirrored: bool,
}

/// Drawing and collision radius derived from mass.
pub(crate) fn radius(mass: f32) -> f32 {
    5. + mass.sqrt() * 5.
}
