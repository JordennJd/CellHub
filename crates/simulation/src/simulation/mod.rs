//! Cell simulation: public wasm ABI and tick orchestration.

mod bonds;
mod spatial;
mod step;
#[cfg(test)]
mod tests;

use crate::components::{Cell, LIMIT, Position, SIZE, radius};
use crate::genome::{Genome, decode};
use crate::terrain::{GRID, Tile, map};
use hecs::World;
use spatial::SpatialGrid;
use std::collections::{BTreeSet, HashMap};
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
    // Reused per-tick scratch storage: no heap churn at the population limit.
    entities: Vec<(u32, hecs::Entity)>,
    spatial: SpatialGrid,
    // Undirected bonds use stable cell IDs, never recycled ECS entity indices.
    bonds: BTreeSet<(u32, u32)>,
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
            entities: Vec::new(),
            spatial: SpatialGrid::new(),
            bonds: BTreeSet::new(),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.seed);
    }

    pub fn spawn(&mut self, x: f32, y: f32, mode: usize, values: &[f32]) -> Result<u32, JsError> {
        self.spawn_checked(x, y, mode, values)
            .map_err(|error| JsError::new(&error))
    }

    pub fn edit(&mut self, id: u32, mode: usize, values: &[f32]) -> Result<(), JsError> {
        self.edit_checked(id, mode, values)
            .map_err(|error| JsError::new(&error))
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
        let mut rows: Vec<_> = self
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
    /// Native validation path: the server wants a plain message it can put on
    /// the wire, while the wasm ABI keeps `JsError`.
    pub fn spawn_checked(
        &mut self,
        x: f32,
        y: f32,
        mode: usize,
        values: &[f32],
    ) -> Result<u32, String> {
        if !x.is_finite() || !y.is_finite() {
            return Err("Invalid spawn".to_owned());
        }
        let genome = decode(values)?;
        if mode >= genome.len() {
            return Err("Invalid spawn archetype".to_owned());
        }
        if self.world.len() as usize >= LIMIT {
            return Err(format!("Population limit: {LIMIT}"));
        }
        let id = self.next_id;
        self.next_id += 1;
        let angle = self.random() * std::f32::consts::TAU;
        let entity = self.world.spawn((
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
            },
            genome,
        ));
        self.entities.push((id, entity));
        self.births += 1;
        Ok(id)
    }

    pub fn edit_checked(&mut self, id: u32, mode: usize, values: &[f32]) -> Result<(), String> {
        let genome = decode(values)?;
        if mode >= genome.len() {
            return Err("Invalid mode".to_owned());
        }
        for (_, (cell, g)) in self.world.query_mut::<(&mut Cell, &mut Genome)>() {
            if cell.id == id {
                *g = genome;
                cell.mode = mode;
                return Ok(());
            }
        }
        Err("Cell no longer exists".to_owned())
    }

    fn random(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f64 / u32::MAX as f64) as f32
    }

    /// Native-only region export used by the authoritative server.
    ///
    /// Cells are emitted with stride 12: the public render stride 10 plus the
    /// effective speed used for client-side prediction and a flag bit for
    /// adhesion. Each bond is emitted once, in the chunk of its lower ID, so a
    /// client can replace chunk-local links without duplicates.
    pub fn chunk_snapshot(&self, chunk_x: u32, chunk_y: u32, chunk_size: f32) -> ChunkSnapshot {
        let mut cells = Vec::new();
        let mut links = Vec::new();
        let mut terrain = Vec::new();
        let mut cell_chunks = HashMap::new();
        let linked = self.linked_ids();
        for (_, (position, cell, genome)) in
            self.world.query::<(&Position, &Cell, &Genome)>().iter()
        {
            let cx = (position.x / chunk_size) as u32;
            let cy = (position.y / chunk_size) as u32;
            cell_chunks.insert(cell.id, (cx, cy));
            if cx != chunk_x || cy != chunk_y {
                continue;
            }
            let mode = genome.mode(cell.mode);
            let is_linked = linked.contains(&cell.id);
            let speed = if mode.kind == 2 {
                mode.speed
            } else if is_linked {
                0.
            } else {
                mode.speed * 0.15
            };
            cells.extend([
                cell.id as f32,
                position.x,
                position.y,
                radius(cell.mass),
                mode.kind as f32,
                cell.mass,
                cell.age,
                cell.generation as f32,
                cell.mutations as f32,
                position.angle,
                speed,
                is_linked as u8 as f32,
            ]);
        }
        for &(a, b) in &self.bonds {
            if let (Some(&(ax, ay)), Some(_)) = (cell_chunks.get(&a), cell_chunks.get(&b))
                && (ax, ay) == (chunk_x, chunk_y)
            {
                links.extend([a, b]);
            }
        }
        let tiles_per_chunk = (chunk_size / 64.) as usize;
        let start_x = chunk_x as usize * tiles_per_chunk;
        let start_y = chunk_y as usize * tiles_per_chunk;
        for y in start_y..(start_y + tiles_per_chunk).min(GRID) {
            for x in start_x..(start_x + tiles_per_chunk).min(GRID) {
                let tile = self.tiles[y * GRID + x];
                terrain.extend([tile.light, tile.radiation, tile.food, tile.biome]);
            }
        }
        ChunkSnapshot {
            cells,
            links,
            terrain,
        }
    }
}

/// One server-synchronized map chunk.
pub struct ChunkSnapshot {
    pub cells: Vec<f32>,
    pub links: Vec<u32>,
    pub terrain: Vec<f32>,
}
