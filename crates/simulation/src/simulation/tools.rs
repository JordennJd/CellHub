//! Laboratory interventions, independent of genome and time controls.

use super::Simulation;
use super::level::LevelEvent;
use crate::components::{Cell, INITIAL_NITROGEN, Position};
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
        self.check_level_tool("remove")?;
        if self.level.as_ref().is_some_and(|runtime| {
            runtime
                .level
                .remove_max
                .is_some_and(|max| runtime.removed >= max)
        }) {
            return Err("Исчерпан лимит удалений".into());
        }
        let entity = self.find_cell(id)?;
        let event = self
            .world
            .query_one_mut::<(&Position, &Cell)>(entity)
            .ok()
            .map(|(position, cell)| {
                LevelEvent::new(self.tick, "remove")
                    .with_cell(cell)
                    .with_position(position)
            });
        self.world
            .despawn(entity)
            .map_err(|_| "Клетка больше не существует".to_string())?;
        self.entities.retain(|&(cell_id, _)| cell_id != id);
        self.bonds.retain(|&(a, b)| a != id && b != id);
        self.deaths += 1;
        if let Some(runtime) = &mut self.level {
            runtime.removed += 1;
        }
        if let Some(event) = event {
            self.emit_level_event(event);
        }
        Ok(())
    }

    pub(super) fn boost_cell(&mut self, id: u32) -> Result<(), String> {
        self.check_level_tool("add")?;
        let entity = self.find_cell(id)?;
        let event = {
            let (cell, position) = self
                .world
                .query_one_mut::<(&mut Cell, &mut Position)>(entity)
                .unwrap();
            cell.mass = (cell.mass + 1.).min(1_000_000.);
            cell.nitrogen = INITIAL_NITROGEN;
            if matches!(self.boundary, super::boundary::Boundary::Circle) {
                (position.x, position.y, _) = self
                    .boundary
                    .constrain(position.x, position.y, cell.mass, 12.);
            }
            LevelEvent::new(self.tick, "boost")
                .with_cell(cell)
                .with_position(position)
        };
        self.emit_level_event(event);
        Ok(())
    }

    pub(super) fn reposition_cell(&mut self, id: u32, x: f32, y: f32) -> Result<(), String> {
        self.check_level_tool("move")?;
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
