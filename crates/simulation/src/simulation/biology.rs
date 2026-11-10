//! Resource exchange and direct protection in multicellular organisms.

use super::Simulation;
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
            .query::<&Cell>()
            .iter()
            .map(|(entity, cell)| (cell.id, (entity, cell.mass as f64)))
            .collect();
        let mut degree = BTreeMap::<u32, usize>::new();
        for &(a, b) in &self.bonds {
            *degree.entry(a).or_default() += 1;
            *degree.entry(b).or_default() += 1;
        }
        let mut deltas = BTreeMap::<u32, f64>::new();
        for &(a, b) in &self.bonds {
            let flow = (bodies[&a].1 - bodies[&b].1) * fraction.clamp(0., 0.5)
                / degree[&a].max(degree[&b]) as f64;
            *deltas.entry(a).or_default() -= flow;
            *deltas.entry(b).or_default() += flow;
        }
        for (id, delta) in deltas {
            let (entity, mass) = bodies[&id];
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
                !removed.contains(entity) && genome.mode(cell.mode).kind == 4
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
