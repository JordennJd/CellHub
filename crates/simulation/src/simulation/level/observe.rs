//! Diagnostic observation for campaign levels.
//!
//! The schema and the win/lose logic deliberately stay small. Everything an
//! author (or an automated solver) needs in order to understand *why* a demo
//! does not finish lives here: an event ring buffer fed by the simulation and
//! two census views over the live substrate (aggregated and per-cell).
//!
//! Observation is passive: it never changes the RNG stream or the tick order.

use super::{LevelRuntime, STEP, is_black, is_red, is_yellow};
use crate::components::{Cell, Position, SIZE, radius};
use crate::genome::Genome;
use crate::simulation::Simulation;
use serde::Serialize;
use std::collections::BTreeMap;

/// Bounded so a long level cannot grow the wasm heap without limit. Events are
/// transitions, not per-tick samples, so the cap is generous.
pub(crate) const EVENT_CAPACITY: usize = 4096;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LevelEvent {
    pub(crate) tick: u32,
    pub(crate) time: f32,
    pub(crate) kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cell: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parent: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) class: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mode: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mass: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) nitrogen: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) detail: Option<String>,
}

impl LevelEvent {
    pub(crate) fn new(tick: u32, kind: &'static str) -> Self {
        Self {
            tick,
            time: tick as f32 * STEP,
            kind,
            cell: None,
            parent: None,
            class: None,
            mode: None,
            color: None,
            x: None,
            y: None,
            mass: None,
            nitrogen: None,
            detail: None,
        }
    }

    pub(crate) fn with_cell(mut self, cell: &Cell) -> Self {
        self.set_cell(cell);
        self
    }

    pub(crate) fn set_cell(&mut self, cell: &Cell) {
        self.cell = Some(cell.id);
        self.class = Some(cell.class);
        self.mode = Some(cell.mode);
        self.mass = Some(cell.mass);
        self.nitrogen = Some(cell.nitrogen);
    }

    pub(crate) fn with_position(mut self, position: &Position) -> Self {
        self.x = Some(position.x / SIZE);
        self.y = Some(position.y / SIZE);
        self
    }

    pub(crate) fn with_color(mut self, genome: &Genome, mode: usize) -> Self {
        self.color = Some(genome.mode(mode).resolved_color());
        self
    }

    pub(crate) fn with_parent(mut self, parent: u32) -> Self {
        self.parent = Some(parent);
        self
    }

