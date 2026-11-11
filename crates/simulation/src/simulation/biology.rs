//! Resource exchange and direct protection in multicellular organisms.

use super::Simulation;
use crate::cell_types::{LIPOCYTE, blocks_signals, protects_neighbours};
use crate::components::Cell;
use crate::genome::Genome;
use hecs::Entity;
use std::collections::{BTreeMap, HashSet};

impl Simulation {
    /// Diffuse mass across bonds from one common snapshot, then commit together.
    /// Degree-normalized flow keeps the update a convex combination of existing
    /// masses, so dense graphs cannot overdraw a donor or amplify total energy.
    pub(super) fn exchange_bond_energy(&mut self, fraction: f64) {
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
                    (entity, cell.mass as f64, genome.mode(cell.mode).kind),
                )
            })
            .collect();
        let mut degree = BTreeMap::<u32, usize>::new();
        for &(a, b) in &self.bonds {
            *degree.entry(a).or_default() += 1;
            *degree.entry(b).or_default() += 1;
        }
        let mut deltas = BTreeMap::<u32, f64>::new();
        for &(a, b) in &self.bonds {
            let (_, mass_a, kind_a) = bodies[&a];
            let (_, mass_b, kind_b) = bodies[&b];
            let mut flow =
                (mass_a - mass_b) * fraction.clamp(0., 0.5) / degree[&a].max(degree[&b]) as f64;
            // Fat cells have a strong nutrient priority: they absorb quickly and
            // give stored lipids away only reluctantly.
            let priority_a = priority(kind_a);
            let priority_b = priority(kind_b);
            if flow > 0. {
                flow *= priority_b / priority_a;
            } else {
                flow *= priority_a / priority_b;
            }
            *deltas.entry(a).or_default() -= flow;
            *deltas.entry(b).or_default() += flow;
        }
        for (id, delta) in deltas {
            let (entity, mass, _) = bodies[&id];
            self.world.get::<&mut Cell>(entity).unwrap().mass = (mass + delta) as f32;
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

    /// Diffuse signalling substances and cyanide along adhesion bonds.
    /// Lipocytes insulate the connection, so a fat cell can separate circuits.
    pub(super) fn exchange_bond_signals(&mut self, fraction: f64) {
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
                    (entity, genome.mode(cell.mode).kind, cell.signal, cell.toxin),
                )
            })
            .collect();
        let mut degree = BTreeMap::<u32, usize>::new();
        for &(a, b) in &self.bonds {
            *degree.entry(a).or_default() += 1;
            *degree.entry(b).or_default() += 1;
        }
        let mut deltas = BTreeMap::<u32, [f32; 5]>::new();
        for &(a, b) in &self.bonds {
            let (_, kind_a, signal_a, toxin_a) = bodies[&a];
            let (_, kind_b, signal_b, toxin_b) = bodies[&b];
            if blocks_signals(kind_a) || blocks_signals(kind_b) {
                continue;
            }
            let share = fraction.clamp(0., 0.45) as f32 / degree[&a].max(degree[&b]) as f32;
            let mut flow = [0_f32; 5];
            for (channel, value) in flow.iter_mut().take(4).enumerate() {
                *value = (signal_a[channel] - signal_b[channel]) * share;
            }
            flow[4] = (toxin_a - toxin_b) * share * 0.5;
            {
                let delta = deltas.entry(a).or_default();
                for (slot, value) in delta.iter_mut().zip(flow) {
                    *slot -= value;
                }
            }
            {
                let delta = deltas.entry(b).or_default();
                for (slot, value) in delta.iter_mut().zip(flow) {
                    *slot += value;
                }
            }
        }
        for (id, delta) in deltas {
            let (entity, _, mut signal, mut toxin) = bodies[&id];
            for (value, change) in signal.iter_mut().zip(&delta[..4]) {
                *value = (*value + change).clamp(-100., 100.);
            }
            toxin = (toxin + delta[4]).clamp(0., 100.);
            let mut cell = self.world.get::<&mut Cell>(entity).unwrap();
            cell.signal = signal;
            cell.toxin = toxin;
        }
    }
}

fn priority(kind: u8) -> f64 {
    if kind == LIPOCYTE { 6. } else { 1. }
}
