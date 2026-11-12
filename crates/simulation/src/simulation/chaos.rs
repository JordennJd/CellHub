//! Composable dynamic environment, editor-slot immigration and observation.

use super::Simulation;
use super::boundary::Boundary;
use crate::components::{Cell, LIMIT, Position, STEP, TICK_RATE, radius};
use crate::genome::{Genome, MAX_MODES, MODE_STRIDE, Mode, decode_checked};
#[cfg(test)]
use crate::terrain::{CHAOS_GRID, index};
use crate::terrain::{CHAOS_SIZE, Tile, chaos_map, random};
use wasm_bindgen::prelude::*;

pub(super) const HISTORY_LIMIT: usize = 300;
pub(super) const EVENT_LIMIT: usize = 80;

/// Full day/night cycle: 60 seconds of simulated time.
pub(super) const DAY_TICKS: u32 = 60 * TICK_RATE;
/// A curated multicellular organism arrives once every 15 seconds.
pub(super) const ORGANISM_INTERVAL: u32 = 15 * TICK_RATE;
/// First spontaneous arrival happens after six seconds.
pub(super) const FIRST_SPAWN_TICKS: u32 = 6 * TICK_RATE;

/// A curated multicellular organism supplied by the frontend catalog.
#[derive(Clone)]
pub(super) struct Primordial {
    pub(super) genome: Genome,
    /// Cells seeded in a chain, so the organism starts multicellular.
    pub(super) span: usize,
    pub(super) weight: u32,
}

pub(super) struct Chaos {
    pub(super) pool: Genome,
    pub(super) initial_pool: Genome,
    /// Curated organisms for spontaneous immigration. They are supplied by the
    /// frontend and are stateless, so checkpoints rebuild them after restore.
    pub(super) organisms: Vec<Primordial>,
    pub(super) baseline: Vec<Tile>,
    pub(super) rng: u32,
    pub(super) enabled: bool,
    pub(super) rate: u32,
    pub(super) viable: bool,
    pub(super) next_spawn: u32,
    pub(super) spawned: u32,
    pub(super) organisms_spawned: u32,
    pub(super) history: Vec<[f32; 20]>,
    pub(super) events: Vec<[u32; 3]>,
}

impl Chaos {
    pub(super) fn new(seed: u32, pool: Genome, organisms: Vec<Primordial>) -> Self {
        Self {
            initial_pool: pool.clone(),
            pool,
            organisms,
            baseline: chaos_map(seed),
            rng: (seed ^ 0xa341316c).max(1),
            enabled: true,
            rate: 10,
            viable: true,
            next_spawn: FIRST_SPAWN_TICKS,
            spawned: 0,
            organisms_spawned: 0,
            history: Vec::new(),
            events: Vec::new(),
        }
    }

    pub(super) fn event(&mut self, tick: u32, kind: u32, count: u32) {
        if self.events.len() == EVENT_LIMIT {
            self.events.remove(0);
        }
        self.events.push([tick, kind, count]);
    }
}

/// Continuous daylight: noon at t=0, minimum 40% at t=30, next noon at t=60.
pub(super) fn daylight(tick: u32) -> f32 {
    0.7 + 0.3 * libm::cosf((tick % DAY_TICKS) as f32 / DAY_TICKS as f32 * std::f32::consts::TAU)
}

/// Analytic seeded orbits keep clouds independent of biology and spawn RNG.
#[cfg(test)]
pub(super) fn clouds(seed: u32, tick: u32) -> [[f32; 6]; 3] {
    clouds_rect(seed, tick, CHAOS_SIZE, CHAOS_SIZE)
}

/// Versioned frontend catalog: `[version, count, (span, weight, genome...)*]`.
const ORGANISM_ABI_VERSION: f32 = 1.;
const MAX_ORGANISM_SPAN: usize = 16;

/// Always-present single cells. They keep the primordial soup alive and
/// interesting even when no multicellular organism has appeared yet.
fn single_cell(kind: u8, threshold: f32, can_divide: bool, speed: f32) -> Genome {
    Genome::from_modes(vec![Mode {
        kind,
        threshold,
        can_divide,
        speed,
        lifespan: 1000.,
        ..Mode::default()
    }])
}

fn single_cells() -> Vec<(Genome, u32)> {
    vec![
        (single_cell(1, 2.2, true, 0.), 34),  // phagocyte
        (single_cell(5, 2.6, true, 0.), 16),  // devorocyte
        (single_cell(0, 2.1, true, 0.), 12),  // photocyte, deliberately rarer
        (single_cell(3, 2.2, true, 0.), 12),  // virocyte
        (single_cell(2, 2.0, false, 32.), 8), // lone flagellocyte
    ]
}

fn whole(value: f32) -> Option<usize> {
    (value.is_finite() && value >= 0. && value.fract() == 0. && value <= u32::MAX as f32)
        .then_some(value as usize)
}

