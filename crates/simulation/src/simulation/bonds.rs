//! Adhesion graph maintenance and positional constraints.

use super::Simulation;
use super::boundary::Boundary;
use super::interactions::{Components, for_each_island};
use crate::components::{Cell, Position, radius};
use hecs::Entity;
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
        if self.bonds.is_empty() && new_bonds.is_empty() {
            return;
        }
        let living = |id| {
            self.entities
                .binary_search_by_key(&id, |&(key, _)| key)
                .is_ok()
        };
        let mut bonds = BTreeSet::new();
        // Replace both endpoints together, including neighbors dividing on the same tick.
        for (a, b) in &self.bonds {
            let old_a = std::slice::from_ref(a);
            let old_b = std::slice::from_ref(b);
            for &next_a in inheritors.get(a).map_or(old_a, Vec::as_slice) {
                for &next_b in inheritors.get(b).map_or(old_b, Vec::as_slice) {
                    if next_a != next_b && living(next_a) && living(next_b) {
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
        let mut bonded_ids: Vec<_> = self.bonds.iter().flat_map(|&(a, b)| [a, b]).collect();
        bonded_ids.sort_unstable();
        bonded_ids.dedup();
        let bodies = bonded_ids
            .iter()
            .map(|id| {
                let index = self
                    .entities
                    .binary_search_by_key(id, |&(key, _)| key)
                    .unwrap();
                let entity = self.entities[index].1;
                let (position, cell) = self
                    .world
                    .query_one_mut::<(&Position, &Cell)>(entity)
                    .unwrap();
                (entity, *position, cell.mass)
            })
            .collect::<Vec<_>>();
        let index = |id| bonded_ids.binary_search(&id).unwrap();
        let edges: Vec<_> = self
            .bonds
            .iter()
            .map(|&(a, b)| (index(a), index(b)))
            .collect();
        let mut graph = Components::new(bodies.len());
        for &(a, b) in &edges {
            graph.join(a, b);
        }
        let (bodies, slots) = graph.islands(bodies);
        let active = bodies.iter().map(Vec::len).sum();
        let mut islands: Vec<_> = bodies
            .into_iter()
            .map(|bodies| BondIsland {
                bodies,
                edges: Vec::new(),
            })
            .collect();
        for (a, b) in edges {
            let a = slots[a].unwrap();
            let b = slots[b].unwrap();
            islands[a.island].edges.push((a.index, b.index));
        }
        let boundary = self.boundary;
        for_each_island(&mut islands, active, |island| island.solve(boundary));
        for (entity, position, _) in islands.into_iter().flat_map(|island| island.bodies) {
            *self.world.get::<&mut Position>(entity).unwrap() = position;
        }
    }
}

struct BondIsland {
    bodies: Vec<(Entity, Position, f32)>,
    edges: Vec<(usize, usize)>,
}

impl BondIsland {
    fn solve(&mut self, boundary: Boundary) {
        // Retain Gauss-Seidel order within an island. Disjoint islands can run
        // in any order because no constraint changes another island's positions.
        for _ in 0..12 {
            for &(a, b) in &self.edges {
                let (left, right) = self.bodies.split_at_mut(b);
                let (_, pa, ma) = &mut left[a];
                let (_, pb, mb) = &mut right[0];
                let dx = pb.x - pa.x;
                let dy = pb.y - pa.y;
                let distance = libm::hypotf(dx, dy);
                let (nx, ny) = if distance > 0.0001 {
                    (dx / distance, dy / distance)
                } else {
                    (1., 0.)
                };
                let error = distance - (radius(*ma) + radius(*mb) + 2.);
                let share_a = *mb / (*ma + *mb);
                let share_b = 1. - share_a;
                (pa.x, pa.y, _) = boundary.constrain(
                    pa.x + nx * error * share_a,
                    pa.y + ny * error * share_a,
                    *ma,
                    12.,
                );
                (pb.x, pb.y, _) = boundary.constrain(
                    pb.x - nx * error * share_b,
                    pb.y - ny * error * share_b,
                    *mb,
                    12.,
                );
            }
        }
    }
}
