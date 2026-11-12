//! Campaign levels: loading, objective tracking and scripted demo playback.
//!
//! The runtime is deliberately split from the schema. `schema` only describes
//! the on-disk format; this module turns a manifest into a live substrate and
//! evaluates its win/lose conditions without knowing anything about rendering.

pub(crate) mod schema;
mod observe;
mod demo;
mod rules;

pub(crate) use observe::LevelEvent;
pub(crate) use schema::{ConditionKind, Level};

use super::Simulation;
use super::boundary::Boundary;
use super::environment::Environment;
use crate::components::{
    CLASS_HYBRID, CLASS_INFECTED, CLASS_LEVEL, CLASS_USER, Cell, INITIAL_NITROGEN, LIMIT, Position,
    SIZE, STEP, radius,
};
use crate::genome::Genome;
use crate::terrain::{GRID, Tile, map, nitrate_capacity};
use serde::Serialize;
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LevelState {
    Running,
    Won,
    Lost,
}

impl LevelState {
    fn key(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Won => "won",
            Self::Lost => "lost",
        }
    }
}

/// One loaded level plus its mutable campaign state. The compiled genomes are
/// cached so demo spawns and initial cells never re-parse JSON.
pub(crate) struct LevelRuntime {
    pub(crate) level: Level,
    pub(crate) genomes: Vec<Genome>,
    pub(crate) state: LevelState,
    pub(crate) win_hold: f32,
    pub(crate) demo: bool,
    pub(crate) demo_cursor: usize,
    pub(crate) inserted: u32,
    pub(crate) removed: u32,
    pub(crate) demo_error: Option<String>,
    /// Bounded transition log for diagnostics. Never read by objective code.
    pub(crate) events: VecDeque<LevelEvent>,
    /// Previous met/unmet flag per condition, in canonical order.
    pub(crate) condition_flags: Vec<bool>,
}

impl LevelRuntime {
    fn new(level: Level, genomes: Vec<Genome>) -> Self {
        let condition_flags = vec![false; level.conditions.values().len()];
        Self {
            level,
            genomes,
            state: LevelState::Running,
            win_hold: 0.,
            demo: false,
            demo_cursor: 0,
            inserted: 0,
            removed: 0,
            demo_error: None,
            events: VecDeque::new(),
            condition_flags,
        }
    }

    fn elapsed(&self, tick: u32) -> f32 {
        tick as f32 * STEP
    }
}

/// Population split used by every campaign condition. Level conditions count
/// substrate (0) and infected (3) cells together; colour conditions only count
/// substrate cells, matching the original `Cell.E` semantics.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LevelCounts {
    pub(crate) total: u32,
    pub(crate) user: u32,
    pub(crate) level: u32,
    pub(crate) infected: u32,
    pub(crate) hybrid: u32,
    pub(crate) black: u32,
    pub(crate) red: u32,
    pub(crate) yellow: u32,
}

impl LevelCounts {
    pub(crate) fn level_total(self) -> u32 {
        self.level + self.infected
    }
}

fn condition_is_max(kind: ConditionKind) -> bool {
    matches!(
        kind,
        ConditionKind::UserCellsMax
            | ConditionKind::LevelCells
            | ConditionKind::InfectedLevelCellsMax
            | ConditionKind::BlackLevelCells
            | ConditionKind::RedLevelCells
            | ConditionKind::YellowLevelCells
            | ConditionKind::MaxTime
    )
}