/// Parse the curated organism catalog supplied by the frontend. Checkpoints
/// omit it, so the worker resends the same buffer after restoring a world.
pub(super) fn decode_primordial(buffer: &[f32]) -> Result<Vec<Primordial>, String> {
    if buffer.is_empty() {
        return Ok(Vec::new());
    }
    if buffer[0] != ORGANISM_ABI_VERSION {
        return Err("Unsupported chaos organism catalog".into());
    }
    let count = buffer
        .get(1)
        .copied()
        .and_then(whole)
        .ok_or("Invalid chaos organism catalog")?;
    if count > 256 {
        return Err("Too many chaos organisms".into());
    }
    let mut position = 2;
    let mut organisms = Vec::with_capacity(count);
    for _ in 0..count {
        let span = buffer
            .get(position)
            .copied()
            .and_then(whole)
            .ok_or("Invalid chaos organism span")?;
        let weight = buffer
            .get(position + 1)
            .copied()
            .and_then(whole)
            .ok_or("Invalid chaos organism weight")?;
        position += 2;
        if !(1..=MAX_ORGANISM_SPAN).contains(&span) || !(1..=u16::MAX as usize).contains(&weight) {
            return Err("Invalid chaos organism span or weight".into());
        }
        let len = buffer
            .get(position + 1)
            .copied()
            .and_then(whole)
            .ok_or("Invalid chaos organism genome")?;
        if !(1..=MAX_MODES).contains(&len) {
            return Err("Invalid chaos organism genome".into());
        }
        let end = position
            .checked_add(2 + len * MODE_STRIDE)
            .filter(|&end| end <= buffer.len())
            .ok_or("Truncated chaos organism genome")?;
        let genome = decode_checked(&buffer[position..end])?;
        organisms.push(Primordial {
            genome,
            span,
            weight: weight as u32,
        });
        position = end;
    }
    if position != buffer.len() {
        return Err("Trailing chaos organism catalog data".into());
    }
    Ok(organisms)
}

fn pick_weighted(weights: &[u32], rng: &mut u32) -> usize {
    let total: u32 = weights.iter().sum();
    let mut roll = (random(rng) * total as f32) as u32;
    for (index, &weight) in weights.iter().enumerate() {
        if roll < weight {
            return index;
        }
        roll -= weight;
    }
    0
}

fn clouds_rect(seed: u32, tick: u32, width: f32, height: f32) -> [[f32; 6]; 3] {
    let mut rng = (seed ^ 0xc8013ea4).max(1);
    std::array::from_fn(|i| {
        let phase = random(&mut rng) * std::f32::consts::TAU;
        let period = 100. + random(&mut rng) * 100.;
        let radius = 650. + random(&mut rng) * 350.;
        let a = phase + tick as f32 / TICK_RATE as f32 / period * std::f32::consts::TAU;
        let orbit = 1000. + i as f32 * 300.;
        let omega = std::f32::consts::TAU / period;
        [
            width / 2. + libm::cosf(a) * orbit * width / CHAOS_SIZE,
            height / 2. + libm::sinf(a * 1.3 + phase) * orbit * 0.8 * height / CHAOS_SIZE,
            radius,
            0.55 + i as f32 * 0.15,
            -libm::sinf(a) * orbit * omega * width / CHAOS_SIZE,
            libm::cosf(a * 1.3 + phase) * orbit * 0.8 * 1.3 * omega * height / CHAOS_SIZE,
        ]
    })
}

#[wasm_bindgen]
impl Simulation {
    pub fn start_chaos(
        &mut self,
        seed: u32,
        genes: &[f32],
        width: Option<f32>,
    ) -> Result<(), JsError> {
        let pool = decode_checked(genes).map_err(|e| JsError::new(&e))?;
        if let Some(width) = width {
            if !width.is_finite()
                || width < CHAOS_SIZE * 0.5
                || width > CHAOS_SIZE * 4.
                || width % 64. != 0.
            {
                return Err(JsError::new("Некорректная ширина поля"));
            }
            self.start_chaos_in_bounds(seed, pool, width, CHAOS_SIZE, true);
        } else {
            self.start_chaos_checked(seed, pool);
        }
        Ok(())
    }

    /// Start chaos with the curated multicellular catalog in one call.
    pub fn start_chaos_with_organisms(
        &mut self,
        seed: u32,
        genes: &[f32],
        organisms: &[f32],
        width: Option<f32>,
    ) -> Result<(), JsError> {
        self.set_chaos_organisms(organisms)?;
        self.start_chaos(seed, genes, width)
    }

    /// Replace the organism catalog. It is kept outside checkpoints and must be
    /// sent again after restoring a saved world.
    pub fn set_chaos_organisms(&mut self, organisms: &[f32]) -> Result<(), JsError> {
        let catalog = decode_primordial(organisms).map_err(|e| JsError::new(&e))?;
        self.chaos_catalog = catalog.clone();
        if let Some(chaos) = &mut self.chaos {
            chaos.organisms = catalog;
        }
        Ok(())
    }

    pub fn set_chaos_pool(&mut self, genes: &[f32]) -> Result<(), JsError> {
        let pool = decode_checked(genes).map_err(|e| JsError::new(&e))?;
        if let Some(chaos) = &mut self.chaos {
            chaos.pool = pool;
        }
        Ok(())
    }

    pub fn configure_chaos(
        &mut self,
        enabled: bool,
        rate: u32,
        viable: bool,
    ) -> Result<(), JsError> {
        if !(1..=60).contains(&rate) {
            return Err(JsError::new("Частота должна быть от 1 до 60"));
        }
        if let Some(chaos) = &mut self.chaos {
            if chaos.enabled != enabled || chaos.rate != rate {
                chaos.next_spawn = self.tick + DAY_TICKS / rate;
            }
            chaos.enabled = enabled;
            chaos.rate = rate;
            chaos.viable = viable;
            chaos.event(self.tick, 6, enabled as u32);
        }
        Ok(())
    }

