//! Laboratory interventions, independent of genome and time controls.

use super::Simulation;
use crate::components::{Cell, Position, SIZE};
use hecs::Entity;

impl Simulation {
    pub(super) fn find_cell(&self, id: u32) -> Result<Entity, String> {
        self.world
            .query::<&Cell>()
            .iter()
            .find(|(_, c)| c.id == id)
            .map(|(entity, _)| entity)
            .ok_or_else(|| "Клетка больше не существует".into())
    }

    pub(super) fn remove_cell(&mut self, id: u32) -> Result<(), String> {
        let entity = self.find_cell(id)?;
        self.world
            .despawn(entity)
            .map_err(|_| "Клетка больше не существует".to_string())?;
        self.bonds.retain(|&(a, b)| a != id && b != id);
        self.deaths += 1;
        Ok(())
    }

    pub(super) fn boost_cell(&mut self, id: u32) -> Result<(), String> {
        let entity = self.find_cell(id)?;
        let mut cell = self.world.get::<&mut Cell>(entity).unwrap();
        cell.mass = (cell.mass + 1.).min(1_000_000.);
        Ok(())
    }

    pub(super) fn reposition_cell(&mut self, id: u32, x: f32, y: f32) -> Result<(), String> {
        if !x.is_finite() || !y.is_finite() {
            return Err("Некорректная позиция клетки".into());
        }
        let entity = self.find_cell(id)?;
        let mut position = self.world.get::<&mut Position>(entity).unwrap();
        position.x = x.clamp(12., SIZE - 12.);
        position.y = y.clamp(12., SIZE - 12.);
        Ok(())
    }
}