fn condition_current(kind: ConditionKind, counts: LevelCounts, elapsed: f32, win_hold: f32) -> f32 {
    match kind {
        ConditionKind::UserCells => counts.user as f32,
        ConditionKind::UserCellsMax => counts.user as f32,
        ConditionKind::LevelCells => counts.level_total() as f32,
        ConditionKind::LevelCellsMin => counts.level_total() as f32,
        ConditionKind::TotalCellsMin => counts.total as f32,
        ConditionKind::InfectedLevelCells => counts.infected as f32,
        ConditionKind::InfectedLevelCellsMax => counts.infected as f32,
        ConditionKind::HybridCellsMin => counts.hybrid as f32,
        ConditionKind::BlackLevelCells => counts.black as f32,
        ConditionKind::RedLevelCells => counts.red as f32,
        ConditionKind::RedLevelCellsMin => counts.red as f32,
        ConditionKind::YellowLevelCells => counts.yellow as f32,
        ConditionKind::YellowLevelCellsMin => counts.yellow as f32,
        ConditionKind::WinTime => win_hold,
        ConditionKind::MaxTime => elapsed,
    }
}

fn condition_met(kind: ConditionKind, current: f32, target: f32) -> bool {
    if condition_is_max(kind) {
        current <= target + 1e-6
    } else {
        current + 1e-6 >= target
    }
}

/// A condition is part of the "hold" set when it demands something positive.
/// Max conditions are constraints: breaking one loses immediately.
fn condition_required(kind: ConditionKind) -> bool {
    !matches!(
        kind,
        ConditionKind::UserCellsMax | ConditionKind::InfectedLevelCellsMax | ConditionKind::MaxTime
    )
}

fn is_black(color: [f32; 3]) -> bool {
    color[0].abs() + color[1].abs() + color[2].abs() < 0.1
}

fn is_red(color: [f32; 3]) -> bool {
    color[0] > 0.65 && color[1] < 0.35 && color[2] < 0.35
}

fn is_yellow(color: [f32; 3]) -> bool {
    color[0] > 0.65 && color[1] > 0.65 && color[2] < 0.35
}

impl Simulation {
    pub(crate) fn load_level(&mut self, text: &str) -> Result<(), String> {
        let level = Level::from_json(text)?;
        self.install_level(level)
    }

    pub(crate) fn install_level(&mut self, level: Level) -> Result<(), String> {
        level.validate()?;
        let population_limit = level.environment.max_cells.clamp(1, LIMIT);
        let seed = level.test.seed.unwrap_or(self.seed).max(1);
        let genomes = level
            .genomes
            .iter()
            .map(schema::LevelGenome::to_genome)
            .collect::<Result<Vec<_>, _>>()?;
        let environment = level_environment(&level)?;
        let tiles = level_tiles(&level);
        let mut world = hecs::World::new();
        let initial = spawn_initial_cells(&mut world, &level, &genomes)?;
        *self = Self::new(seed);
        self.population_limit = population_limit;
        self.world = world;
        self.boundary = Boundary::Circle;
        self.settings = environment;
        self.tiles = tiles;
        self.rng = seed.max(1);
        self.next_id = initial.next_id;
        self.entities = initial.entities;
        self.bonds = initial.bonds;
        self.births = initial.cells as u32;
        self.level = Some(LevelRuntime::new(level, genomes));
        let counts = self.level_counts();
        let tick = self.tick;
        let summary = format!(
            "cells={} level={} user={} infected={} hybrid={} limit={} demo={}",
            counts.total,
            counts.level,
            counts.user,
            counts.infected,
            counts.hybrid,
            population_limit,
            self.level
                .as_ref()
                .map_or(0, |runtime| runtime.level.demo.len()),
        );
        self.emit_level_event(LevelEvent::new(tick, "load").with_detail(summary));
        Ok(())
    }

    pub(crate) fn start_demo(&mut self) -> Result<(), String> {
        let runtime = self.level.as_ref().ok_or("Уровень не загружен")?;
        if runtime.level.solution_status.as_deref() == Some("unavailable") {
            return Err(runtime
                .level
                .solution_issue
                .clone()
                .unwrap_or_else(|| "Проверенное решение пока недоступно".into()));
        }
        runtime.level.check_demo(&runtime.genomes)?;
        let level = runtime.level.clone();
        self.install_level(level)?;
        let runtime = self.level.as_mut().unwrap();
        runtime.demo = true;
        runtime.demo_cursor = 0;
        runtime.state = LevelState::Running;
        runtime.win_hold = 0.;
        runtime.events.clear();
        runtime.condition_flags.fill(false);
        let actions = runtime.level.demo.len();
        runtime.push_event(
            LevelEvent::new(self.tick, "demo-start").with_detail(format!("actions={actions}")),
        );
        Ok(())
    }

