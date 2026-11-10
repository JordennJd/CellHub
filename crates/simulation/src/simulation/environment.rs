//! User-controlled substrate settings; changing them preserves resource depletion.

use super::Simulation;
use crate::terrain::map;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Environment {
    pub light: f32,
    pub food: f32,
    pub radiation: f32,
    pub viscosity: f32,
    pub aging: bool,
    pub kill_at_edge: bool,
    pub only_point_mutations: bool,
    pub gravity: f32,
    /// Log10 of fluid density relative to the common reference cell density.
    pub density: f32,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            light: 1.,
            food: 1.,
            radiation: 1.,
            viscosity: 1.,
            aging: true,
            kill_at_edge: false,
            only_point_mutations: false,
            gravity: 0.,
            density: 0.,
        }
    }
}

impl Environment {
    pub(super) fn validate(&self) -> Result<(), String> {
        if [self.light, self.food, self.radiation]
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=5.).contains(v))
            || !self.viscosity.is_finite()
            || !(0.1..=5.).contains(&self.viscosity)
            || !self.gravity.is_finite()
            || !(0. ..=1.).contains(&self.gravity)
            || !self.density.is_finite()
            || !(-3.3..=3.3).contains(&self.density)
        {
            return Err("Некорректные параметры среды".into());
        }
        Ok(())
    }

    /// A bounded terminal drift for the web model, not an Android physics constant.
    pub(super) fn vertical_drift(&self) -> f32 {
        let fluid_density = 10_f32.powf(self.density);
        20. * self.gravity * (1. - fluid_density) / (1. + fluid_density) / self.viscosity
    }
}

impl Simulation {
    pub(super) fn configure_environment(&mut self, settings: Environment) -> Result<(), String> {
        settings.validate()?;
        for (tile, baseline) in self.tiles.iter_mut().zip(map()) {
            let fraction = if tile.capacity > 0. {
                tile.food / tile.capacity
            } else {
                1.
            };
            tile.light = baseline.light * settings.light;
            tile.radiation = baseline.radiation * settings.radiation;
            tile.capacity = baseline.capacity * settings.food;
            tile.food = (fraction * tile.capacity).clamp(0., tile.capacity);
        }
        self.settings = settings;
        Ok(())
    }
}
