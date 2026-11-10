//! Cell simulation: public wasm ABI and tick orchestration.

mod bonds;
mod biology;
mod environment;
mod persistence;
mod tools;
mod step;
#[cfg(test)]
mod tests;

use crate::components::{Cell, LIMIT, Position, SIZE, SNAPSHOT_STRIDE, radius};
use crate::genome::{Genome, decode};
use crate::terrain::{Tile, map};
use environment::Environment;
use hecs::World;
use std::collections::BTreeSet;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Simulation {
    world: World,
    tiles: Vec<Tile>,
    tick: u32,
    next_id: u32,
    rng: u32,
    seed: u32,
    births: u32,
    deaths: u32,
    infections: u32,
    frame: Vec<f32>,
    // Undirected bonds use stable cell IDs, never recycled ECS entity indices.
    bonds: BTreeSet<(u32, u32)>,
    settings: Environment,
}

#[wasm_bindgen]
impl Simulation {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32) -> Self {
        Self {
            world: World::new(),
            tiles: map(),
            tick: 0,
            next_id: 1,
            rng: seed.max(1),
            seed,
            births: 0,
            deaths: 0,
            infections: 0,
            frame: Vec::new(),
            bonds: BTreeSet::new(),
            settings: Environment::default(),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.seed);
    }

    /// Remove living cells while retaining the current substrate, time and settings.
    pub fn sterilize(&mut self) {
        self.deaths += self.world.len();
        self.world.clear();
        self.bonds.clear();
        self.frame.clear();
    }

    pub fn configure(
        &mut self,
        light: f32,
        food: f32,
        radiation: f32,
        viscosity: f32,
        aging: bool,
        kill_at_edge: bool,
        only_point_mutations: Option<bool>,
        gravity: Option<f32>,
        density: Option<f32>,
    ) -> Result<(), JsError> {
        self.configure_environment(Environment {
            light,
            food,
            radiation,
            viscosity,
            aging,
            kill_at_edge,
            only_point_mutations: only_point_mutations.unwrap_or(false),
            gravity: gravity.unwrap_or(0.),
            density: density.unwrap_or(0.),
        })
        .map_err(|error| JsError::new(&error))
    }

    pub fn environment(&self) -> Vec<f32> {
        let s = self.settings;
        vec![
            s.light,
            s.food,
            s.radiation,
            s.viscosity,
            s.aging as u8 as f32,
            s.kill_at_edge as u8 as f32,
            s.only_point_mutations as u8 as f32,
            s.gravity,
            s.density,
        ]
    }

    pub fn remove(&mut self, id: u32) -> Result<(), JsError> {
        self.remove_cell(id).map_err(|error| JsError::new(&error))
    }

    pub fn boost(&mut self, id: u32) -> Result<(), JsError> {
        self.boost_cell(id).map_err(|error| JsError::new(&error))
    }

    pub fn move_cell(&mut self, id: u32, x: f32, y: f32) -> Result<(), JsError> {
        self.reposition_cell(id, x, y)
            .map_err(|error| JsError::new(&error))
    }

    /// Versioned exact numeric state; the worker provides the JSON envelope.
    pub fn export_state(&self) -> Vec<f64> {
        self.encode_state()
    }

    /// Validate into a new world first, then atomically replace the current world.
    pub fn restore_state(&mut self, values: &[f64]) -> Result<(), JsError> {
        self.restore_checked(values)
            .map_err(|error| JsError::new(&error))
    }

    pub fn spawn(&mut self, x: f32, y: f32, mode: usize, values: &[f32]) -> Result<u32, JsError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(JsError::new("Invalid spawn"));
        }
        let genome = decode(values)?;
        if mode >= genome.len() {
            return Err(JsError::new("Invalid spawn archetype"));
        }
        if self.world.len() as usize >= LIMIT {
            return Err(JsError::new(&format!("Population limit: {LIMIT}")));
        }
        let id = self.next_id;
        self.next_id += 1;
        let angle = self.random() * std::f32::consts::TAU;
        self.world.spawn((
            Position {
                x: x.clamp(10., SIZE - 10.),
                y: y.clamp(10., SIZE - 10.),
                angle,
            },
            Cell {
                id,
                mode,
                mass: 1.,
                age: 0.,
                generation: 0,
                mutations: 0,
                cooldown: 0.,
                mirrored: false,
            },
            genome,
        ));
        self.births += 1;
        Ok(id)
    }

    pub fn edit(&mut self, id: u32, mode: usize, values: &[f32]) -> Result<(), JsError> {
        let genome = decode(values)?;
        if mode >= genome.len() {
            return Err(JsError::new("Invalid mode"));
        }
        for (_, (cell, g)) in self.world.query_mut::<(&mut Cell, &mut Genome)>() {
            if cell.id == id {
                *g = genome;
                cell.mode = mode;
                return Ok(());
            }
        }
        Err(JsError::new("Cell no longer exists"))
    }

    pub fn genome(&self, id: u32) -> Vec<f32> {
        for (_, (c, g)) in self.world.query::<(&Cell, &Genome)>().iter() {
            if c.id == id {
                return g.values();
            }
        }
        Vec::new()
    }

    pub fn cell_mode(&self, id: u32) -> usize {
        self.world
            .query::<&Cell>()
            .iter()
            .find(|(_, c)| c.id == id)
            .map(|(_, c)| c.mode)
            .unwrap_or(0)
    }

    pub fn advance(&mut self, steps: u32) {
        for _ in 0..steps.min(120) {
            self.step();
        }
    }

    pub fn snapshot(&mut self) -> Vec<f32> {
        self.frame.clear();
        let mut rows: Vec<[f32; SNAPSHOT_STRIDE]> = self
            .world
            .query::<(&Position, &Cell, &Genome)>()
            .iter()
            .map(|(_, (p, c, g))| {
                [
                    c.id as f32,
                    p.x,
                    p.y,
                    radius(c.mass),
                    g.mode(c.mode).kind as f32,
                    c.mass,
                    c.age,
                    c.generation as f32,
                    c.mutations as f32,
                    p.angle,
                    if g.mode(c.mode).color[0] < 0. {
                        -1.
                    } else {
                        let [r, green, b] = g.mode(c.mode).color.map(|v| (v * 255.).round() as u32);
                        ((r << 16) | (green << 8) | b) as f32
                    },
                    c.mirrored as u8 as f32,
                ]
            })
            .collect();
        rows.sort_by_key(|r| r[0] as u32);
        for row in rows {
            self.frame.extend(row);
        }
        self.frame.clone()
    }

    pub fn terrain(&self) -> Vec<f32> {
        self.tiles
            .iter()
            .flat_map(|t| [t.light, t.radiation, t.food, t.biome])
            .collect()
    }

    pub fn links(&self) -> Vec<u32> {
        self.bonds.iter().flat_map(|&(a, b)| [a, b]).collect()
    }

    pub fn stats(&self) -> Vec<f32> {
        vec![
            self.tick as f32,
            self.world.len() as f32,
            self.births as f32,
            self.deaths as f32,
            self.infections as f32,
        ]
    }
}

impl Simulation {
    fn random(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f64 / u32::MAX as f64) as f32
    }
}
