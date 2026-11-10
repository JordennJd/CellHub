//! Adhesion graph maintenance and positional constraints.

use super::Simulation;
use crate::components::{Cell, Position, SIZE, radius};
use std::collections::{BTreeMap, BTreeSet, HashSet};

impl Simulation {
    /// IDs that participate in at least one bond.
    pub(super) fn linked_ids(&self) -> HashSet<u32> {
        self.bonds.iter().flat_map(|&(a, b)| [a, b]).collect()
    }

    /// Rebuild the graph after deaths and divisions, then solve constraints.
    pub(super) fn replace_bonds(
        &mut self,
        inheritors: &BTreeMap<u32, Vec<u32>>,
        new_bonds: Vec<(u32, u32)>,
    ) {
        let living: HashSet<_> = self
            .world
            .query::<&Cell>()
            .iter()
            .map(|(_, c)| c.id)
            .collect();
        let mut bonds = BTreeSet::new();
        // Replace both endpoints together, including neighbors dividing on the same tick.
        for &(a, b) in &self.bonds {
            let old_a = vec![a];
            let old_b = vec![b];
            for &next_a in inheritors.get(&a).unwrap_or(&old_a) {
                for &next_b in inheritors.get(&b).unwrap_or(&old_b) {
                    if next_a != next_b && living.contains(&next_a) && living.contains(&next_b) {
                        bonds.insert((next_a.min(next_b), next_a.max(next_b)));
                    }
                }
            }
        }
        bonds.extend(new_bonds);
        self.bonds = bonds;
        self.solve_bonds();
    }

    fn solve_bonds(&mut self) {
        if self.bonds.is_empty() {
            return;
        }
        let mut bodies: Vec<_> = self
            .world
            .query::<(&Position, &Cell)>()
            .iter()
            .map(|(e, (p, c))| (e, c.id, *p, c.mass))
            .collect();
        bodies.sort_by_key(|b| b.1);
        let indices: BTreeMap<_, _> = bodies.iter().enumerate().map(|(i, b)| (b.1, i)).collect();
        let edges: Vec<_> = self
            .bonds
            .iter()
            .map(|(a, b)| (indices[a], indices[b]))
            .collect();
        // Position constraints distribute propulsion according to mass. Re-evaluate
        // rest length as cells grow; the two cells remain distinct, touching bodies.
        for _ in 0..12 {
            for &(a, b) in &edges {
                let (left, right) = bodies.split_at_mut(b);
                let (_, _, pa, ma) = &mut left[a];
                let (_, _, pb, mb) = &mut right[0];
                let dx = pb.x - pa.x;
                let dy = pb.y - pa.y;
                let distance = dx.hypot(dy);
                let (nx, ny) = if distance > 0.0001 {
                    (dx / distance, dy / distance)
                } else {
                    (1., 0.)
                };
                let error = distance - (radius(*ma) + radius(*mb) + 2.);
                let share_a = *mb / (*ma + *mb);
                let share_b = 1. - share_a;
                pa.x = (pa.x + nx * error * share_a).clamp(12., SIZE - 12.);
                pa.y = (pa.y + ny * error * share_a).clamp(12., SIZE - 12.);
                pb.x = (pb.x - nx * error * share_b).clamp(12., SIZE - 12.);
                pb.y = (pb.y - ny * error * share_b).clamp(12., SIZE - 12.);
            }
        }
        for (entity, _, position, _) in bodies {
            *self.world.get::<&mut Position>(entity).unwrap() = position;
        }
    }
}
