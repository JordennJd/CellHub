//! Exact versioned world snapshots. No state is mutated before validation succeeds.

use super::Simulation;
use super::environment::Environment;
use crate::components::{Cell, LIMIT, Position, SIZE};
use crate::genome::{Genome, decode_checked};
use crate::terrain::{GRID, Tile};
use std::collections::BTreeSet;

struct Reader<'a> {
    values: &'a [f64],
    offset: usize,
}

impl Reader<'_> {
    fn number(&mut self) -> Result<f64, String> {
        let value = *self.values.get(self.offset).ok_or("Сохранение обрезано")?;
        self.offset += 1;
        if !value.is_finite() {
            return Err("Сохранение содержит недопустимое число".into());
        }
        Ok(value)
    }

    fn integer(&mut self, max: u32) -> Result<u32, String> {
        let value = self.number()?;
        if value < 0. || value > max as f64 || value.fract() != 0. {
            return Err("Некорректное целое число в сохранении".into());
        }
        Ok(value as u32)
    }

    fn float(&mut self, min: f32, max: f32) -> Result<f32, String> {
        let value = self.number()?;
        let value32 = value as f32;
        if !value32.is_finite() || value32 < min || value32 > max {
            return Err("Значение вне допустимого диапазона сохранения".into());
        }
        Ok(value32)
    }

    fn flag(&mut self) -> Result<bool, String> {
        Ok(self.integer(1)? == 1)
    }
}

impl Simulation {
    pub(super) fn encode_state(&self) -> Vec<f64> {
        let s = self.settings;
        let mut values = vec![
            3.,
            self.seed as f64,
            self.rng as f64,
            self.tick as f64,
            self.next_id as f64,
            self.births as f64,
            self.deaths as f64,
            self.infections as f64,
            s.light as f64,
            s.food as f64,
            s.radiation as f64,
            s.viscosity as f64,
            s.aging as u8 as f64,
            s.kill_at_edge as u8 as f64,
            s.only_point_mutations as u8 as f64,
            s.gravity as f64,
            s.density as f64,
            self.tiles.len() as f64,
        ];
        for tile in &self.tiles {
            values.extend(
                [
                    tile.light,
                    tile.radiation,
                    tile.food,
                    tile.capacity,
                    tile.biome,
                ]
                .map(f64::from),
            );
        }
        let mut cells: Vec<_> = self
            .world
            .query::<(&Position, &Cell, &Genome)>()
            .iter()
            .map(|(_, (p, c, g))| (*p, c.clone(), g.clone()))
            .collect();
        cells.sort_by_key(|(_, c, _)| c.id);
        values.push(cells.len() as f64);
        for (p, c, genome) in cells {
            values.extend([
                c.id as f64,
                p.x as f64,
                p.y as f64,
                p.angle as f64,
                c.mode as f64,
                c.mass as f64,
                c.age as f64,
                c.generation as f64,
                c.mutations as f64,
                c.cooldown as f64,
                c.mirrored as u8 as f64,
            ]);
            let genes = genome.values();
            values.push(genes.len() as f64);
            values.extend(genes.into_iter().map(f64::from));
        }
        values.push(self.bonds.len() as f64);
        for &(a, b) in &self.bonds {
            values.extend([a as f64, b as f64]);
        }
        values
    }

    pub(super) fn restore_checked(&mut self, values: &[f64]) -> Result<(), String> {
        let mut replacement = Self::decode_state(values)?;
        replacement.set_circular_dish(matches!(self.boundary, super::boundary::Boundary::Circle));
        *self = replacement;
        Ok(())
    }

