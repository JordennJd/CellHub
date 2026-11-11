//! Cell simulation: public wasm ABI and tick orchestration.

mod biology;
mod boundary;
mod environment;
mod spatial;
mod program;
mod signals;
mod collisions;
mod interactions;
mod bonds;
mod specialized;
mod chaos;
mod persistence;
mod tools;
mod step;
mod level;
mod metabolism;
#[cfg(test)]
mod tests;

use crate::cell_types::FLAGELLOCYTE;
use crate::components::{Cell, LIMIT, Position, SNAPSHOT_STRIDE, radius};
use crate::genome::{Genome, decode_checked};
use crate::terrain::{GRID, Tile, map};
use boundary::Boundary;
use environment::Environment;
use hecs::World;
use program::{SPEED_TARGET, controlled_value};
use spatial::SpatialGrid;
use std::collections::{BTreeSet, HashMap, VecDeque};
use wasm_bindgen::prelude::*;

#[cfg(all(target_arch = "wasm32", feature = "wasm-threads"))]
#[wasm_bindgen]
pub fn enable_parallel_simulation() {
    interactions::enable_parallel();
}

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
    division_events: Option<VecDeque<[f32; 8]>>,
    // Reused per-tick scratch storage: no heap churn at the population limit.
    entities: Vec<(u32, hecs::Entity)>,
    spatial: SpatialGrid,
    metabolism: metabolism::MetabolismScratch,
    // Undirected bonds use stable cell IDs, never recycled ECS entity indices.
    bonds: BTreeSet<(u32, u32)>,
    settings: Environment,
    boundary: Boundary,
    level: Option<level::LevelRuntime>,
    /// Per-level population cap from the original substrate (`max_cells`).
    population_limit: usize,
    chaos: Option<chaos::Chaos>,
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
            division_events: None,
            entities: Vec::new(),
            spatial: SpatialGrid::new(),
            metabolism: metabolism::MetabolismScratch::default(),
            bonds: BTreeSet::new(),
            settings: Environment::default(),
            boundary: Boundary::default(),
            level: None,
            population_limit: LIMIT,
            chaos: None,
        }
    }

    pub fn reset(&mut self) {
        if let Some(chaos) = &self.chaos {
            let pool = chaos.pool.clone();
            let initial_pool = chaos.initial_pool.clone();
            let enabled = chaos.enabled;
            let rate = chaos.rate;
            let viable = chaos.viable;
            self.start_chaos_in_bounds(
                self.seed,
                initial_pool,
                self.boundary.width(),
                self.boundary.size(),
                matches!(self.boundary, Boundary::Rectangle { .. }),
            );
            let chaos = self.chaos.as_mut().unwrap();
            chaos.pool = pool;
            chaos.enabled = enabled;
            chaos.rate = rate;
            chaos.viable = viable;
            chaos.next_spawn = 1800 / rate;
            return;
        }
        if let Some(runtime) = &self.level {
            let level = runtime.level.clone();
            if self.install_level(level).is_ok() {
                return;
            }
            self.level = None;
        }
        let boundary = self.boundary;
        let track_divisions = self.division_events.is_some();
        *self = Self::new(self.seed);
        self.boundary = boundary;
        if let Boundary::Rectangle { width, height } = boundary {
            self.set_rectangular_dish(width, height)
                .expect("valid retained bounds");
        }
        self.set_division_event_tracking(track_divisions);
    }

    /// Rectangular worlds keep their physical bounds when a viewport is resized.
    pub fn set_rectangular_dish(&mut self, width: f32, height: f32) -> Result<(), JsError> {
        let base = if self.chaos.is_some() {
            crate::terrain::CHAOS_SIZE
        } else {
            crate::components::SIZE
        };
        if !width.is_finite()
            || height != base
            || width < base * 0.5
            || width > base * 4.
            || width % 64. != 0.
            || !self.entities.is_empty()
        {
            return Err(JsError::new("Некорректный размер нового поля"));
        }
        self.boundary = Boundary::Rectangle { width, height };
        self.spatial = SpatialGrid::with_size(width, height);
        self.tiles = if let Some(c) = &mut self.chaos {
            c.baseline = crate::terrain::chaos_map_rect(
                self.seed,
                (width / 64.) as usize,
                (height / 64.) as usize,
            );
            c.baseline.clone()
        } else {
            crate::terrain::map_rect((width / 64.) as usize, (height / 64.) as usize)
        };
        self.configure_environment(self.settings)
            .map_err(|e| JsError::new(&e))
    }

    pub fn use_rectangular_geometry(&mut self) {
        self.boundary = Boundary::Rectangle {
            width: self.boundary.width(),
            height: self.boundary.size(),
        };
        self.spatial = SpatialGrid::with_size(self.boundary.width(), self.boundary.size());
    }

    /// Opt in for the local laboratory without changing the native server map.
    pub fn set_circular_dish(&mut self, enabled: bool) {
        if self.chaos.is_some() {
            self.boundary = Boundary::Chaos;
            return;
        }
        self.boundary = if enabled {
            Boundary::Circle
        } else {
            Boundary::Square
        };
        if enabled {
            for (_, (position, cell)) in self.world.query_mut::<(&mut Position, &Cell)>() {
                (position.x, position.y, _) = self
                    .boundary
                    .constrain(position.x, position.y, cell.mass, 12.);
            }
        }
    }

    /// Remove living cells while retaining the current substrate, time and settings.
    pub fn sterilize(&mut self) {
        if let Some(chaos) = &mut self.chaos {
            chaos.event(self.tick, 5, self.world.len());
        }
        self.deaths += self.world.len();
        self.world.clear();
        self.entities.clear();
        self.spatial.clear();
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
        nitrogen: Option<f32>,
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
            nitrogen: nitrogen.unwrap_or(1.),
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
            s.nitrogen,
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

    /// Load a self-contained campaign manifest. The world is replaced only
    /// after the whole level validates.
    pub fn load_level_json(&mut self, text: &str) -> Result<(), JsError> {
        self.load_level(text).map_err(|error| JsError::new(&error))
    }

    /// Serialized campaign progress for the objective panel and tests.
    pub fn level_status(&self) -> String {
        self.level_status_snapshot().to_json()
    }

    /// Play the level's authored demo track from the current state.
    pub fn start_level_demo(&mut self) -> Result<(), JsError> {
        self.start_demo().map_err(|error| JsError::new(&error))
    }

    pub fn stop_level_demo(&mut self) {
        self.stop_demo();
    }

    pub fn spawn(&mut self, x: f32, y: f32, mode: usize, values: &[f32]) -> Result<u32, JsError> {
        self.spawn_checked(x, y, mode, values)
            .map_err(|error| JsError::new(&error))
    }

    pub fn edit(&mut self, id: u32, mode: usize, values: &[f32]) -> Result<(), JsError> {
        self.edit_checked(id, mode, values)
            .map_err(|error| JsError::new(&error))
    }

    /// Export the genome this cell will pass to its offspring.
    pub fn genome(&self, id: u32) -> Vec<f32> {
        for (_, (c, g)) in self.world.query::<(&Cell, &Genome)>().iter() {
            if c.id == id {
                return g.offspring().values();
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

    pub fn tick_number(&self) -> u32 {
        self.tick
    }

    /// Capture visual events for consumers that render local simulation frames.
    pub fn set_division_event_tracking(&mut self, enabled: bool) {
        if enabled {
            self.division_events.get_or_insert_with(VecDeque::new);
        } else {
            self.division_events = None;
        }
    }

    /// Drain rows of tick, parent ID, daughter IDs and the parent's pose at division.
    pub fn take_division_events(&mut self) -> Vec<f32> {
        self.division_events
            .as_mut()
            .map_or_else(Vec::new, |events| events.drain(..).flatten().collect())
    }

    /// Fingerprint the canonical checkpoint for replay tests and diagnostics.
    pub fn checksum(&self) -> String {
        let hash = self
            .encode_state()
            .into_iter()
            .fold(0xcbf29ce484222325_u64, |hash, value| {
                value.to_le_bytes().into_iter().fold(hash, |hash, byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
                })
            });
        format!("{hash:016x}")
    }

    pub fn snapshot(&mut self) -> Vec<f32> {
        self.frame.clear();
        let mut rows: Vec<(u32, [f32; SNAPSHOT_STRIDE])> = self
            .world
            .query::<(&Position, &Cell, &Genome)>()
            .iter()
            .map(|(_, (p, c, g))| {
                (
                    c.id,
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
                        packed_color(g.mode(c.mode).color),
                        c.mirrored as u8 as f32,
                    ],
                )
            })
            .collect();
        rows.sort_unstable_by_key(|(id, _)| *id);
        for (_, row) in rows {
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

    pub fn nitrogen_reserves(&self) -> Vec<f32> {
        self.entities
            .iter()
            .map(|&(_, entity)| self.world.get::<&Cell>(entity).unwrap().nitrogen)
            .collect()
    }

    pub fn cell_details(&self) -> Vec<f32> {
        self.entities
            .iter()
            .flat_map(|&(_, entity)| {
                let cell = self.world.get::<&Cell>(entity).unwrap();
                [
                    cell.signals[0],
                    cell.signals[1],
                    cell.signals[2],
                    cell.signals[3],
                    cell.signals[4],
                    cell.signals[5],
                    cell.signals[6],
                    cell.signals[7],
                    cell.toxin,
                    cell.injury,
                    cell.telomeres as f32,
                    cell.mode as f32,
                    cell.anchored as u8 as f32,
                ]
            })
            .collect()
    }

    pub fn covered_food(&self) -> Vec<f32> {
        self.tiles.iter().map(|tile| tile.covered_food).collect()
    }

    pub fn nitrates(&self) -> Vec<f32> {
        self.tiles.iter().map(|tile| tile.nitrates).collect()
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
        let genome = decode_checked(values)?;
        if mode >= genome.len() {
            return Err("Invalid spawn archetype".to_owned());
        }
        if self.world.len() as usize >= self.population_limit {
            return Err(format!("Population limit: {}", self.population_limit));
        }
        let id = self.next_id;
        self.next_id += 1;
        let angle = self.random() * std::f32::consts::TAU;
        let (x, y, _) = self.boundary.constrain(x, y, 1., 10.);
        let entity = self
            .world
            .spawn((Position { x, y, angle }, Cell::new(id, mode), genome));
        self.entities.push((id, entity));
        self.births += 1;
        Ok(id)
    }

    pub fn edit_checked(&mut self, id: u32, mode: usize, values: &[f32]) -> Result<(), String> {
        let genome = decode_checked(values)?;
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
        draw_random(&mut self.rng)
    }

    /// Native-only region export used by the authoritative server.
    ///
    /// Cells are emitted with stride 28, including the effective world speed
    /// used directly for client-side prediction. Each bond is emitted once, in
    /// the chunk of its lower ID, so a client can replace chunk-local links
    /// without duplicates.
    pub fn chunk_snapshot(&self, chunk_x: u32, chunk_y: u32, chunk_size: f32) -> ChunkSnapshot {
        let mut links = Vec::new();
        let mut terrain = Vec::new();
        let mut query = self.world.query::<(&Position, &Cell, &Genome)>();
        let visible: Vec<_> = query
            .iter()
            .map(|(_, row)| row)
            .filter(|(position, _, _)| {
                (position.x / chunk_size) as u32 == chunk_x
                    && (position.y / chunk_size) as u32 == chunk_y
            })
            .collect();
        let mut degrees = HashMap::new();
        if !self.bonds.is_empty() && !visible.is_empty() {
            degrees.extend(visible.iter().map(|(_, cell, _)| (cell.id, 0)));
            for &(a, b) in &self.bonds {
                if let Some(degree) = degrees.get_mut(&a) {
                    *degree += 1;
                    if self
                        .entities
                        .binary_search_by_key(&b, |&(id, _)| id)
                        .is_ok()
                    {
                        links.extend([a, b]);
                    }
                }
                if let Some(degree) = degrees.get_mut(&b) {
                    *degree += 1;
                }
            }
        }
        let mut cells = Vec::with_capacity(visible.len() * 28);
        for (position, cell, genome) in visible {
            let mode = genome.mode(cell.mode);
            let degree = degrees.get(&cell.id).copied().unwrap_or(0);
            let is_linked = degree != 0;
            let programmed_speed = controlled_value(cell, mode, SPEED_TARGET, degree);
            let speed = if cell.anchored {
                0.
            } else if mode.kind == FLAGELLOCYTE {
                programmed_speed
            } else if is_linked {
                0.
            } else {
                programmed_speed * 0.15
            } / self.settings.viscosity;
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
                packed_color(mode.color),
                cell.mirrored as u8 as f32,
                cell.nitrogen,
                cell.signals[0],
                cell.signals[1],
                cell.signals[2],
                cell.signals[3],
                cell.signals[4],
                cell.signals[5],
                cell.signals[6],
                cell.signals[7],
                cell.toxin,
                cell.injury,
                cell.telomeres as f32,
                cell.mode as f32,
                cell.anchored as u8 as f32,
            ]);
        }
        let tiles_per_chunk = (chunk_size / 64.) as usize;
        let start_x = chunk_x as usize * tiles_per_chunk;
        let start_y = chunk_y as usize * tiles_per_chunk;
        for y in start_y..(start_y + tiles_per_chunk).min(GRID) {
            for x in start_x..(start_x + tiles_per_chunk).min(GRID) {
                let tile = self.tiles[y * GRID + x];
                terrain.extend([
                    tile.light,
                    tile.radiation,
                    tile.food,
                    tile.biome,
                    tile.nitrates,
                    tile.covered_food,
                ]);
            }
        }
        ChunkSnapshot {
            cells,
            links,
            terrain,
        }
    }
}

fn packed_color(color: [f32; 3]) -> f32 {
    if color[0] < 0. {
        return -1.;
    }
    let [r, g, b] = color.map(|v| (v * 255.).round() as u32);
    ((r << 16) | (g << 8) | b) as f32
}

/// One server-synchronized map chunk.
pub struct ChunkSnapshot {
    pub cells: Vec<f32>,
    pub links: Vec<u32>,
    pub terrain: Vec<f32>,
}

fn draw_random(rng: &mut u32) -> f32 {
    let mut x = *rng;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *rng = x;
    (x as f64 / u32::MAX as f64) as f32
}
