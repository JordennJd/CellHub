//! Resource uptake, exchange and protection in multicellular organisms.

use super::Simulation;
use crate::cell_types::{GAMETE, NITROCYTE, protects_neighbours};
use crate::components::{Cell, MIN_DIVISION_AGE};
use crate::genome::{Genome, MAX_MODES, Mode};
use crate::terrain::Tile;
use hecs::Entity;
use std::collections::{BTreeMap, HashSet};

const STARVING_MASS: f32 = 0.65;
const STARVING_PRIORITY_MULTIPLIER: f64 = 4.;
const NITROGEN_CONSUMPTION: f32 = 0.025;
const NITROGEN_FIXATION: f32 = 0.10;
const NITROGEN_UPTAKE: f32 = 0.35;
// The environment stores a concentration, in units per tile. Cell stores are
// normalized separately: one full reserve consumes this much concentration.
const NITRATE_PER_RESERVE: f32 = 0.01;

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

    pub(super) fn feed_nitrogen(&mut self, cell: &mut Cell, kind: u8, step: f32) {
        let fixation = if kind == NITROCYTE {
            NITROGEN_FIXATION
        } else {
            0.
        };
        cell.nitrogen = (cell.nitrogen + step * (fixation - NITROGEN_CONSUMPTION)).clamp(0., 1.);
        let uptake = (step * NITROGEN_UPTAKE * self.nitrates)
            .min(self.nitrates / NITRATE_PER_RESERVE)
            .min(1. - cell.nitrogen);
        self.nitrates = (self.nitrates - uptake * NITRATE_PER_RESERVE).max(0.);
        cell.nitrogen += uptake;
    }
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
