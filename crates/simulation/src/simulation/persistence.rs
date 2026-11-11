//! Exact versioned world snapshots. No state is mutated before validation succeeds.

use super::Simulation;
use super::boundary::Boundary;
use super::environment::Environment;
use crate::components::{Cell, INITIAL_NITROGEN, LIMIT, MAX_RECOIL_MOMENTUM, Position, SIZE};
use crate::genome::{Genome, V4Layout, decode_checked, decode_v4_checkpoint, normalize_v4_signal};
use crate::terrain::{GRID, Tile, nitrate_capacity};
use std::collections::{BTreeMap, BTreeSet};

const VERSION: u32 = 6;

fn is_main_v4(values: &[f64]) -> bool {
    // The deployed v4 writes the tile count immediately after fluid density.
    // The resource v4 writes the nitrogen setting (0..5) in that same slot.
    values.first() == Some(&4.) && values.get(17) == Some(&((GRID * GRID) as f64))
}

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
                f64::from(c.recoil[0]),
                f64::from(c.recoil[1]),
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
        if values[0] < 4. || is_main_v4(values) {
            replacement.set_circular_dish(matches!(self.boundary, Boundary::Circle));
        }
        replacement.set_division_event_tracking(self.division_events.is_some());
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
        let main_v4 = is_main_v4(values);
        let has_resources = version >= 4 && !main_v4;
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
            nitrogen: if has_resources {
                input.float(0., 5.)?
            } else {
                1.
            },
        };
        restored.settings.validate()?;
        if has_resources && input.flag()? {
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
            let (nitrates, nitrate_capacity) = if has_resources {
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
                genomes.push(read_genome(&mut input, version, false)?);
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
                nitrogen: if has_resources {
                    input.float(0., INITIAL_NITROGEN)?
                } else {
                    INITIAL_NITROGEN
                },
                ..Cell::default()
            };
            if main_v4 {
                for signal in &mut cell.signals[..4] {
                    *signal = normalize_v4_signal(input.float(-100., 100.)?);
                }
                cell.toxin = input.float(0., 100.)? / 50.;
            }
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
                read_genome(&mut input, version, main_v4)?
            };
            if version >= 6 {
                for component in &mut cell.recoil {
                    let value = input.number()?;
                    if value.abs() > f64::from(MAX_RECOIL_MOMENTUM) {
                        return Err("Импульс клетки вне допустимого диапазона".into());
                    }
                    *component = value as f32;
                }
            }
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

