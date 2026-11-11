//! Composable dynamic environment, editor-slot immigration and observation.

use super::Simulation;
use super::boundary::Boundary;
use crate::components::{Cell, LIMIT, Position, radius};
use crate::genome::{Genome, Mode, decode_checked};
#[cfg(test)]
use crate::terrain::{CHAOS_GRID, index};
use crate::terrain::{CHAOS_SIZE, Tile, chaos_map, random};
use wasm_bindgen::prelude::*;

pub(super) const HISTORY_LIMIT: usize = 300;
pub(super) const EVENT_LIMIT: usize = 80;

pub(super) struct Chaos {
    pub(super) pool: Genome,
    pub(super) initial_pool: Genome,
    /// Deterministic procedurally varied organisms, mixed with the editor pool.
    pub(super) wild: Vec<Genome>,
    pub(super) baseline: Vec<Tile>,
    pub(super) rng: u32,
    pub(super) enabled: bool,
    pub(super) rate: u32,
    pub(super) viable: bool,
    pub(super) next_spawn: u32,
    pub(super) spawned: u32,
    pub(super) organisms: u32,
    pub(super) history: Vec<[f32; 20]>,
    pub(super) events: Vec<[u32; 3]>,
}

impl Chaos {
    pub(super) fn new(seed: u32, pool: Genome) -> Self {
        Self {
            initial_pool: pool.clone(),
            pool,
            wild: wild_genomes(seed),
            baseline: chaos_map(seed),
            rng: (seed ^ 0xa341316c).max(1),
            enabled: true,
            rate: 10,
            viable: true,
            next_spawn: 180,
            spawned: 0,
            organisms: 0,
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
    0.7 + 0.3 * libm::cosf((tick % 1800) as f32 / 1800. * std::f32::consts::TAU)
}

/// Analytic seeded orbits keep clouds independent of biology and spawn RNG.
#[cfg(test)]
pub(super) fn clouds(seed: u32, tick: u32) -> [[f32; 6]; 3] {
    clouds_rect(seed, tick, CHAOS_SIZE, CHAOS_SIZE)
}

const WILD_TEMPLATES: usize = 7;
const WILD_VARIANTS: usize = 2;

/// Deterministic wild organisms for spontaneous generation. They are rebuilt
/// from the world seed, so checkpoints do not need to store them separately.
pub(super) fn wild_genomes(seed: u32) -> Vec<Genome> {
    (0..WILD_TEMPLATES * WILD_VARIANTS)
        .map(|index| wild_genome(seed, index))
        .collect()
}

fn wild_genome(seed: u32, index: usize) -> Genome {
    let template = index % WILD_TEMPLATES;
    let variant = index / WILD_TEMPLATES;
    let mut rng = seed
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add(index as u32 + 1)
        .max(1);
    let mut photo = Mode {
        kind: 0,
        child_a: 0,
        child_b: 1,
        angle: 0.,
        lifespan: 1000.,
        bind_children: !matches!(template, 1 | 4 | 5),
        ..Mode::default()
    };
    let mut modes = match template {
        // A photo cell grows into a mobile chain.
        0 => {
            photo.threshold = 1.8;
            vec![
                photo,
                Mode {
                    kind: 2,
                    can_divide: false,
                    speed: 42.,
                    angle: 0.,
                    lifespan: 1000.,
                    ..Mode::default()
                },
            ]
        }
        // A glued photo colony.
        1 => {
            photo.threshold = 1.6;
            vec![
                photo,
                Mode {
                    kind: 9,
                    threshold: 2.,
                    child_a: 0,
                    child_b: 1,
                    max_connections: Some(4),
                    angle: 0.,
                    lifespan: 1000.,
                    bind_children: true,
                    ..Mode::default()
                },
            ]
        }
        // Photosynthesis plus digestion and nitrogen exchange.
        2 => {
            photo.threshold = 1.9;
            vec![
                photo,
                Mode {
                    kind: 1,
                    threshold: 2.4,
                    child_a: 0,
                    child_b: 2,
                    keep_b: true,
                    angle: 0.,
                    lifespan: 1000.,
                    ..Mode::default()
                },
                Mode {
                    kind: 6,
                    can_divide: false,
                    angle: 0.,
                    lifespan: 1000.,
                    ..Mode::default()
                },
            ]
        }
        // A mobile predator.
        3 => {
            photo.kind = 1;
            photo.threshold = 2.;
            vec![
                photo,
                Mode {
                    kind: 2,
                    can_divide: false,
                    speed: 38.,
                    angle: 0.,
                    lifespan: 1000.,
                    ..Mode::default()
                },
            ]
        }
        // A protected glue/keratin colony.
        4 => {
            photo.threshold = 1.7;
            vec![
                photo,
                Mode {
                    kind: 9,
                    threshold: 2.2,
                    child_a: 0,
                    child_b: 2,
                    keep_b: true,
                    max_connections: Some(3),
                    angle: 0.,
                    lifespan: 1000.,
                    ..Mode::default()
                },
                Mode {
                    kind: 4,
                    can_divide: false,
                    angle: 0.,
                    lifespan: 1000.,
                    ..Mode::default()
                },
            ]
        }
        // A buoyant photo colony; useful when gravity is enabled.
        5 => {
            photo.threshold = 1.7;
            vec![
                photo,
                Mode {
                    kind: 8,
                    can_divide: false,
                    density: -2.2,
                    angle: 0.,
                    lifespan: 1000.,
                    ..Mode::default()
                },
            ]
        }
        // A predator carrying an infectious virocyte.
        _ => {
            photo.kind = 1;
            photo.threshold = 2.;
            vec![
                photo,
                Mode {
                    kind: 3,
                    can_divide: false,
                    angle: 0.,
                    lifespan: 1000.,
                    ..Mode::default()
                },
            ]
        }
    };
    for mode in &mut modes {
        mode.threshold = (mode.threshold + (variant as f32 - 0.5) * 0.35
            + (random(&mut rng) - 0.5) * 0.5)
            .clamp(1.2, 4.);
        if mode.kind == 2 {
            mode.speed = (mode.speed + (random(&mut rng) - 0.5) * 12.).clamp(22., 55.);
        }
        mode.ratio = (mode.ratio + (random(&mut rng) - 0.5) * 0.16).clamp(0.3, 0.7);
        mode.angle = (mode.angle + (random(&mut rng) - 0.5) * 0.9).clamp(-1.4, 1.4);
        mode.child_angle_a = (random(&mut rng) - 0.5) * 1.6;
        mode.child_angle_b = (random(&mut rng) - 0.5) * 1.6;
        mode.lifespan = (mode.lifespan + (random(&mut rng) - 0.5) * 280.).clamp(360., 1000.);
        mode.nutrient_priority = (random(&mut rng) * 0.9 + 0.8).clamp(0.8, 1.7);
        if mode.can_divide {
            mode.max_connections = mode
                .max_connections
                .or(Some(if mode.kind == 9 { 4 } else { 2 }));
            mode.keep_a = random(&mut rng) < 0.35;
            mode.keep_b = random(&mut rng) < 0.35;
        }
    }
    Genome::from_modes(modes)
}

fn clouds_rect(seed: u32, tick: u32, width: f32, height: f32) -> [[f32; 6]; 3] {
    let mut rng = (seed ^ 0xc8013ea4).max(1);
    std::array::from_fn(|i| {
        let phase = random(&mut rng) * std::f32::consts::TAU;
        let period = 100. + random(&mut rng) * 100.;
        let radius = 650. + random(&mut rng) * 350.;
        let a = phase + tick as f32 / 30. / period * std::f32::consts::TAU;
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
                chaos.next_spawn = self.tick + 1800 / rate;
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
            chaos.next_spawn.saturating_sub(self.tick) as f64 / 30.,
            (450 - self.tick % 450) as f64 / 30.,
            chaos.spawned as f64,
            chaos.organisms as f64,
            chaos.pool.len() as f64,
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
        *self = Self::new(seed);
        self.set_division_event_tracking(track_divisions);
        self.boundary = Boundary::Chaos;
        self.chaos = Some(Chaos::new(seed, pool));
        self.tiles = self.chaos.as_ref().unwrap().baseline.clone();
        self.spatial = super::spatial::SpatialGrid::with_size(width, height);
        if rectangle {
            self.set_rectangular_dish(width, height)
                .expect("validated chaos bounds");
        }
        self.refresh_chaos_fields();
        let pool = self.chaos.as_ref().unwrap().pool.clone();
        for i in 0..132 {
            self.chaos_arrival_with(pool.clone(), i % 11 == 0, false);
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
        if self.tick % 1800 == 450 {
            self.chaos.as_mut().unwrap().event(self.tick, 2, 0);
        }
        if self.tick % 1800 == 1350 {
            self.chaos.as_mut().unwrap().event(self.tick, 1, 0);
        }
        let chaos = self.chaos.as_mut().unwrap();
        let due = self.tick >= chaos.next_spawn;
        if due {
            chaos.next_spawn = self.tick + 1800 / chaos.rate;
        }
        let organism = self.tick % 450 == 0;
        if chaos.enabled && (due || organism) {
            self.chaos_arrival(organism, true);
        }
    }

    /// Half of the spontaneous organisms use a procedural wild genome; the
    /// other half copy the current editor pool.
    fn chaos_arrival(&mut self, organism: bool, immigration: bool) {
        let genome = if organism {
            self.wild_source()
                .unwrap_or_else(|| self.chaos.as_ref().unwrap().pool.clone())
        } else {
            self.chaos.as_ref().unwrap().pool.clone()
        };
        self.chaos_arrival_with(genome, organism, immigration);
    }

    fn wild_source(&mut self) -> Option<Genome> {
        let (rng, len) = {
            let chaos = self.chaos.as_ref()?;
            (chaos.rng, chaos.wild.len())
        };
        if len == 0 {
            return None;
        }
        let mut rng = rng;
        let roll = random(&mut rng);
        let index = (random(&mut rng) * len as f32) as usize;
        self.chaos.as_mut().unwrap().rng = rng;
        if roll < 0.5 {
            Some(self.chaos.as_ref().unwrap().wild[index].clone())
        } else {
            None
        }
    }

    fn chaos_arrival_with(&mut self, genome: Genome, organism: bool, immigration: bool) {
        let chaos = self.chaos.as_mut().unwrap();
        let count = if organism {
            4 + (random(&mut chaos.rng) * 4.) as usize
        } else {
            1
        };
        if self.world.len() as usize + count > LIMIT {
            return;
        }
        let sources: Vec<_> = (0..genome.len())
            .filter(|&i| matches!(genome.mode(i).kind, 0 | 1))
            .collect();
        // Viability constrains the initial slot, never changes the editor genome.
        let first = if chaos.viable && !sources.is_empty() {
            sources[(random(&mut chaos.rng) * sources.len() as f32) as usize]
        } else {
            (random(&mut chaos.rng) * genome.len() as f32) as usize
        };
        let mut center = None;
        for _ in 0..64 {
            let x = 80. + random(&mut chaos.rng) * (self.boundary.width() - 160.);
            let y = 80. + random(&mut chaos.rng) * (self.boundary.size() - 160.);
            if !matches!(self.boundary, Boundary::Rectangle { .. })
                && libm::hypotf(x - CHAOS_SIZE / 2., y - CHAOS_SIZE / 2.) > CHAOS_SIZE / 2. - 100.
            {
                continue;
            }
            if self
                .world
                .query::<(&Position, &Cell)>()
                .iter()
                .any(|(_, (p, cell))| {
                    let clearance = 54. + radius(cell.mass) + radius(1.8);
                    (p.x - x).powi(2) + (p.y - y).powi(2) < clearance * clearance
                })
            {
                continue;
            }
            let tile = self.tiles[self.boundary.tile(x, y)];
            if chaos.viable && tile.radiation > 0.5 {
                continue;
            }
            center = Some((x, y));
            break;
        }
        let Some((x, y)) = center else {
            return;
        };
        let mut previous = None;
        let direction = random(&mut chaos.rng) * std::f32::consts::TAU;
        for i in 0..count {
            let mode = if i == 0 {
                first
            } else {
                (random(&mut chaos.rng) * genome.len() as f32) as usize
            };
            let id = self.next_id;
            self.next_id += 1;
            let mut cell = Cell::new(id, mode);
            cell.mass = if organism { 1.8 } else { 1.4 };
            let offset = (i as f32 - (count - 1) as f32 / 2.) * 25.;
            let p = Position {
                x: x + libm::cosf(direction) * offset,
                y: y + libm::sinf(direction) * offset,
                angle: random(&mut chaos.rng) * std::f32::consts::TAU,
            };
            let entity = self.world.spawn((p, cell, genome.clone()));
            self.entities.push((id, entity));
            if let Some(a) = previous {
                self.bonds.insert((a, id));
            }
            previous = Some(id);
        }
        if immigration {
            chaos.spawned += count as u32;
            if organism {
                chaos.organisms += 1;
            }
            chaos.event(self.tick, if organism { 4 } else { 3 }, count as u32);
        }
    }

    pub(super) fn record_chaos_history(&mut self) {
        let Some(chaos) = &mut self.chaos else {
            return;
        };
        if self.tick % 30 != 0 {
            return;
        }
        let mut sample = [0.; 20];
        sample[0] = self.tick as f32 / 30.;
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

    fn world(seed: u32) -> Simulation {
        let mut sim = Simulation::new(seed);
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
    fn rectangular_worlds_keep_scale_corners_resources_and_exact_saves() {
        let mut sandbox = Simulation::new(91);
        sandbox.set_rectangular_dish(6144., 3072.).unwrap();
        let id = sandbox.spawn(6000., 100., 1, &pool().values()).unwrap();
        let tile = sandbox.boundary.tile(6000., 100.);
        assert_eq!(sandbox.tiles.len(), 96 * 48);
        let food = sandbox.tiles[tile].food;
        sandbox.advance(1);
        assert!(sandbox.tiles[tile].food < food);
        let row = sandbox.snapshot();
        assert_eq!(row[0], id as f32);
        assert!(row[1] > 5900. && row[2] < 200.);
        let mut chaos = Simulation::new(91);
        chaos.start_chaos_in_bounds(91, pool(), 12288., 6144., true);
        assert_eq!(chaos.world_width(), sandbox.world_width() * 2.);
        assert_eq!(chaos.world_size(), sandbox.world_size() * 2.);
        assert_eq!(chaos.tiles.len(), sandbox.tiles.len() * 4);
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
    fn seeded_map_has_ten_biomes_and_large_initial_population_inside_dish() {
        for seed in [0, 1, 73421, u32::MAX] {
            let mut sim = world(seed);
            assert_eq!(sim.tiles.len(), 96 * 96);
            assert_eq!(sim.world_size(), 6144.);
            assert!(sim.world.len() >= 150);
            let mut biomes = std::collections::BTreeSet::new();
            for (i, tile) in sim.tiles.iter().enumerate() {
                let x = (i % 96) as f32 * 64. + 32.;
                let y = (i / 96) as f32 * 64. + 32.;
                if libm::hypotf(x - 3072., y - 3072.) < 3072. {
                    biomes.insert(tile.biome as u32);
                }
            }
            assert_eq!(biomes.len(), 10);
            for (_, p) in sim.world.query::<&Position>().iter() {
                assert!(libm::hypotf(p.x - 3072., p.y - 3072.) < 3050.);
            }
            let state = sim.encode_state();
            sim.reset();
            assert_eq!(sim.encode_state(), state);
            assert_ne!(sim.terrain(), world(seed.wrapping_add(1)).terrain());
        }
    }

    #[test]
    fn tile_owned_metabolism_feeds_cells_beyond_the_classic_world_size() {
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
        let tile = index(5000., 3000., CHAOS_GRID);
        sim.tiles[tile].food = 0.5;
        sim.tiles[tile].capacity = 1.;
        let regeneration = crate::terrain::BIOMES[sim.tiles[tile].biome as usize][4];
        let id = sim.spawn(5000., 3000., 1, &pool().values()).unwrap();
        sim.advance(1);
        let row = sim.snapshot();
        assert_eq!(row[0], id as f32);
        assert!((row[5] - (1. + 0.18 / 30. * 1.6 - 0.025 / 30.)).abs() < 1e-6);
        assert!(
            (sim.tiles[tile].food - (0.5 + 0.018 / 30. * regeneration - 0.18 / 30.)).abs() < 1e-6
        );
    }

    #[test]
    fn daylight_is_smooth_mild_and_clouds_are_independent_of_spawn_randomness() {
        assert!((daylight(0) - 1.).abs() < 1e-6);
        assert!((daylight(900) - 0.4).abs() < 1e-6);
        assert_eq!(daylight(0), daylight(1800));
        for tick in 1..1800 {
            assert!((daylight(tick) - daylight(tick - 1)).abs() < 0.002);
        }
        let mut a = world(5);
        let mut b = world(5);
        b.configure_chaos(false, 10, true).unwrap();
        let before = clouds(a.seed, 0);
        advance(&mut a, 181);
        advance(&mut b, 181);
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
        advance(&mut sim, 179);
        assert_eq!(sim.chaos.as_ref().unwrap().spawned, 0);
        sim.advance(1);
        assert_eq!(sim.chaos.as_ref().unwrap().spawned, 1);
        advance(&mut sim, 270);
        let chaos = sim.chaos.as_ref().unwrap();
        assert_eq!(chaos.organisms, 1);
        assert!((6..=9).contains(&chaos.spawned));
        let event = chaos.events.last().unwrap();
        assert_eq!(event[0], 450);
        assert_eq!(event[1], 4);
        assert!((4..=7).contains(&event[2]));
        let ids: Vec<_> = sim
            .entities
            .iter()
            .map(|&(id, _)| id)
            .filter(|&id| id >= sim.next_id - event[2])
            .collect();
        assert_eq!(ids.len(), event[2] as usize);
        for pair in ids.windows(2) {
            assert!(sim.bonds.contains(&(pair[0], pair[1])));
        }
        sim.configure_chaos(false, 10, true).unwrap();
        let spawned = sim.chaos.as_ref().unwrap().spawned;
        advance(&mut sim, 901);
        assert_eq!(sim.chaos.as_ref().unwrap().spawned, spawned);
    }

    #[test]
    fn spontaneous_organisms_mix_wild_and_editor_genomes() {
        let mut sim = world(23);
        let editor = sim.chaos.as_ref().unwrap().pool.values();
        let wild: Vec<_> = sim
            .chaos
            .as_ref()
            .unwrap()
            .wild
            .iter()
            .map(Genome::values)
            .collect();
        assert!(wild.len() >= 7);
        let (mut editor_seen, mut wild_seen) = (false, false);
        for _ in 0..40 {
            sim.sterilize();
            let before = sim.next_id;
            sim.chaos_arrival(true, true);
            let count = sim.next_id - before;
            assert!((4..=7).contains(&count));
            let genome = sim.genome(before);
            if genome == editor {
                editor_seen = true;
            } else if wild.contains(&genome) {
                wild_seen = true;
            } else {
                panic!("organism used an unexpected genome");
            }
        }
        assert!(editor_seen && wild_seen);
    }

    #[test]
    fn wild_genomes_are_deterministic_valid_and_viable() {
        let first = wild_genomes(5);
        let second = wild_genomes(5);
        assert_eq!(
            first.iter().map(Genome::values).collect::<Vec<_>>(),
            second.iter().map(Genome::values).collect::<Vec<_>>()
        );
        assert_ne!(first[0].values(), wild_genomes(6)[0].values());
        for genome in first {
            assert!((2..=3).contains(&genome.len()));
            assert_eq!(decode_checked(&genome.values()).unwrap().values(), genome.values());
            assert!((0..genome.len()).any(|i| matches!(genome.mode(i).kind, 0 | 1)));
        }
    }

    #[test]
    fn current_editor_pool_is_used_without_mutating_existing_cells() {
        let mut sim = world(11);
        let id = sim.entities[0].0;
        let existing = sim.genome(id);
        let replacement = Genome::from_modes(vec![Mode {
            kind: 4,
            can_divide: false,
            ..Mode::default()
        }]);
        sim.set_chaos_pool(&replacement.values()).unwrap();
        let before = sim.next_id;
        sim.chaos_arrival_with(replacement.clone(), true, true);
        assert_eq!(sim.genome(id), existing);
        for id in before..sim.next_id {
            assert_eq!(sim.genome(id), replacement.values());
            assert_eq!(sim.cell_mode(id), 0);
        }
        assert!((4..=7).contains(&(sim.next_id - before)));
    }

    #[test]
    fn save_restore_keeps_environment_scheduler_pool_history_and_future() {
        let mut sim = world(33);
        advance(&mut sim, 811);
        let state = sim.encode_state();
        let mut restored = Simulation::new(99);
        restored.restore_checked(&state).unwrap();
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
            sim.tick = second * 30;
            sim.record_chaos_history();
        }
        assert_eq!(sim.chaos.as_ref().unwrap().history.len(), HISTORY_LIMIT);
    }
}
