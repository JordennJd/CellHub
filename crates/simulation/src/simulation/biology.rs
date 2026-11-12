//! Resource uptake, exchange and protection in multicellular organisms.

use super::Simulation;
use crate::cell_types::{GAMETE, NITROCYTE, protects_neighbours};
use crate::components::{Cell, MIN_DIVISION_AGE, Position, STEP};
use crate::genome::{Genome, MAX_MODES, Mode};
use crate::terrain::Tile;
use hecs::Entity;
use std::collections::{BTreeMap, HashSet};

const STARVING_MASS: f32 = 0.65;
const STARVING_PRIORITY_MULTIPLIER: f64 = 4.;
const NITROGEN_MAINTENANCE: f32 = 0.024;
const CAMPAIGN_NITROGEN_MAINTENANCE: f32 = 0.025;
const NITROGEN_FIXATION: f32 = 0.10;
const NITROGEN_UPTAKE: f32 = 0.35;
const NITROGEN_PER_BIOMASS: f32 = 0.65;
pub(super) const NITROGEN_FOR_DAUGHTERS: f32 = 0.5;
// Substrate concentration and a cell's fractional reserve use different units.
const NITRATE_PER_RESERVE: f32 = 1.6;
const CAMPAIGN_NITRATE_PER_RESERVE: f32 = 0.01;

/// Gamete metadata shared by physical contacts and the subsequent fusion pass.
#[derive(Clone, Copy)]
pub(super) struct GameteContact {
    mode: usize,
    compatible_mode: usize,
    legacy_group: Option<u8>,
    genome_len: usize,
    mature: bool,
}

impl GameteContact {
    pub(super) fn new(cell: &Cell, mode: &Mode, genome_len: usize) -> Option<Self> {
        (mode.kind == GAMETE).then_some(Self {
            mode: cell.mode,
            compatible_mode: mode.compatible_mode,
            legacy_group: mode.legacy_gamete_group,
            genome_len,
            mature: cell.age + 0.000001 >= MIN_DIVISION_AGE,
        })
    }

    /// Fusion reach as a fraction of the sum of radii. Age is checked
    /// separately so potential partners can stay in contact while maturing.
    pub(super) fn fusion_scale(self, other: Self) -> Option<f32> {
        if self.genome_len.checked_add(other.genome_len)? > MAX_MODES {
            return None;
        }
        match (self.legacy_group, other.legacy_group) {
            (None, None) => {
                let compatible =
                    self.compatible_mode == other.mode && other.compatible_mode == self.mode;
                Some(if compatible { 1.03 } else { 0.65 })
            }
            (Some(a), Some(b)) if a == b => Some(1.03),
            _ => None,
        }
    }

    pub(super) fn mature(self) -> bool {
        self.mature
    }
}

fn effective_priority(mode: &Mode, mass: f32) -> f64 {
    let base = f64::from(mode.nutrient_priority);
    if mode.prioritize_when_starving && mass < STARVING_MASS {
        base * STARVING_PRIORITY_MULTIPLIER
    } else {
        base
    }
}

impl Tile {
    pub(super) fn regenerate_nitrates(&mut self, step: f32) {
        self.nitrates =
            (self.nitrates + step * 0.08 * self.nitrate_capacity).min(self.nitrate_capacity);
    }

    pub(super) fn feed_campaign_nitrogen(&mut self, cell: &mut Cell, kind: u8) {
        use_nitrogen(cell, kind, CAMPAIGN_NITROGEN_MAINTENANCE);
        let demand = (STEP * NITROGEN_UPTAKE * self.nitrates).min(1. - cell.nitrogen);
        cell.nitrogen += self.withdraw_nitrate(demand, CAMPAIGN_NITRATE_PER_RESERVE);
    }

    fn withdraw_nitrate(&mut self, demand: f32, nitrate_per_reserve: f32) -> f32 {
        let uptake = demand.min(self.nitrates / nitrate_per_reserve);
        self.nitrates = (self.nitrates - uptake * nitrate_per_reserve).max(0.);
        uptake
    }
}

fn use_nitrogen(cell: &mut Cell, kind: u8, maintenance: f32) {
    let fixation = if kind == NITROCYTE {
        NITROGEN_FIXATION
    } else {
        0.
    };
    cell.nitrogen = (cell.nitrogen + STEP * (fixation - maintenance)).clamp(0., 1.);
}

/// The quadratic weights vary continuously with position and sum to one.
/// Nearby cells share the same stores even on opposite sides of a tile edge.
fn uptake_axis(position: f32, length: usize) -> [(usize, f32); 3] {
    let coordinate = position / 64.;
    let center = (coordinate as usize).min(length - 1);
    let offset = coordinate - center as f32 - 0.5;
    [
        (center.saturating_sub(1), 0.5 * (0.5 - offset).powi(2)),
        (center, 0.75 - offset * offset),
        ((center + 1).min(length - 1), 0.5 * (0.5 + offset).powi(2)),
    ]
}