fn read_genome(input: &mut Reader<'_>, version: u32, main_v4: bool) -> Result<Genome, String> {
    let length = input.integer(1_200_000)? as usize;
    if input.values.len().saturating_sub(input.offset) < length {
        return Err("Геном в сохранении обрезан".into());
    }
    let genes = (0..length)
        .map(|_| input.float(-f32::MAX, f32::MAX))
        .collect::<Result<Vec<_>, _>>()?;
    if version == 4 {
        let layout = if main_v4 {
            V4Layout::MainSignals
        } else {
            V4Layout::BranchResources
        };
        return decode_v4_checkpoint(&genes, layout);
    }
    let genome = decode_checked(&genes)?;
    let canonical = match version {
        1 => genome.legacy_values(),
        2 | 3 => genome.values_v3(),
        _ => genome.values(),
    };
    if canonical != genes {
        return Err("Некорректный геном в сохранении".into());
    }
    Ok(genome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell_types::{LIPOCYTE, NEUROCYTE};
    use crate::genome::Mode;

    #[test]
    fn deployed_v4_world_migrates_terrain_cells_bonds_and_signals() {
        let mut checkpoint = vec![
            4.,
            123.,
            456.,
            91.,
            4.,
            3.,
            1.,
            0.,
            1.,
            1.,
            1.,
            1.,
            0.,
            0.,
            1.,
            0.4,
            -0.1,
            (GRID * GRID) as f64,
        ];
        for i in 0..GRID * GRID {
            checkpoint.extend([0.8, 0.2, if i == 0 { 0.4 } else { 0.5 }, 0.5, 2.]);
        }
        checkpoint.push(2.);
        for (id, x, kind, signal_gain) in [(1., 500., 6., 8.), (3., 520., 13., -25.)] {
            checkpoint.extend([
                id,
                x,
                600.,
                0.3,
                0.,
                1.2,
                2.5,
                1.,
                2.,
                0.1,
                1.,
                100.,
                -50.,
                25.,
                -12.5,
                25.,
                24.,
                -4.,
                1.,
                kind,
                2.4,
                0.,
                0.,
                0.5,
                12.,
                0.8,
                240.,
                1.,
                0.,
                0.,
                0.,
                0.,
                0.,
                0.,
                0.,
                -1.,
                -1.,
                -1.,
                2.,
                0.,
                signal_gain,
            ]);
        }
        checkpoint.extend([1., 1., 3.]);

        let mut sim = Simulation::new(5);
        sim.set_circular_dish(true);
        sim.restore_checked(&checkpoint).unwrap();
        assert!(matches!(sim.boundary, Boundary::Circle));
        assert_eq!(sim.rng, 456);
        assert_eq!(sim.tick, 91);
        assert_eq!(sim.settings.nitrogen, 1.);
        assert_eq!(sim.tiles[0].food, 0.4);
        assert_eq!(sim.tiles[0].nitrates, nitrate_capacity(2.));
        assert!(sim.bonds.contains(&(1, 3)));
        for ((_, entity), kind) in sim.entities.iter().zip([LIPOCYTE, NEUROCYTE]) {
            let cell = sim.world.get::<&Cell>(*entity).unwrap();
            let genome = sim.world.get::<&Genome>(*entity).unwrap();
            assert_eq!(genome.mode(cell.mode).kind, kind);
            assert!(cell.mirrored);
            assert_eq!(cell.nitrogen, INITIAL_NITROGEN);
            assert_eq!(cell.signals, [8., -4., 2., -1., 0., 0., 0., 0.]);
            assert_eq!(cell.toxin, 0.5);
            assert_eq!(cell.recoil, [0.; 2]);
        }

        let saved = sim.encode_state();
        let mut replacement = Simulation::new(0);
        replacement.restore_checked(&saved).unwrap();
        assert_eq!(replacement.encode_state(), saved);
        checkpoint.pop();
        assert!(sim.restore_checked(&checkpoint).is_err());
        assert_eq!(sim.encode_state(), saved);
    }

    #[test]
    fn deployed_v5_checkpoint_keeps_shared_genomes_and_bonds_with_zero_recoil() {
        let genes = Genome::from_modes(vec![Mode {
            kind: NEUROCYTE,
            ..Mode::default()
        }])
        .values();
        let mut checkpoint = vec![
            5.,
            123.,
            456.,
            91.,
            3.,
            2.,
            0.,
            0.,
            1.,
            1.,
            0.,
            1.,
            1.,
            0.,
            0.,
            0.,
            0.,
            1.,
            0.,
            (GRID * GRID) as f64,
        ];
        for _ in 0..GRID * GRID {
            checkpoint.extend([1., 0., 0.25, 1., 0., 0.75, 1., 0.125]);
        }
        checkpoint.extend([1., genes.len() as f64]);
        checkpoint.extend(genes.iter().copied().map(f64::from));
        checkpoint.push(2.);
        for (id, x) in [(1., 500.), (2., 522.)] {
            checkpoint.extend([
                id, x, 600., 0.125, 0., 1.25, 2.5, 1., 0., 0.25, 1., 0.75, 1., 2., 3., 4., 5., 6.,
                7., 8., 0.125, 0.25, 9., 1., 0.,
            ]);
        }
        checkpoint.extend([1., 1., 2.]);

        let mut sim = Simulation::new(0);
        sim.restore_checked(&checkpoint).unwrap();
        assert_eq!(sim.tick, 91);
        assert_eq!(sim.tiles[0].covered_food, 0.125);
        assert_eq!(sim.bonds, BTreeSet::from([(1, 2)]));
        assert_eq!(sim.entities.len(), 2);
        for &(_, entity) in &sim.entities {
            let cell = sim.world.get::<&Cell>(entity).unwrap();
            assert_eq!(cell.recoil, [0.; 2]);
            assert_eq!(cell.signals[7], 8.);
            assert!(cell.anchored);
            assert_eq!(sim.world.get::<&Genome>(entity).unwrap().values(), genes);
        }
        assert_eq!(sim.encode_state()[0], 6.);
    }

    #[test]
    fn recoil_round_trips_and_invalid_momentum_does_not_replace_the_world() {
        let mut sim = Simulation::new(42);
        let genes = Genome::from_modes(vec![Mode::default()]).values();
        sim.spawn_checked(500., 600., 0, &genes).unwrap();
        let without_recoil = sim.checksum();
        let recoil = [0.41234567_f32, -0.215625_f32];
        sim.world
            .get::<&mut Cell>(sim.entities[0].1)
            .unwrap()
            .recoil = recoil;
        let saved = sim.encode_state();
        let checksum = sim.checksum();
        assert_ne!(checksum, without_recoil);

        let mut restored = Simulation::new(0);
        restored.restore_checked(&saved).unwrap();
        assert_eq!(restored.encode_state(), saved);
        assert_eq!(restored.checksum(), checksum);
        let restored_cell = restored.world.get::<&Cell>(restored.entities[0].1).unwrap();
        assert_eq!(
            restored_cell.recoil.map(f32::to_bits),
            recoil.map(f32::to_bits)
        );
        drop(restored_cell);

        // With one unbonded cell, the last values are recoil X/Y and bond count.
        let above_limit = 2. * f64::from(MAX_RECOIL_MOMENTUM);
        for index in (saved.len() - 3)..(saved.len() - 1) {
            for invalid in [
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                above_limit,
                -above_limit,
            ] {
                let mut malformed = saved.clone();
                malformed[index] = invalid;
                assert!(restored.restore_checked(&malformed).is_err());
                assert_eq!(restored.checksum(), checksum);
            }
        }

        sim.advance(15);
        restored.advance(15);
        assert_eq!(restored.checksum(), sim.checksum());
    }
}