    pub fn world_size(&self) -> f32 {
        self.boundary.size()
    }
    pub fn world_width(&self) -> f32 {
        self.boundary.width()
    }
    /// Whether the dish is drawn and simulated as a circle.
    pub fn circular_dish(&self) -> bool {
        matches!(self.boundary, Boundary::Circle)
    }

    /// Header followed by three clouds (x,y,r,intensity,vx,vy).
    pub fn chaos_info(&self) -> Vec<f64> {
        let Some(chaos) = &self.chaos else {
            return Vec::new();
        };
        let mut values = vec![
            self.seed as f64,
            daylight(self.tick) as f64,
            chaos.enabled as u8 as f64,
            chaos.rate as f64,
            chaos.viable as u8 as f64,
            chaos.next_spawn.saturating_sub(self.tick) as f64 / TICK_RATE as f64,
            (ORGANISM_INTERVAL - self.tick % ORGANISM_INTERVAL) as f64 / TICK_RATE as f64,
            chaos.spawned as f64,
            chaos.organisms_spawned as f64,
            chaos.organisms.len() as f64,
        ];
        values.extend(
            clouds_rect(
                self.seed,
                self.tick,
                self.boundary.width(),
                self.boundary.size(),
            )
            .into_iter()
            .flatten()
            .map(f64::from),
        );
        values
    }

    pub fn chaos_history(&self) -> Vec<f32> {
        self.chaos
            .as_ref()
            .map(|c| c.history.iter().flatten().copied().collect())
            .unwrap_or_default()
    }

    pub fn chaos_events(&self) -> Vec<u32> {
        self.chaos
            .as_ref()
            .map(|c| c.events.iter().flatten().copied().collect())
            .unwrap_or_default()
    }
}

impl Simulation {
    /// Native entry point for the authoritative server: start a Chaos world
    /// with the preset supplied by the creating client.
    pub fn start_chaos_native(
        &mut self,
        seed: u32,
        genes: &[f32],
        organisms: &[f32],
    ) -> Result<(), String> {
        let pool = decode_checked(genes)?;
        let catalog = decode_primordial(organisms)?;
        self.chaos_catalog = catalog;
        // The arena is a square 3072 x 3072 world, matching the frontend
        // Chaos laboratory with the viewport at its default width.
        self.start_chaos_in_bounds(seed, pool, CHAOS_SIZE, CHAOS_SIZE, true);
        Ok(())
    }

    /// Fallback Chaos world for clients that do not send a preset. The
    /// environment, immigration schedule and mandatory single cells match
    /// Chaos; only the curated multicellular catalog is absent.
    pub fn start_chaos_defaults(&mut self, seed: u32) {
        let modes = (0..crate::cell_types::COUNT)
            .map(|kind| Mode {
                kind,
                threshold: 2.6,
                lifespan: 360.,
                can_divide: matches!(
                    kind,
                    crate::cell_types::PHOTOCYTE | crate::cell_types::PHAGOCYTE
                ),
                speed: if kind == crate::cell_types::FLAGELLOCYTE {
                    28.
                } else {
                    0.
                },
                keep_a: true,
                ..Mode::default()
            })
            .collect();
        self.chaos_catalog.clear();
        self.start_chaos_in_bounds(
            seed,
            Genome::from_modes(modes),
            CHAOS_SIZE,
            CHAOS_SIZE,
            true,
        );
    }

    pub(super) fn start_chaos_checked(&mut self, seed: u32, pool: Genome) {
        self.start_chaos_in_bounds(seed, pool, CHAOS_SIZE, CHAOS_SIZE, false);
    }

    pub(super) fn start_chaos_in_bounds(
        &mut self,
        seed: u32,
        pool: Genome,
        width: f32,
        height: f32,
        rectangle: bool,
    ) {
        let track_divisions = self.division_events.is_some();
        let catalog = std::mem::take(&mut self.chaos_catalog);
        *self = Self::new(seed);
        self.set_division_event_tracking(track_divisions);
        self.boundary = Boundary::Chaos;
        self.chaos_catalog = catalog.clone();
        self.chaos = Some(Chaos::new(seed, pool, catalog));
        self.tiles = self.chaos.as_ref().unwrap().baseline.clone();
        self.spatial = super::spatial::SpatialGrid::with_size(width, height);
        if rectangle {
            self.set_rectangular_dish(width, height)
                .expect("validated chaos bounds");
        }
        self.refresh_chaos_fields();
        // A mixed primordial soup: ordinary single cells plus several complete
        // multicellular organisms, present from the very first frame.
        for _ in 0..108 {
            self.chaos_arrival(false, false);
        }
        for _ in 0..16 {
            self.chaos_arrival(true, false);
        }
        let count = self.world.len();
        self.chaos.as_mut().unwrap().event(0, 0, count);
        self.record_chaos_history();
    }

    pub(super) fn refresh_chaos_fields(&mut self) {
        let Some(chaos) = &self.chaos else {
            return;
        };
        let light = daylight(self.tick) * self.settings.light;
        let clouds = clouds_rect(
            self.seed,
            self.tick,
            self.boundary.width(),
            self.boundary.size(),
        );
        for (i, (tile, base)) in self.tiles.iter_mut().zip(&chaos.baseline).enumerate() {
            let x = (i % self.boundary.columns()) as f32 * 64. + 32.;
            let y = (i / self.boundary.columns()) as f32 * 64. + 32.;
            let mut radiation = base.radiation;
            for cloud in clouds {
                let falloff = (1.
                    - ((x - cloud[0]).powi(2) + (y - cloud[1]).powi(2)) / cloud[2].powi(2))
                .max(0.);
                radiation += falloff * falloff * cloud[3];
            }
            tile.light = (base.light * light).min(5.);
            tile.radiation = (radiation * self.settings.radiation).min(5.);
        }
    }