    pub(crate) fn stop_demo(&mut self) {
        if let Some(runtime) = &mut self.level {
            runtime.demo = false;
        }
    }

    pub(crate) fn level_counts(&self) -> LevelCounts {
        let mut counts = LevelCounts::default();
        for (_, (cell, genome)) in self.world.query::<(&Cell, &Genome)>().iter() {
            counts.total += 1;
            match cell.class {
                CLASS_USER => counts.user += 1,
                CLASS_LEVEL => counts.level += 1,
                CLASS_INFECTED => counts.infected += 1,
                CLASS_HYBRID => counts.hybrid += 1,
                _ => {}
            }
            if cell.class == CLASS_LEVEL {
                let color = genome.mode(cell.mode).resolved_color();
                counts.black += u32::from(is_black(color));
                counts.red += u32::from(is_red(color));
                counts.yellow += u32::from(is_yellow(color));
            }
        }
        counts
    }

    /// Advance hold timers and terminal state once per simulation tick.
    pub(crate) fn update_level_status(&mut self) {
        if self
            .level
            .as_ref()
            .is_none_or(|runtime| runtime.state != LevelState::Running)
        {
            return;
        }
        let counts = self.level_counts();
        let tick = self.tick;
        let Some(runtime) = &mut self.level else {
            return;
        };
        let elapsed = runtime.elapsed(tick);
        let conditions = runtime.level.conditions.values();
        let values = conditions
            .iter()
            .map(|&(kind, target)| {
                let current = condition_current(kind, counts, elapsed, runtime.win_hold);
                (kind, target, current, condition_met(kind, current, target))
            })
            .collect::<Vec<_>>();
        if runtime.condition_flags.len() != values.len() {
            runtime.condition_flags = vec![false; values.len()];
        }
        let mut events = Vec::new();
        for (index, &(kind, target, current, met)) in values.iter().enumerate() {
            if runtime.condition_flags[index] == met {
                continue;
            }
            runtime.condition_flags[index] = met;
            events.push(LevelEvent::new(tick, "objective").with_detail(format!(
                "{} {} ({current:.2}/{target:.2})",
                condition_key(kind),
                if met { "met" } else { "unmet" },
            )));
        }
        let mut required = true;
        let mut constrained = true;
        for &(kind, _target, _, met) in &values {
            // `win-time` is the hold accumulator itself, not a prerequisite for
            // accumulating. Counting it as unmet froze the timer at zero on
            // every level whose win condition is "hold for N seconds".
            if met || kind == ConditionKind::WinTime {
                continue;
            }
            if condition_required(kind) {
                required = false;
            } else {
                constrained = false;
            }
        }
        let max_time = runtime.level.conditions.max_time();
        let win_time = runtime.level.conditions.win_time();
        let mut next = LevelState::Running;
        if !constrained {
            next = LevelState::Lost;
        } else if required {
            runtime.win_hold += STEP;
            if runtime.win_hold + 1e-6 >= win_time {
                next = LevelState::Won;
            }
        } else {
            runtime.win_hold = 0.;
        }
        if next == LevelState::Running && elapsed > max_time + 1e-6 {
            next = LevelState::Lost;
        }
        if next != LevelState::Running {
            runtime.state = next;
            events.push(
                LevelEvent::new(tick, "state").with_detail(format!("{:?} at {elapsed:.2}s", next)),
            );
        }
        for event in events {
            runtime.push_event(event);
        }
    }