    fn decode_state(values: &[f64]) -> Result<Self, String> {
        if values.len() > 10_000_000 {
            return Err("Сохранение слишком большое".into());
        }
        let mut input = Reader { values, offset: 0 };
        let version = input.integer(u32::MAX)?;
        if !(1..=3).contains(&version) {
            return Err("Неподдерживаемая версия сохранения".into());
        }
        let seed = input.integer(u32::MAX)?;
        let mut restored = Self::new(seed);
        restored.rng = input.integer(u32::MAX)?;
        restored.tick = input.integer(u32::MAX)?;
        restored.next_id = input.integer(u32::MAX)?;
        restored.births = input.integer(u32::MAX)?;
        restored.deaths = input.integer(u32::MAX)?;
        restored.infections = input.integer(u32::MAX)?;
        if restored.rng == 0 || restored.next_id == 0 {
            return Err("Некорректное состояние генератора или идентификаторов".into());
        }
        restored.settings = Environment {
            light: input.float(0., 5.)?,
            food: input.float(0., 5.)?,
            radiation: input.float(0., 5.)?,
            viscosity: input.float(0.1, 5.)?,
            aging: input.flag()?,
            kill_at_edge: input.flag()?,
            only_point_mutations: if version >= 3 { input.flag()? } else { false },
            gravity: if version >= 3 {
                input.float(0., 1.)?
            } else {
                0.
            },
            density: if version >= 3 {
                input.float(-3.3, 3.3)?
            } else {
                0.
            },
        };
        restored.settings.validate()?;
        if input.integer((GRID * GRID) as u32)? as usize != GRID * GRID {
            return Err("Некорректный размер среды".into());
        }
        for tile in &mut restored.tiles {
            *tile = Tile {
                light: input.float(0., 5.)?,
                radiation: input.float(0., 5.)?,
                food: input.float(0., 5.)?,
                capacity: input.float(0., 5.)?,
                biome: input.integer(3)? as f32,
            };
            if tile.food > tile.capacity {
                return Err("Запас пищи превышает ёмкость среды".into());
            }
        }
        let count = input.integer(LIMIT as u32)?;
        let mut ids = BTreeSet::new();
        for _ in 0..count {
            let id = input.integer(u32::MAX)?;
            if id == 0 || id >= restored.next_id || !ids.insert(id) {
                return Err("Некорректный идентификатор клетки".into());
            }
            let position = Position {
                x: input.float(0., SIZE)?,
                y: input.float(0., SIZE)?,
                angle: input.float(-f32::MAX, f32::MAX)?,
            };
            let cell = Cell {
                id,
                mode: input.integer(u32::MAX)? as usize,
                mass: input.float(f32::MIN_POSITIVE, 1_000_000.)?,
                age: input.float(0., f32::MAX)?,
                generation: input.integer(u32::MAX)?,
                mutations: input.integer(u32::MAX)?,
                cooldown: input.float(0., f32::MAX)?,
                mirrored: if version >= 2 { input.flag()? } else { false },
            };
            let length = input.integer(1_200_000)? as usize;
            if input.values.len().saturating_sub(input.offset) < length {
                return Err("Геном в сохранении обрезан".into());
            }
            let genes = (0..length)
                .map(|_| input.float(-f32::MAX, f32::MAX))
                .collect::<Result<Vec<_>, _>>()?;
            let genome = decode_checked(&genes)?;
            // The ordinary input ABI clamps sliders; stored state must never be silently changed.
            if (if version == 1 {
                genome.legacy_values()
            } else {
                genome.values()
            }) != genes
                || cell.mode >= genome.len()
            {
                return Err("Некорректный геном или режим клетки в сохранении".into());
            }
            let entity = restored.world.spawn((position, cell, genome));
            restored.entities.push((id, entity));
        }
        let count = input.integer(1_000_000)?;
        for _ in 0..count {
            let a = input.integer(u32::MAX)?;
            let b = input.integer(u32::MAX)?;
            if a >= b || !ids.contains(&a) || !ids.contains(&b) || !restored.bonds.insert((a, b)) {
                return Err("Некорректная связь клеток в сохранении".into());
            }
        }
        if input.offset != values.len() {
            return Err("Лишние данные в сохранении".into());
        }
        restored.entities.sort_by_key(|&(id, _)| id);
        Ok(restored)
    }
}