    pub(super) fn update_chaos(&mut self) {
        if self.chaos.is_none() {
            return;
        }
        self.refresh_chaos_fields();
        if self.tick % DAY_TICKS == DAY_TICKS / 4 {
            self.chaos.as_mut().unwrap().event(self.tick, 2, 0);
        }
        if self.tick % DAY_TICKS == DAY_TICKS * 3 / 4 {
            self.chaos.as_mut().unwrap().event(self.tick, 1, 0);
        }
        let chaos = self.chaos.as_mut().unwrap();
        let due = self.tick >= chaos.next_spawn;
        if due {
            chaos.next_spawn = self.tick + DAY_TICKS / chaos.rate;
        }
        let organism = self.tick % ORGANISM_INTERVAL == 0;
        if chaos.enabled && (due || organism) {
            self.chaos_arrival(organism, true);
        }
    }

    /// Spontaneous immigration: a complete curated organism on organism ticks,
    /// otherwise one of the mandatory single-cell roles.
    fn chaos_arrival(&mut self, organism: bool, immigration: bool) {
        let (genome, span, complex) = {
            let Some(chaos) = &mut self.chaos else {
                return;
            };
            let mut rng = chaos.rng;
            if organism && !chaos.organisms.is_empty() {
                let weights: Vec<u32> = chaos.organisms.iter().map(|o| o.weight).collect();
                let index = pick_weighted(&weights, &mut rng);
                chaos.rng = rng;
                let chosen = &chaos.organisms[index];
                (chosen.genome.clone(), chosen.span, true)
            } else {
                let singles = single_cells();
                let mut weights: Vec<u32> = singles.iter().map(|(_, weight)| *weight).collect();
                if chaos.viable {
                    for (index, (genome, _)) in singles.iter().enumerate() {
                        if matches!(genome.mode(0).kind, 0 | 1) {
                            weights[index] *= 4;
                        }
                    }
                }
                let index = pick_weighted(&weights, &mut rng);
                chaos.rng = rng;
                (singles[index].0.clone(), 1, false)
            }
        };
        self.seed_arrival(genome, span, complex, immigration);
    }

    fn seed_arrival(&mut self, genome: Genome, span: usize, complex: bool, immigration: bool) {
        let span = span.clamp(1, MAX_ORGANISM_SPAN);
        if self.world.len() as usize + span > LIMIT {
            return;
        }
        let viable = self.chaos.as_ref().unwrap().viable;
        let mut rng = self.chaos.as_ref().unwrap().rng;
        const SPACING: f32 = 26.;
        let reach = SPACING * (span.saturating_sub(1)) as f32 / 2.;
        let clearance = 44. + reach + radius(2.);
        let mut center = None;
        for _ in 0..64 {
            let x = 80. + random(&mut rng) * (self.boundary.width() - 160.);
            let y = 80. + random(&mut rng) * (self.boundary.size() - 160.);
            if !matches!(self.boundary, Boundary::Rectangle { .. })
                && libm::hypotf(x - CHAOS_SIZE / 2., y - CHAOS_SIZE / 2.)
                    > CHAOS_SIZE / 2. - 100. - reach
            {
                continue;
            }
            if self
                .world
                .query::<(&Position, &Cell)>()
                .iter()
                .any(|(_, (p, cell))| {
                    let distance = clearance + radius(cell.mass);
                    (p.x - x).powi(2) + (p.y - y).powi(2) < distance * distance
                })
            {
                continue;
            }
            let tile = self.tiles[self.boundary.tile(x, y)];
            if viable && tile.radiation > 0.5 {
                continue;
            }
            center = Some((x, y));
            break;
        }
        let Some((x, y)) = center else {
            self.chaos.as_mut().unwrap().rng = rng;
            return;
        };
        let direction = random(&mut rng) * std::f32::consts::TAU;
        self.chaos.as_mut().unwrap().rng = rng;
        let mut previous = None;
        for i in 0..span {
            let id = self.next_id;
            self.next_id += 1;
            // A one-mode organism (a photocyte filament) repeats its only mode.
            let mut cell = Cell::new(id, i.min(genome.len() - 1));
            cell.mass = if complex { 1.8 } else { 1.4 };
            // Front cell first; the flagellum at the end trails behind it.
            let along = (span as f32 - 1.) / 2. - i as f32;
            let p = Position {
                x: x + libm::cosf(direction) * along * SPACING,
                y: y + libm::sinf(direction) * along * SPACING,
                angle: direction,
            };
            let entity = self.world.spawn((p, cell, genome.clone()));
            self.entities.push((id, entity));
            if let Some(a) = previous {
                self.bonds.insert((a, id));
            }
            previous = Some(id);
        }
        if immigration {
            let chaos = self.chaos.as_mut().unwrap();
            chaos.spawned += span as u32;
            if complex {
                chaos.organisms_spawned += 1;
            }
            chaos.event(self.tick, if complex { 4 } else { 3 }, span as u32);
        }
    }