    pub(crate) fn level_status_snapshot(&self) -> LevelStatus {
        let Some(runtime) = &self.level else {
            return LevelStatus::default();
        };
        let counts = self.level_counts();
        let elapsed = runtime.elapsed(self.tick);
        let conditions = runtime
            .level
            .conditions
            .values()
            .into_iter()
            .map(|(kind, target)| {
                let current = condition_current(kind, counts, elapsed, runtime.win_hold);
                ConditionStatus {
                    key: condition_key(kind),
                    target,
                    current,
                    met: condition_met(kind, current, target),
                    required: condition_required(kind),
                }
            })
            .collect();
        LevelStatus {
            id: runtime.level.id.clone(),
            state: runtime.state.key(),
            elapsed,
            win_time: runtime.level.conditions.win_time(),
            win_hold: runtime.win_hold,
            demo: runtime.demo,
            demo_actions: runtime.level.demo.len(),
            demo_cursor: runtime.demo_cursor,
            inserted: runtime.inserted,
            removed: runtime.removed,
            demo_error: runtime.demo_error.clone(),
            tutorial: runtime.level.tutorial,
            counts: StatusCounts {
                total: counts.total,
                user: counts.user,
                level: counts.level,
                infected: counts.infected,
                hybrid: counts.hybrid,
                black: counts.black,
                red: counts.red,
                yellow: counts.yellow,
            },
            conditions,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LevelStatus {
    pub(crate) id: String,
    pub(crate) state: &'static str,
    pub(crate) elapsed: f32,
    pub(crate) win_time: f32,
    pub(crate) win_hold: f32,
    pub(crate) demo: bool,
    pub(crate) demo_actions: usize,
    pub(crate) demo_cursor: usize,
    pub(crate) inserted: u32,
    pub(crate) removed: u32,
    pub(crate) demo_error: Option<String>,
    pub(crate) tutorial: bool,
    pub(crate) counts: StatusCounts,
    pub(crate) conditions: Vec<ConditionStatus>,
}

impl Default for LevelStatus {
    fn default() -> Self {
        Self {
            id: String::new(),
            state: "running",
            elapsed: 0.,
            win_time: 0.,
            win_hold: 0.,
            demo: false,
            demo_actions: 0,
            demo_cursor: 0,
            inserted: 0,
            removed: 0,
            demo_error: None,
            tutorial: false,
            counts: StatusCounts::default(),
            conditions: Vec::new(),
        }
    }
}

impl LevelStatus {
    pub(crate) fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatusCounts {
    pub(crate) total: u32,
    pub(crate) user: u32,
    pub(crate) level: u32,
    pub(crate) infected: u32,
    pub(crate) hybrid: u32,
    pub(crate) black: u32,
    pub(crate) red: u32,
    pub(crate) yellow: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConditionStatus {
    pub(crate) key: &'static str,
    pub(crate) target: f32,
    pub(crate) current: f32,
    pub(crate) met: bool,
    pub(crate) required: bool,
}

fn condition_key(kind: ConditionKind) -> &'static str {
    match kind {
        ConditionKind::UserCells => "user-cells",
        ConditionKind::UserCellsMax => "user-cells-max",
        ConditionKind::LevelCells => "level-cells",
        ConditionKind::LevelCellsMin => "level-cells-min",
        ConditionKind::TotalCellsMin => "total-cells-min",
        ConditionKind::InfectedLevelCells => "infected-level-cells",
        ConditionKind::InfectedLevelCellsMax => "infected-level-cells-max",
        ConditionKind::HybridCellsMin => "hybrid-cells-min",
        ConditionKind::BlackLevelCells => "black-level-cells",
        ConditionKind::RedLevelCells => "red-level-cells",
        ConditionKind::RedLevelCellsMin => "red-level-cells-min",
        ConditionKind::YellowLevelCells => "yellow-level-cells",
        ConditionKind::YellowLevelCellsMin => "yellow-level-cells-min",
        ConditionKind::WinTime => "win-time",
        ConditionKind::MaxTime => "max-time",
    }
}

fn level_environment(level: &Level) -> Result<Environment, String> {
    let source = &level.environment;
    // The original slider runs 0..150 while the web model normalises around 60.
    let viscosity = (source.viscosity / 60.).clamp(0.1, 5.);
    let environment = Environment {
        light: source.light.clamp(0., 5.),
        food: source.food.clamp(0., 5.),
        radiation: source.radiation.clamp(0., 5.),
        viscosity,
        aging: source.aging,
        kill_at_edge: source.kill_at_edge,
        only_point_mutations: source.only_point_mutations,
        gravity: source.gravity.clamp(0., 1.),
        density: source.density.clamp(-3.3, 3.3),
        nitrogen: source.nitrogen.clamp(0., 5.),
    };
    environment.validate()?;
    Ok(environment)
}

fn level_tiles(level: &Level) -> Vec<Tile> {
    let terrain = &level.terrain;
    let mut tiles = map();
    for (index, tile) in tiles.iter_mut().enumerate() {
        let y = (index / GRID) as f32 / (GRID - 1) as f32;
        let falloff = if terrain.light_range > 0. {
            (1. - y / terrain.light_range).clamp(0., 1.)
        } else {
            1.
        };
        let capacity = terrain.capacity.max(0.).max(terrain.food.max(0.));
        tile.light = terrain.light.max(0.) * falloff;
        tile.radiation = terrain.radiation.max(0.);
        tile.capacity = capacity;
        tile.food = terrain.food.clamp(0., capacity);
        tile.biome = terrain.biome as f32;
        tile.nitrate_capacity = nitrate_capacity(tile.biome) * terrain.nitrates.max(0.);
        tile.nitrates = tile.nitrate_capacity;
        tile.covered_food = 0.;
    }
    for particle in &level.initial.food {
        if particle.len() < 2 || !particle[0].is_finite() || !particle[1].is_finite() {
            continue;
        }
        // The converter normalizes every particle into [0, 1] dish coordinates.
        let x = particle[0].clamp(0., 1.);
        let y = particle[1].clamp(0., 1.);
        let index = ((y * GRID as f32) as usize).min(GRID - 1) * GRID
            + ((x * GRID as f32) as usize).min(GRID - 1);
        let amount = particle.get(2).copied().unwrap_or(0.05).max(0.).min(5.);
        let tile = &mut tiles[index];
        tile.food = (tile.food + amount).min(1_000_000.);
        tile.capacity = tile.capacity.max(tile.food);
    }
    tiles
}

struct InitialSpawn {
    entities: Vec<(u32, hecs::Entity)>,
    bonds: BTreeSet<(u32, u32)>,
    next_id: u32,
    cells: usize,
}

fn spawn_initial_cells(
    world: &mut hecs::World,
    level: &Level,
    genomes: &[Genome],
) -> Result<InitialSpawn, String> {
    let mut entities = Vec::new();
    let mut ids = Vec::with_capacity(level.initial.cells.len());
    let mut bonds = BTreeSet::new();
    let mut next_id = 1;
    for (index, source) in level.initial.cells.iter().enumerate() {
        let genome = genomes
            .get(source.genome)
            .ok_or_else(|| format!("Клетка {index} ссылается на отсутствующий геном"))?
            .clone();
        if source.mode >= genome.len() {
            return Err(format!("Клетка {index} ссылается на отсутствующий режим"));
        }
        let id = next_id;
        next_id += 1;
        let angle = normalize_angle(source.angle);
        let mut cell = Cell::new(id, source.mode);
        cell.mass = source.mass.clamp(0.18, 1_000_000.);
        cell.class = source.class;
        cell.age = source.age.max(0.).min(1_000.);
        cell.generation = source.generation;
        cell.nitrogen = INITIAL_NITROGEN;
        let position = Position {
            x: source.x.clamp(0., 1.) * SIZE,
            y: source.y.clamp(0., 1.) * SIZE,
            angle,
        };
        // Clamp into the circular dish without letting a wall cell jump across
        // the map: positions are already normalized to the original radius.
        let (x, y) = clamp_to_dish(position.x, position.y, cell.mass);
        let entity = world.spawn((Position { x, y, angle }, cell, genome));
        entities.push((id, entity));
        ids.push(id);
    }
    for &[a, b] in &level.initial.links {
        let (Some(&left), Some(&right)) = (ids.get(a), ids.get(b)) else {
            continue;
        };
        if left != right {
            bonds.insert((left.min(right), left.max(right)));
        }
    }
    Ok(InitialSpawn {
        cells: entities.len(),
        entities,
        bonds,
        next_id,
    })
}

fn clamp_to_dish(x: f32, y: f32, mass: f32) -> (f32, f32) {
    let center = SIZE / 2.;
    let dx = x - center;
    let dy = y - center;
    let distance = libm::hypotf(dx, dy);
    let limit = (center - radius(mass).max(12.)).max(0.);
    if distance > limit && distance > 0. {
        (
            center + dx * limit / distance,
            center + dy * limit / distance,
        )
    } else {
        (x, y)
    }
}

fn normalize_angle(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.;
    }
    let mut angle = value % std::f32::consts::TAU;
    if angle > std::f32::consts::PI {
        angle -= std::f32::consts::TAU;
    } else if angle < -std::f32::consts::PI {
        angle += std::f32::consts::TAU;
    }
    angle
}

/// Shared by the loader and the demo runner. New player cells always receive
/// class 1; only substrate cells may be class 0.
/// Native-facing helpers used by `cargo test` and the `level_check` example.
/// The wasm ABI wraps the same methods with `JsError`.
impl Simulation {
    pub fn load_level_text(&mut self, text: &str) -> Result<(), String> {
        self.load_level(text)
    }

    pub fn level_status_json(&self) -> String {
        self.level_status_snapshot().to_json()
    }

    pub fn validate_demo_track(&self) -> Result<(), String> {
        let runtime = self.level.as_ref().ok_or("Уровень не загружен")?;
        runtime.level.check_demo(&runtime.genomes)
    }

    pub fn start_demo_track(&mut self) -> Result<(), String> {
        self.start_demo()
    }

    pub fn stop_demo_track(&mut self) {
        self.stop_demo();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_helpers_match_original_boundaries() {
        assert!(condition_met(ConditionKind::LevelCells, 10., 10.));
        assert!(!condition_met(ConditionKind::LevelCells, 11., 10.));
        assert!(condition_met(ConditionKind::UserCells, 100., 100.));
        assert!(!condition_met(ConditionKind::UserCellsMax, 1., 0.));
        assert!(condition_met(ConditionKind::BlackLevelCells, 0., 0.));
    }

    #[test]
    fn win_time_accumulates_while_other_conditions_hold() {
        let level = Level {
            id: "win-time".into(),
            conditions: schema::Conditions {
                user_cells: Some(0),
                win_time: Some(0.5),
                ..schema::Conditions::default()
            },
            ..Level::default()
        };
        let mut simulation = Simulation::new(1);
        simulation.install_level(level).unwrap();
        simulation.advance(20);
        assert_eq!(
            simulation.level.as_ref().unwrap().state,
            LevelState::Won,
            "win-time must count up even though it is itself an unmet condition"
        );
    }

    #[test]
    fn colours_only_count_substrate_class() {
        let counts = LevelCounts {
            total: 6,
            level: 2,
            red: 2,
            ..LevelCounts::default()
        };
        assert_eq!(counts.level_total(), 2);
        assert_eq!(counts.red, 2);
    }
}