    pub(crate) fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl LevelRuntime {
    pub(crate) fn push_event(&mut self, event: LevelEvent) {
        if self.events.len() == EVENT_CAPACITY {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub(crate) fn events_json(&self) -> String {
        serde_json::to_string(&self.events.iter().collect::<Vec<_>>())
            .unwrap_or_else(|_| "[]".to_owned())
    }
}

impl Simulation {
    /// Record a diagnostic event when a level is installed. No-op otherwise.
    pub(crate) fn emit_level_event(&mut self, event: LevelEvent) {
        if let Some(runtime) = &mut self.level {
            runtime.push_event(event);
        }
    }

    /// Aggregated substrate view: populations grouped by class/mode/colour plus
    /// terrain resource totals and the current objective conditions.
    pub(crate) fn level_census_snapshot(&self) -> LevelCensus {
        let status = self.level_status_snapshot();
        let environment = self.settings;
        let mut modes: BTreeMap<(u8, usize, u32), ModeAccum> = BTreeMap::new();
        for (_, (position, cell, genome)) in
            self.world.query::<(&Position, &Cell, &Genome)>().iter()
        {
            let color = genome.mode(cell.mode).resolved_color();
            let key = (
                cell.class,
                cell.mode,
                ((color[0] * 255.) as u32) << 16
                    | ((color[1] * 255.) as u32) << 8
                    | (color[2] * 255.) as u32,
            );
            modes
                .entry(key)
                .or_insert_with(|| ModeAccum::new(color))
                .add(position, cell);
        }
        let mut tiles = SubstrateAccum::default();
        for tile in &self.tiles {
            tiles.add(tile);
        }
        LevelCensus {
            status,
            bonds: self.bonds.len(),
            population_limit: self.population_limit,
            environment: CensusEnvironment {
                light: environment.light,
                food: environment.food,
                radiation: environment.radiation,
                viscosity: environment.viscosity,
                aging: environment.aging,
                kill_at_edge: environment.kill_at_edge,
                only_point_mutations: environment.only_point_mutations,
                gravity: environment.gravity,
                density: environment.density,
                nitrogen: environment.nitrogen,
            },
            modes: modes
                .into_iter()
                .map(|((class, mode, _), accum)| accum.finish(class, mode))
                .collect(),
            substrate: tiles.finish(&self.tiles),
        }
    }

    /// Per-cell view, sorted by id. Intended for small substrates and for
    /// explaining a failing demo near an objective boundary.
    pub(crate) fn level_cells_snapshot(&self) -> Vec<CellCensus> {
        let mut cells = self
            .world
            .query::<(&Position, &Cell, &Genome)>()
            .iter()
            .map(|(_, (position, cell, genome))| {
                let mode = genome.mode(cell.mode);
                let color = mode.resolved_color();
                CellCensus {
                    id: cell.id,
                    class: cell.class,
                    mode: cell.mode,
                    kind: mode.kind,
                    color,
                    rank: color_rank(color),
                    x: position.x / SIZE,
                    y: position.y / SIZE,
                    mass: cell.mass,
                    radius: radius(cell.mass),
                    nitrogen: cell.nitrogen,
                    age: cell.age,
                    generation: cell.generation,
                    mutations: cell.mutations,
                    anchored: cell.anchored,
                    telomeres: cell.telomeres,
                }
            })
            .collect::<Vec<_>>();
        cells.sort_unstable_by_key(|cell| cell.id);
        cells
    }
}

fn color_rank(color: [f32; 3]) -> &'static str {
    if is_black(color) {
        "black"
    } else if is_red(color) {
        "red"
    } else if is_yellow(color) {
        "yellow"
    } else {
        "other"
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LevelCensus {
    pub(crate) status: super::LevelStatus,
    pub(crate) bonds: usize,
    pub(crate) population_limit: usize,
    pub(crate) environment: CensusEnvironment,
    pub(crate) modes: Vec<ModeCensus>,
    pub(crate) substrate: SubstrateCensus,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CensusEnvironment {
    pub(crate) light: f32,
    pub(crate) food: f32,
    pub(crate) radiation: f32,
    pub(crate) viscosity: f32,
    pub(crate) aging: bool,
    pub(crate) kill_at_edge: bool,
    pub(crate) only_point_mutations: bool,
    pub(crate) gravity: f32,
    pub(crate) density: f32,
    pub(crate) nitrogen: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModeCensus {
    pub(crate) class: u8,
    pub(crate) mode: usize,
    pub(crate) count: u32,
    pub(crate) color: [f32; 3],
    pub(crate) rank: &'static str,
    pub(crate) mass_min: f32,
    pub(crate) mass_max: f32,
    pub(crate) mass_avg: f32,
    pub(crate) radius_avg: f32,
    pub(crate) centroid: [f32; 2],
    pub(crate) generation_max: u32,
    pub(crate) mutations_max: u32,
}

struct ModeAccum {
    color: [f32; 3],
    count: u32,
    mass_sum: f64,
    mass_min: f32,
    mass_max: f32,
    radius_sum: f64,
    x_sum: f64,
    y_sum: f64,
    generation_max: u32,
    mutations_max: u32,
}

impl ModeAccum {
    fn new(color: [f32; 3]) -> Self {
        Self {
            color,
            count: 0,
            mass_sum: 0.,
            mass_min: f32::MAX,
            mass_max: f32::MIN,
            radius_sum: 0.,
            x_sum: 0.,
            y_sum: 0.,
            generation_max: 0,
            mutations_max: 0,
        }
    }

    fn add(&mut self, position: &Position, cell: &Cell) {
        self.count += 1;
        self.mass_sum += f64::from(cell.mass);
        self.mass_min = self.mass_min.min(cell.mass);
        self.mass_max = self.mass_max.max(cell.mass);
        self.radius_sum += f64::from(radius(cell.mass));
        self.x_sum += f64::from(position.x / SIZE);
        self.y_sum += f64::from(position.y / SIZE);
        self.generation_max = self.generation_max.max(cell.generation);
        self.mutations_max = self.mutations_max.max(cell.mutations);
    }

    fn finish(self, class: u8, mode: usize) -> ModeCensus {
        let count = self.count.max(1);
        ModeCensus {
            class,
            mode,
            count: self.count,
            color: self.color,
            rank: color_rank(self.color),
            mass_min: self.mass_min,
            mass_max: self.mass_max,
            mass_avg: (self.mass_sum / f64::from(count)) as f32,
            radius_avg: (self.radius_sum / f64::from(count)) as f32,
            centroid: [
                (self.x_sum / f64::from(count)) as f32,
                (self.y_sum / f64::from(count)) as f32,
            ],
            generation_max: self.generation_max,
            mutations_max: self.mutations_max,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubstrateCensus {
    pub(crate) tiles: usize,
    pub(crate) food: f64,
    pub(crate) capacity: f64,
    pub(crate) nitrates: f64,
    pub(crate) nitrate_capacity: f64,
    pub(crate) covered_food: f64,
    pub(crate) light: f64,
    pub(crate) radiation: f64,
    pub(crate) biomes: [u32; 10],
}

#[derive(Default)]
struct SubstrateAccum {
    food: f64,
    capacity: f64,
    nitrates: f64,
    nitrate_capacity: f64,
    covered_food: f64,
    light: f64,
    radiation: f64,
    biomes: [u32; 10],
}

impl SubstrateAccum {
    fn add(&mut self, tile: &crate::terrain::Tile) {
        self.food += f64::from(tile.food);
        self.capacity += f64::from(tile.capacity);
        self.nitrates += f64::from(tile.nitrates);
        self.nitrate_capacity += f64::from(tile.nitrate_capacity);
        self.covered_food += f64::from(tile.covered_food);
        self.light += f64::from(tile.light);
        self.radiation += f64::from(tile.radiation);
        let biome = (tile.biome as usize).min(self.biomes.len() - 1);
        self.biomes[biome] += 1;
    }

    fn finish(self, tiles: &[crate::terrain::Tile]) -> SubstrateCensus {
        let count = tiles.len().max(1) as f64;
        SubstrateCensus {
            tiles: tiles.len(),
            food: self.food,
            capacity: self.capacity,
            nitrates: self.nitrates,
            nitrate_capacity: self.nitrate_capacity,
            covered_food: self.covered_food,
            light: self.light / count,
            radiation: self.radiation / count,
            biomes: self.biomes,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CellCensus {
    pub(crate) id: u32,
    pub(crate) class: u8,
    pub(crate) mode: usize,
    pub(crate) kind: u8,
    pub(crate) color: [f32; 3],
    pub(crate) rank: &'static str,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) mass: f32,
    pub(crate) radius: f32,
    pub(crate) nitrogen: f32,
    pub(crate) age: f32,
    pub(crate) generation: u32,
    pub(crate) mutations: u32,
    pub(crate) anchored: bool,
    pub(crate) telomeres: i32,
}

impl Simulation {
    pub fn level_census_json(&self) -> String {
        serde_json::to_string(&self.level_census_snapshot()).unwrap_or_else(|_| "{}".to_owned())
    }

    pub fn level_cells_json(&self) -> String {
        serde_json::to_string(&self.level_cells_snapshot()).unwrap_or_else(|_| "[]".to_owned())
    }

    pub fn level_events_json(&self) -> String {
        self.level
            .as_ref()
            .map_or_else(|| "[]".to_owned(), LevelRuntime::events_json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_buffer_is_bounded() {
        let level = super::super::schema::Level::default();
        let mut runtime = LevelRuntime::new(level, Vec::new());
        for tick in 0..(EVENT_CAPACITY as u32 + 10) {
            runtime.push_event(LevelEvent::new(tick, "test"));
        }
        assert_eq!(runtime.events.len(), EVENT_CAPACITY);
        assert_eq!(runtime.events.front().unwrap().tick, 10);
    }

    #[test]
    fn colour_ranks_match_objective_helpers() {
        assert_eq!(color_rank([0., 0., 0.]), "black");
        assert_eq!(color_rank([0.9, 0.1, 0.1]), "red");
        assert_eq!(color_rank([0.9, 0.9, 0.1]), "yellow");
        assert_eq!(color_rank([0.1, 0.9, 0.1]), "other");
    }
}