    pub(super) fn record_chaos_history(&mut self) {
        let Some(chaos) = &mut self.chaos else {
            return;
        };
        if self.tick % TICK_RATE != 0 {
            return;
        }
        let mut sample = [0.; 20];
        sample[0] = self.tick as f32 * STEP;
        sample[1] = self.world.len() as f32;
        for (_, (cell, genome)) in self.world.query::<(&Cell, &Genome)>().iter() {
            sample[2 + genome.mode(cell.mode).kind as usize] += 1.;
        }
        if chaos.history.len() == HISTORY_LIMIT {
            chaos.history.remove(0);
        }
        chaos.history.push(sample);
    }
}

/// One bounded edit per exposure; structural edits preserve the slot count.
pub(super) fn mutate(mode: &mut Mode, count: usize, roll: f32, point_only: bool) {
    let scaled = roll * 4096.;
    let choice = scaled as usize % if point_only { 14 } else { 20 };
    let delta = (scaled.fract() - 0.5) * 2.;
    let slot = (scaled.fract() * count as f32) as usize;
    match choice {
        0 => mode.threshold = (mode.threshold + delta * 0.6).clamp(1., 8.),
        1 => mode.speed = (mode.speed + delta * 8.).clamp(0., 60.),
        2 => mode.ratio = (mode.ratio + delta * 0.1).clamp(0.2, 0.8),
        3 => mode.angle = (mode.angle + delta * 0.4).clamp(-3.14, 3.14),
        4 => mode.lifespan = (mode.lifespan + delta * 30.).clamp(20., 1000.),
        5 => mode.nutrient_priority = (mode.nutrient_priority + delta * 0.3).clamp(0.25, 4.),
        6 => mode.density = (mode.density + delta * 0.3).clamp(-3.3, 3.3),
        7 => mode.sensor_gain = (mode.sensor_gain + delta * 0.5).clamp(-16., 16.),
        8 => mode.secretion_rate = (mode.secretion_rate + delta * 0.2).clamp(0., 4.),
        9 => mode.cilia_left = (mode.cilia_left + delta * 6.).clamp(-60., 60.),
        10 => mode.cilia_right = (mode.cilia_right + delta * 6.).clamp(-60., 60.),
        11 => mode.contraction = (mode.contraction + delta * 0.15).clamp(-1., 1.),
        12 => {
            mode.transition_threshold_a = (mode.transition_threshold_a + delta * 0.2).clamp(-8., 8.)
        }
        13 => {
            let channel = scaled as usize % 3;
            if mode.color[0] < 0. {
                mode.color = mode.resolved_color();
            }
            mode.color[channel] = (mode.color[channel] + delta * 0.12).clamp(0., 1.);
        }
        14 => mode.kind = (scaled.fract() * 18.) as u8,
        15 => mode.child_a = slot,
        16 => mode.child_b = slot,
        17 => mode.bind_children = !mode.bind_children,
        18 => mode.keep_a = !mode.keep_a,
        _ => mode.transition_a = slot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> Genome {
        Genome::from_modes(
            [0, 1, 2, 7]
                .into_iter()
                .map(|kind| Mode {
                    kind,
                    can_divide: false,
                    speed: 0.,
                    lifespan: 1000.,
                    ..Mode::default()
                })
                .collect(),
        )
    }

    fn catalog_genomes() -> Vec<Genome> {
        let mobile = |builder: u8, tail: u8| {
            Genome::from_modes(vec![
                Mode {
                    kind: builder,
                    threshold: 1.8,
                    child_a: 0,
                    child_b: 1,
                    bind_children: true,
                    keep_a: true,
                    keep_b: true,
                    max_connections: Some(4),
                    lifespan: 1000.,
                    ..Mode::default()
                },
                Mode {
                    kind: 2,
                    threshold: 2.,
                    can_divide: false,
                    speed: 30.,
                    lifespan: 1000.,
                    ..Mode::default()
                },
                Mode {
                    kind: tail,
                    threshold: 2.,
                    can_divide: false,
                    lifespan: 1000.,
                    ..Mode::default()
                },
            ])
        };
        vec![mobile(0, 0), mobile(1, 5)]
    }

    fn catalog() -> Vec<f32> {
        let genomes = catalog_genomes();
        let mut buffer = vec![ORGANISM_ABI_VERSION, genomes.len() as f32];
        for genome in genomes {
            buffer.push(genome.len() as f32);
            buffer.push(1.);
            buffer.extend(genome.values().into_iter().map(f32::from));
        }
        buffer
    }

    fn world(seed: u32) -> Simulation {
        let mut sim = Simulation::new(seed);
        sim.set_chaos_organisms(&catalog()).unwrap();
        sim.start_chaos_checked(seed, pool());
        sim
    }

    fn advance(sim: &mut Simulation, mut ticks: u32) {
        while ticks > 0 {
            let n = ticks.min(120);
            sim.advance(n);
            ticks -= n;
        }
    }

    #[test]
    fn native_chaos_preset_and_fallback_populate_the_world() {
        let mut preset = Simulation::new(7);
        preset
            .start_chaos_native(7, &pool().values(), &[1., 0.])
            .unwrap();
        assert!(preset.world.len() > 0);
        preset.set_track_owner_scores(true);
        preset.reset();
        assert!(preset.world.len() > 0, "reset keeps the chaos scenario");
        assert_eq!(preset.owner_score(7), 0., "reset clears arena scores");
        preset
            .spawn_owned(100., 100., 0, &pool().values(), 7)
            .unwrap();
        preset.advance(1);
        assert!(
            preset.owner_score(7) > 0.,
            "reset keeps owner score tracking"
        );

        let mut fallback = Simulation::new(11);
        fallback.start_chaos_defaults(11);
        assert!(fallback.world.len() > 0);
        assert!(!fallback.chaos_info().is_empty());
    }

    #[test]
    fn rectangular_worlds_keep_scale_corners_resources_and_exact_saves() {
        let mut sandbox = Simulation::new(91);
        sandbox.set_rectangular_dish(3072., 3072.).unwrap();
        let id = sandbox.spawn(3000., 100., 1, &pool().values()).unwrap();
        let tile = sandbox.boundary.tile(3000., 100.);
        assert_eq!(sandbox.tiles.len(), 48 * 48);
        let food = sandbox.tiles[tile].food;
        sandbox.advance(1);
        assert!(sandbox.tiles[tile].food < food);
        let row = sandbox.snapshot();
        assert_eq!(row[0], id as f32);
        assert!(row[1] > 2900. && row[2] < 200.);
        let mut chaos = Simulation::new(91);
        chaos.start_chaos_in_bounds(91, pool(), 6144., 3072., true);
        assert_eq!(chaos.world_width(), sandbox.world_width() * 2.);
        assert_eq!(chaos.world_size(), sandbox.world_size());
        assert_eq!(chaos.tiles.len(), sandbox.tiles.len() * 2);
        for sim in [&mut sandbox, &mut chaos] {
            let state = sim.encode_state();
            let mut restored = Simulation::new(1);
            restored.restore_checked(&state).unwrap();
            assert_eq!(restored.encode_state(), state);
            sim.advance(120);
            restored.advance(120);
            assert_eq!(sim.encode_state(), restored.encode_state());
            let width = sim.world_width();
            let height = sim.world_size();
            sim.reset();
            assert_eq!(sim.world_width(), width);
            assert_eq!(sim.world_size(), height);
        }
    }

    #[test]
    fn legacy_6144_worlds_load_and_reset_without_rescaling_cells() {
        let mut legacy = Simulation::new(17);
        legacy.start_chaos_in_bounds(17, pool(), 8192., 6144., true);
        legacy.sterilize();
        legacy.spawn(6000., 3000., 1, &pool().values()).unwrap();
        let state = legacy.encode_state();
        let mut restored = Simulation::new(1);
        restored.restore_checked(&state).unwrap();
        assert_eq!(restored.world_width(), 8192.);
        assert_eq!(restored.world_size(), 6144.);
        assert_eq!(restored.encode_state(), state);
        restored.reset();
        assert_eq!(restored.world_width(), 8192.);
        assert_eq!(restored.world_size(), 6144.);
    }

    #[test]
    fn seeded_map_has_ten_biomes_and_large_initial_population_inside_dish() {
        for seed in [0, 1, 73421, u32::MAX] {
            let mut sim = world(seed);
            let center = CHAOS_SIZE / 2.;
            assert_eq!(sim.tiles.len(), CHAOS_GRID * CHAOS_GRID);
            assert_eq!(sim.world_size(), CHAOS_SIZE);
            assert!(sim.world.len() >= 150);
            let mut biomes = std::collections::BTreeSet::new();
            for (i, tile) in sim.tiles.iter().enumerate() {
                let x = (i % CHAOS_GRID) as f32 * 64. + 32.;
                let y = (i / CHAOS_GRID) as f32 * 64. + 32.;
                if libm::hypotf(x - center, y - center) < center {
                    biomes.insert(tile.biome as u32);
                }
            }
            assert_eq!(biomes.len(), 10);
            for (_, p) in sim.world.query::<&Position>().iter() {
                assert!(libm::hypotf(p.x - center, p.y - center) < center - 22.);
            }
            let state = sim.encode_state();
            sim.reset();
            assert_eq!(sim.encode_state(), state);
            assert_ne!(sim.terrain(), world(seed.wrapping_add(1)).terrain());
        }
    }

    #[test]
    fn tile_owned_metabolism_feeds_cells_across_the_chaos_grid() {
        let mut sim = world(71);
        sim.sterilize();
        sim.configure_chaos(false, 10, true).unwrap();
        let mut settings = sim.settings;
        settings.radiation = 0.;
        sim.configure_environment(settings).unwrap();
        for tile in &mut sim.tiles {
            tile.food = 0.;
            tile.capacity = 0.;
        }
        let tile = index(2000., 1500., CHAOS_GRID);
        sim.tiles[tile].food = 0.5;
        sim.tiles[tile].capacity = 1.;
        let regeneration = crate::terrain::BIOMES[sim.tiles[tile].biome as usize][4];
        let id = sim.spawn(2000., 1500., 1, &pool().values()).unwrap();
        sim.advance(1);
        let row = sim.snapshot();
        assert_eq!(row[0], id as f32);
        assert!((row[5] - (1. + 0.18 * STEP * 1.6 - 0.025 * STEP)).abs() < 1e-6);
        assert!(
            (sim.tiles[tile].food - (0.5 + 0.018 * STEP * regeneration - 0.18 * STEP)).abs() < 1e-6
        );
    }

    #[test]
    fn daylight_is_smooth_mild_and_clouds_are_independent_of_spawn_randomness() {
        assert!((daylight(0) - 1.).abs() < 1e-6);
        assert!((daylight(DAY_TICKS / 2) - 0.4).abs() < 1e-6);
        assert_eq!(daylight(0), daylight(DAY_TICKS));
        for tick in 1..DAY_TICKS {
            assert!((daylight(tick) - daylight(tick - 1)).abs() < 0.004);
        }
        let mut a = world(5);
        let mut b = world(5);
        b.configure_chaos(false, 10, true).unwrap();
        let before = clouds(a.seed, 0);
        advance(&mut a, FIRST_SPAWN_TICKS + 1);
        advance(&mut b, FIRST_SPAWN_TICKS + 1);
        assert_ne!(before, clouds(a.seed, a.tick));
        assert_eq!(
            a.tiles
                .iter()
                .map(|t| (t.light, t.radiation))
                .collect::<Vec<_>>(),
            b.tiles
                .iter()
                .map(|t| (t.light, t.radiation))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn immigration_starts_at_six_seconds_and_fifteenth_second_spawns_organism() {
        let mut sim = world(9);
        advance(&mut sim, FIRST_SPAWN_TICKS - 1);
        assert_eq!(sim.chaos.as_ref().unwrap().spawned, 0);
        sim.advance(1);
        assert_eq!(sim.chaos.as_ref().unwrap().spawned, 1);
        advance(&mut sim, ORGANISM_INTERVAL - FIRST_SPAWN_TICKS - 1);
        let before = sim.next_id;
        sim.advance(1);
        let chaos = sim.chaos.as_ref().unwrap();
        assert_eq!(chaos.organisms_spawned, 1);
        assert_eq!(chaos.spawned, 5);
        let event = chaos.events.last().unwrap();
        assert_eq!(event[0], ORGANISM_INTERVAL);
        assert_eq!(event[1], 4);
        assert_eq!(event[2], 3);
        let ids: Vec<_> = (before..before + event[2]).collect();
        assert_eq!(ids.len(), event[2] as usize);
        for pair in ids.windows(2) {
            assert!(
                sim.bonds.contains(&(pair[0], pair[1])),
                "ids {ids:?} bonds {:?}",
                sim.bonds
            );
        }
        assert_eq!(sim.cell_mode(ids[0] as u32), 0);
        assert_eq!(sim.cell_mode(ids[2] as u32), 2);
        sim.configure_chaos(false, 10, true).unwrap();
        let spawned = sim.chaos.as_ref().unwrap().spawned;
        advance(&mut sim, DAY_TICKS / 2);
        assert_eq!(sim.chaos.as_ref().unwrap().spawned, spawned);
    }

    #[test]
    fn spontaneous_arrivals_use_curated_organisms_and_mandatory_singles() {
        let mut sim = world(23);
        let catalog: Vec<_> = catalog_genomes().into_iter().map(|g| g.values()).collect();
        let mut spans = std::collections::BTreeSet::new();
        for _ in 0..24 {
            sim.sterilize();
            let before = sim.next_id;
            sim.chaos_arrival(true, true);
            let count = sim.next_id - before;
            assert!(catalog.contains(&sim.genome(before)));
            assert_eq!(count, 3);
            spans.insert(count);
            for i in 0..count as u32 {
                assert_eq!(sim.cell_mode(before + i), i as usize);
            }
        }
        assert_eq!(spans, [3].into_iter().collect());
        let singles: Vec<_> = single_cells()
            .into_iter()
            .map(|(genome, _)| genome.values())
            .collect();
        let mut kinds = std::collections::BTreeSet::new();
        for _ in 0..240 {
            sim.sterilize();
            let before = sim.next_id;
            sim.chaos_arrival(false, true);
            assert_eq!(sim.next_id - before, 1);
            let values = sim.genome(before);
            assert!(singles.contains(&values));
            kinds.insert(sim.cell_mode(before) as u8);
            let genome = decode_checked(&values).unwrap();
            kinds.insert(genome.mode(0).kind);
        }
        assert_eq!(kinds, [0, 1, 2, 3, 5].into_iter().collect());
    }

    #[test]
    fn catalog_parser_accepts_valid_buffers_and_rejects_broken_ones() {
        let decoded = decode_primordial(&catalog()).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].span, 3);
        assert_eq!(decoded[0].weight, 1);
        assert_eq!(decoded[0].genome.values(), catalog_genomes()[0].values());
        assert!(decode_primordial(&[]).unwrap().is_empty());
        let valid = catalog();
        for (index, value) in [(0, 2.), (1, 1.5), (2, 0.), (3, -1.), (4, 0.), (5, 999.)] {
            let mut broken = valid.clone();
            broken[index] = value;
            assert!(
                decode_primordial(&broken).is_err(),
                "offset {index} should fail"
            );
        }
        let mut truncated = valid.clone();
        truncated.pop();
        assert!(decode_primordial(&truncated).is_err());
        let mut trailing = valid;
        trailing.push(0.);
        assert!(decode_primordial(&trailing).is_err());
    }

    #[test]
    fn singles_ignore_the_editor_pool_and_existing_cells() {
        let mut sim = world(11);
        let id = sim.entities[0].0;
        let existing = sim.genome(id);
        let replacement = Genome::from_modes(vec![Mode {
            kind: 4,
            can_divide: false,
            ..Mode::default()
        }]);
        sim.set_chaos_pool(&replacement.values()).unwrap();
        let singles: Vec<_> = single_cells()
            .into_iter()
            .map(|(genome, _)| genome.values())
            .collect();
        for _ in 0..16 {
            let before = sim.next_id;
            sim.chaos_arrival(false, true);
            assert_eq!(sim.next_id - before, 1);
            assert!(singles.contains(&sim.genome(before)));
            assert_eq!(sim.cell_mode(before), 0);
        }
        assert_eq!(sim.genome(id), existing);
    }

    #[test]
    fn save_restore_keeps_environment_scheduler_pool_history_and_future() {
        let mut sim = world(33);
        advance(&mut sim, 811);
        let state = sim.encode_state();
        let mut restored = Simulation::new(99);
        restored.restore_checked(&state).unwrap();
        restored.set_chaos_organisms(&catalog()).unwrap();
        assert_eq!(state, restored.encode_state());
        for _ in 0..4 {
            sim.advance(120);
            restored.advance(120);
            assert_eq!(sim.encode_state(), restored.encode_state());
        }
        let baseline = sim.chaos.as_ref().unwrap().baseline.clone();
        let food = sim.tiles[0].food / sim.tiles[0].capacity;
        let mut settings = sim.settings;
        settings.food = 2.;
        settings.light = 1.5;
        sim.configure_environment(settings).unwrap();
        assert_eq!(sim.tiles[0].capacity, baseline[0].capacity * 2.);
        assert!((sim.tiles[0].food / sim.tiles[0].capacity - food).abs() < 1e-6);
        assert!((sim.tiles[0].light - baseline[0].light * daylight(sim.tick) * 1.5).abs() < 1e-6);
        let tick = sim.tick;
        let next = sim.chaos.as_ref().unwrap().next_spawn;
        let fields = sim.terrain();
        sim.sterilize();
        assert_eq!(sim.tick, tick);
        assert_eq!(sim.terrain(), fields);
        assert_eq!(sim.chaos.as_ref().unwrap().next_spawn, next);
        assert!(sim.bonds.is_empty());
        advance(&mut sim, next - tick);
        assert!(!sim.entities.is_empty());
        let mut settings = sim.settings;
        settings.food = 5.;
        settings.nitrogen = 5.;
        settings.light = 5.;
        settings.radiation = 5.;
        sim.configure_environment(settings).unwrap();
        assert!(sim.tiles.iter().any(|t| t.capacity > 5.));
        assert!(sim.tiles.iter().any(|t| t.nitrate_capacity > 5.));
        let state = sim.encode_state();
        restored.restore_checked(&state).unwrap();
        assert_eq!(restored.encode_state(), state);
    }

    #[test]
    fn mutations_keep_canonical_genomes_and_point_only_preserves_structure() {
        for point_only in [false, true] {
            let mut genome = pool();
            for i in 0..16_384 {
                let roll = i as f32 / 16384.;
                let mode = &mut genome.modes_mut()[i % 4];
                let before = *mode;
                mutate(mode, 4, roll, point_only);
                if point_only {
                    assert_eq!(mode.kind, before.kind);
                    assert_eq!(mode.child_a, before.child_a);
                    assert_eq!(mode.child_b, before.child_b);
                    assert_eq!(mode.bind_children, before.bind_children);
                    assert_eq!(mode.transition_a, before.transition_a);
                }
                let values = genome.values();
                assert_eq!(decode_checked(&values).unwrap().values(), values);
            }
        }
        let targets: std::collections::BTreeSet<_> = (0..1000)
            .map(|i| {
                let mut mode = Mode::default();
                mutate(
                    &mut mode,
                    1000,
                    (15. + (i as f32 + 0.5) / 1000.) / 4096.,
                    false,
                );
                mode.child_a
            })
            .collect();
        assert_eq!(
            targets.len(),
            1000,
            "every existing slot can become a daughter target"
        );
    }

    #[test]
    fn malformed_chaos_saves_do_not_replace_world_and_old_v5_still_loads() {
        let mut sim = world(47);
        let state = sim.encode_state();
        for (offset, value) in [(19, 0.), (20, 2.), (21, 0.), (23, 0.), (25, 0.)] {
            let mut corrupt = state.clone();
            corrupt[offset] = value;
            assert!(sim.restore_checked(&corrupt).is_err());
            assert_eq!(sim.encode_state(), state);
        }
        let sandbox = Simulation::new(17);
        let mut v5 = sandbox.encode_state();
        v5[0] = 5.;
        v5.drain(19..22);
        sim.restore_checked(&v5).unwrap();
        assert_eq!(sim.world_size(), 3072.);
        assert!(sim.chaos.is_none());
        assert_eq!(sim.encode_state(), sandbox.encode_state());
    }

    #[test]
    fn full_population_skips_arrival_and_history_is_bounded() {
        let mut sim = world(61);
        while sim.world.len() < LIMIT as u32 {
            let id = sim.next_id;
            sim.next_id += 1;
            let entity = sim.world.spawn((
                Position {
                    x: 3000.,
                    y: 3000.,
                    angle: 0.,
                },
                Cell::new(id, 0),
                pool(),
            ));
            sim.entities.push((id, entity));
        }
        let next = sim.next_id;
        sim.chaos_arrival(false, true);
        sim.chaos_arrival(true, true);
        assert_eq!(sim.next_id, next);
        assert_eq!(sim.chaos.as_ref().unwrap().spawned, 0);
        sim.sterilize();
        for second in 1..400 {
            sim.tick = second * TICK_RATE;
            sim.record_chaos_history();
        }
        assert_eq!(sim.chaos.as_ref().unwrap().history.len(), HISTORY_LIMIT);
    }
}
