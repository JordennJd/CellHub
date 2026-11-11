//! Exact versioned world snapshots. No state is mutated before validation succeeds.

use super::Simulation;
use super::boundary::Boundary;
use super::environment::Environment;
use crate::components::{Cell, INITIAL_NITROGEN, LIMIT, Position, SIZE};
use crate::genome::{Genome, decode_checked};
use crate::terrain::{GRID, Tile, nitrate_capacity};
use std::collections::{BTreeMap, BTreeSet};

const VERSION: u32 = 5;

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
            VERSION as f64,
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
            s.nitrogen as f64,
            matches!(self.boundary, Boundary::Circle) as u8 as f64,
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
                    tile.nitrates,
                    tile.nitrate_capacity,
                    tile.covered_food,
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
        let mut genome_ids = BTreeMap::new();
        let mut genomes = Vec::new();
        let mut references = Vec::with_capacity(cells.len());
        for (_, _, genome) in &cells {
            let genes = genome.values();
            let key = genes.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
            let next_id = genomes.len();
            let id = *genome_ids.entry(key).or_insert_with(|| {
                genomes.push(genes);
                next_id
            });
            references.push(id);
        }
        values.push(genomes.len() as f64);
        for genes in genomes {
            values.push(genes.len() as f64);
            values.extend(genes.into_iter().map(f64::from));
        }
        values.push(cells.len() as f64);
        for ((p, c, _), genome_id) in cells.into_iter().zip(references) {
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
                c.nitrogen as f64,
                c.signals[0] as f64,
                c.signals[1] as f64,
                c.signals[2] as f64,
                c.signals[3] as f64,
                c.signals[4] as f64,
                c.signals[5] as f64,
                c.signals[6] as f64,
                c.signals[7] as f64,
                c.toxin as f64,
                c.injury as f64,
                c.telomeres as f64,
                c.anchored as u8 as f64,
                genome_id as f64,
            ]);
        }
        values.push(self.bonds.len() as f64);
        for &(a, b) in &self.bonds {
            values.extend([a as f64, b as f64]);
        }
        values
    }

    pub(super) fn restore_checked(&mut self, values: &[f64]) -> Result<(), String> {
        let mut replacement = Self::decode_state(values)?;
        // Old saves did not encode geometry. Keep their migration path into
        // the current dish. Current saves restore geometry and coordinates as-is.
        if values[0] < 4. {
            replacement.set_circular_dish(matches!(self.boundary, Boundary::Circle));
        }
        *self = replacement;
        Ok(())
    }

    fn decode_state(values: &[f64]) -> Result<Self, String> {
        if values.len() > 10_000_000 {
            return Err("Сохранение слишком большое".into());
        }
        let mut input = Reader { values, offset: 0 };
        let version = input.integer(u32::MAX)?;
        if !(1..=VERSION).contains(&version) {
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
            nitrogen: if version >= 4 {
                input.float(0., 5.)?
            } else {
                1.
            },
        };
        restored.settings.validate()?;
        if version >= 4 && input.flag()? {
            restored.boundary = Boundary::Circle;
        }
        if input.integer((GRID * GRID) as u32)? as usize != GRID * GRID {
            return Err("Некорректный размер среды".into());
        }
        for tile in &mut restored.tiles {
            let light = input.float(0., 5.)?;
            let radiation = input.float(0., 5.)?;
            let food = input.float(0., 5.)?;
            let capacity = input.float(0., 5.)?;
            let biome = input.integer(3)? as f32;
            let (nitrates, nitrate_capacity) = if version >= 4 {
                (input.float(0., 5.)?, input.float(0., 5.)?)
            } else {
                let capacity = nitrate_capacity(biome);
                (capacity, capacity)
            };
            *tile = Tile {
                light,
                radiation,
                food,
                capacity,
                biome,
                nitrates,
                nitrate_capacity,
                covered_food: if version >= 5 {
                    input.float(0., 5.)?
                } else {
                    0.
                },
            };
            if tile.food > tile.capacity {
                return Err("Запас пищи превышает ёмкость среды".into());
            }
            if tile.nitrates > tile.nitrate_capacity {
                return Err("Запас нитратов превышает ёмкость среды".into());
            }
        }
        let mut genomes = Vec::new();
        if version >= 5 {
            let count = input.integer(LIMIT as u32)?;
            for _ in 0..count {
                genomes.push(read_genome(&mut input, version)?);
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
            let mut cell = Cell {
                id,
                mode: input.integer(u32::MAX)? as usize,
                mass: input.float(f32::MIN_POSITIVE, 1_000_000.)?,
                age: input.float(0., f32::MAX)?,
                generation: input.integer(u32::MAX)?,
                mutations: input.integer(u32::MAX)?,
                cooldown: input.float(0., f32::MAX)?,
                mirrored: if version >= 2 { input.flag()? } else { false },
                nitrogen: if version >= 4 {
                    input.float(0., INITIAL_NITROGEN)?
                } else {
                    INITIAL_NITROGEN
                },
                ..Cell::default()
            };
            let genome = if version >= 5 {
                for signal in &mut cell.signals {
                    *signal = input.float(-1000., 1000.)?;
                }
                cell.toxin = input.float(0., 1000.)?;
                cell.injury = input.float(0., 1000.)?;
                let telomeres = input.float(-1., 1000.)?;
                if telomeres.fract() != 0. {
                    return Err("Некорректный запас теломер".into());
                }
                cell.telomeres = telomeres as i32;
                cell.anchored = input.flag()?;
                let id = input.integer(LIMIT as u32)? as usize;
                genomes
                    .get(id)
                    .cloned()
                    .ok_or("Некорректная ссылка на геном")?
            } else {
                read_genome(&mut input, version)?
            };
            if cell.mode >= genome.len() {
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

fn read_genome(input: &mut Reader<'_>, version: u32) -> Result<Genome, String> {
    let length = input.integer(1_200_000)? as usize;
    if input.values.len().saturating_sub(input.offset) < length {
        return Err("Геном в сохранении обрезан".into());
    }
    let genes = (0..length)
        .map(|_| input.float(-f32::MAX, f32::MAX))
        .collect::<Result<Vec<_>, _>>()?;
    let genome = decode_checked(&genes)?;
    let canonical = match version {
        1 => genome.legacy_values(),
        2 | 3 => genome.values_v3(),
        4 => genome.values_v4(),
        _ => genome.values(),
    };
    if canonical != genes {
        return Err("Некорректный геном в сохранении".into());
    }
    Ok(genome)
}