pub(super) fn feed_nitrogen(
    tiles: &mut [Tile],
    columns: usize,
    position: Position,
    cell: &mut Cell,
    kind: u8,
) {
    use_nitrogen(cell, kind, NITROGEN_MAINTENANCE);
    let room = 1. - cell.nitrogen;
    if room <= 0. {
        return;
    }
    let xs = uptake_axis(position.x, columns);
    let ys = uptake_axis(position.y, tiles.len() / columns);
    let mut requests = [(0, 0.); 9];
    let mut total = 0.;
    for ((row, row_weight), line) in ys.into_iter().zip(requests.chunks_exact_mut(3)) {
        for ((column, column_weight), request) in xs.into_iter().zip(line) {
            let index = row * columns + column;
            let uptake =
                (STEP * NITROGEN_UPTAKE) * row_weight * column_weight * tiles[index].nitrates;
            *request = (index, uptake);
            total += uptake;
        }
    }
    if total > 0. {
        let fraction = (room / total).min(1.);
        let uptake = requests
            .into_iter()
            .map(|(index, uptake)| {
                tiles[index].withdraw_nitrate(uptake * fraction, NITRATE_PER_RESERVE)
            })
            .sum::<f32>();
        cell.nitrogen = (cell.nitrogen + uptake).min(1.);
    }
}

/// Synthesis of new biomass draws on the cell's nitrogen reserve. The cell
/// retains the share required to provision its daughters.
pub(super) fn assimilate_growth(cell: &mut Cell, gain: f32, maintenance: f32) -> f32 {
    let available = (cell.nitrogen - NITROGEN_FOR_DAUGHTERS).max(0.);
    let growth = (gain - maintenance)
        .max(0.)
        .min(available / NITROGEN_PER_BIOMASS);
    if growth > 0. {
        cell.nitrogen = (cell.nitrogen - growth * NITROGEN_PER_BIOMASS).max(NITROGEN_FOR_DAUGHTERS);
    }
    gain.min(maintenance + growth)
}

impl Simulation {
    pub(super) fn bond_degrees(&self) -> BTreeMap<u32, usize> {
        let mut degree = BTreeMap::new();
        for &(a, b) in &self.bonds {
            *degree.entry(a).or_default() += 1;
            *degree.entry(b).or_default() += 1;
        }
        degree
    }

    /// Diffuse mass and nitrogen across bonds from one common snapshot. Mass
    /// moves toward the priority-weighted split. Degree normalization bounds
    /// total outgoing flow even when a donor is surrounded by high priorities.
    pub(super) fn exchange_bond_energy(&mut self, fraction: f64, degree: &BTreeMap<u32, usize>) {
        if self.bonds.is_empty() {
            return;
        }
        let bodies: BTreeMap<_, _> = self
            .world
            .query::<(&Cell, &Genome)>()
            .iter()
            .map(|(entity, (cell, genome))| {
                (
                    cell.id,
                    (
                        entity,
                        f64::from(cell.mass),
                        f64::from(cell.nitrogen),
                        effective_priority(genome.mode(cell.mode), cell.mass),
                    ),
                )
            })
            .collect();
        let fraction = fraction.clamp(0., 0.5);
        let mut deltas = BTreeMap::<u32, (f64, f64)>::new();
        for &(a, b) in &self.bonds {
            let (_, mass_a, nitrogen_a, priority_a) = bodies[&a];
            let (_, mass_b, nitrogen_b, priority_b) = bodies[&b];
            let share = fraction / degree[&a].max(degree[&b]) as f64;
            let mass_flow = 2. * (mass_a * priority_b - mass_b * priority_a) * share
                / (priority_a + priority_b);
            let nitrogen_flow = (nitrogen_a - nitrogen_b) * share;
            let (mass, nitrogen) = deltas.entry(a).or_default();
            *mass -= mass_flow;
            *nitrogen -= nitrogen_flow;
            let (mass, nitrogen) = deltas.entry(b).or_default();
            *mass += mass_flow;
            *nitrogen += nitrogen_flow;
        }
        for (id, (mass_delta, nitrogen_delta)) in deltas {
            let (entity, mass, nitrogen, _) = bodies[&id];
            let mut cell = self.world.get::<&mut Cell>(entity).unwrap();
            cell.mass = (mass + mass_delta) as f32;
            cell.nitrogen = (nitrogen + nitrogen_delta) as f32;
        }
    }

    /// Keratin protects itself and its immediate neighbors, never the whole graph.
    pub(super) fn keratin_protection(&self, removed: &HashSet<Entity>) -> HashSet<u32> {
        let keratin: HashSet<_> = self
            .world
            .query::<(&Cell, &Genome)>()
            .iter()
            .filter(|(entity, (cell, genome))| {
                !removed.contains(entity) && protects_neighbours(genome.mode(cell.mode).kind)
            })
            .map(|(_, (cell, _))| cell.id)
            .collect();
        let mut protected = keratin.clone();
        for &(a, b) in &self.bonds {
            if keratin.contains(&a) {
                protected.insert(b);
            }
            if keratin.contains(&b) {
                protected.insert(a);
            }
        }
        protected
    }
}
