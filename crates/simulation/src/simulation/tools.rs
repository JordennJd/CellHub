//! Laboratory interventions, independent of genome and time controls.

use super::Simulation;
use crate::components::{Cell, Position};
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
        self.entities.retain(|&(cell_id, _)| cell_id != id);
        self.bonds.retain(|&(a, b)| a != id && b != id);
        self.deaths += 1;
        Ok(())
    }

    pub(super) fn boost_cell(&mut self, id: u32) -> Result<(), String> {
        let entity = self.find_cell(id)?;
        let (cell, position) = self
            .world
            .query_one_mut::<(&mut Cell, &mut Position)>(entity)
            .unwrap();
        cell.mass = (cell.mass + 1.).min(1_000_000.);
        if matches!(self.boundary, super::boundary::Boundary::Circle) {
            (position.x, position.y, _) = self
                .boundary
                .constrain(position.x, position.y, cell.mass, 12.);
        }
        Ok(())
    }

    pub(super) fn reposition_cell(&mut self, id: u32, x: f32, y: f32) -> Result<(), String> {
        if !x.is_finite() || !y.is_finite() {
            return Err("Некорректная позиция клетки".into());
        }
        let entity = self.find_cell(id)?;
        let (position, cell) = self
            .world
            .query_one_mut::<(&mut Position, &Cell)>(entity)
            .unwrap();
        (position.x, position.y, _) = self.boundary.constrain(x, y, cell.mass, 12.);
        Ok(())
    }
}
