//! Adhesion graph maintenance and positional constraints.

use super::Simulation;
use super::boundary::Boundary;
use super::interactions::{Components, for_each_island};
use super::program::{BENDING_TARGET, CONTRACTION_TARGET, LIFT_TARGET, controlled_value};
use crate::components::{Cell, Position, radius};
use crate::genome::Genome;
use hecs::Entity;
use std::collections::{BTreeMap, BTreeSet};

impl Simulation {
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
        let degrees = self.bond_degrees();
        let bodies = bonded_ids
            .iter()
            .map(|id| {
                let index = self
                    .entities
                    .binary_search_by_key(id, |&(key, _)| key)
                    .unwrap();
                let entity = self.entities[index].1;
                let (position, cell, genome) = self
                    .world
                    .query_one_mut::<(&Position, &Cell, &Genome)>(entity)
                    .unwrap();
                let mode = genome.mode(cell.mode);
                let connections = degrees[id];
                let muscle = if mode.kind == 12 {
                    [
                        controlled_value(cell, mode, CONTRACTION_TARGET, connections),
                        controlled_value(cell, mode, BENDING_TARGET, connections)
                            * if cell.mirrored { -1. } else { 1. },
                        controlled_value(cell, mode, LIFT_TARGET, connections),
                    ]
                } else {
                    [0.; 3]
                };
                let mut p = *position;
                if !cell.anchored && muscle[2] != 0. {
                    (p.x, p.y, _) =
                        self.boundary
                            .constrain(p.x, p.y - muscle[2] * (8. / 30.), cell.mass, 12.);
                }
                BondBody {
                    entity,
                    position: p,
                    mass: cell.mass,
                    anchored: cell.anchored,
                    muscle,
                }
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
        for body in islands.into_iter().flat_map(|island| island.bodies) {
            *self.world.get::<&mut Position>(body.entity).unwrap() = body.position;
        }
    }
}

struct BondIsland {
    bodies: Vec<BondBody>,
    edges: Vec<(usize, usize)>,
}

struct BondBody {
    entity: Entity,
    position: Position,
    mass: f32,
    anchored: bool,
    muscle: [f32; 3],
}

impl BondIsland {
    fn solve(&mut self, boundary: Boundary) {
        // Retain Gauss-Seidel order within an island. Disjoint islands can run
        // in any order because no constraint changes another island's positions.
        for _ in 0..12 {
            for &(a, b) in &self.edges {
                let (left, right) = self.bodies.split_at_mut(b);
                let a = &mut left[a];
                let b = &mut right[0];
                let pa = &mut a.position;
                let pb = &mut b.position;
                let dx = pb.x - pa.x;
                let dy = pb.y - pa.y;
                let distance = libm::hypotf(dx, dy);
                let (nx, ny) = if distance > 0.0001 {
                    (dx / distance, dy / distance)
                } else {
                    (1., 0.)
                };
                let contraction = (a.muscle[0] + b.muscle[0]) * 0.5;
                let rest = (radius(a.mass) + radius(b.mass) + 2.)
                    * (1. - contraction * 0.35).clamp(0.6, 1.4);
                let error = distance - rest;
                let (share_a, share_b) = match (a.anchored, b.anchored) {
                    (true, true) => continue,
                    (true, false) => (0., 1.),
                    (false, true) => (1., 0.),
                    (false, false) => {
                        let share = b.mass / (a.mass + b.mass);
                        (share, 1. - share)
                    }
                };
                // Each of twelve iterations contributes a bounded turn.
                let bend = (a.muscle[1] - b.muscle[1]) * rest * (0.012 / 12.);
                let correction_x = nx * error - ny * bend;
                let correction_y = ny * error + nx * bend;
                (pa.x, pa.y, _) = boundary.constrain(
                    pa.x + correction_x * share_a,
                    pa.y + correction_y * share_a,
                    a.mass,
                    12.,
                );
                (pb.x, pb.y, _) = boundary.constrain(
                    pb.x - correction_x * share_b,
                    pb.y - correction_y * share_b,
                    b.mass,
                    12.,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Mode, Program};
    use crate::simulation::environment::Environment;

    fn muscle_world(contraction: f32, bending: f32, lift: f32) -> Simulation {
        let mut sim = Simulation::new(42);
        sim.configure_environment(Environment {
            radiation: 0.,
            ..Environment::default()
        })
        .unwrap();
        let muscle = Mode {
            kind: 12,
            contraction,
            bending,
            lift,
            can_divide: false,
            speed: 0.,
            ..Mode::default()
        };
        let passive = Mode {
            can_divide: false,
            speed: 0.,
            ..Mode::default()
        };
        let a = sim
            .spawn_checked(500., 500., 0, &Genome::from_modes(vec![muscle]).values())
            .unwrap();
        let b = sim
            .spawn_checked(522., 500., 0, &Genome::from_modes(vec![passive]).values())
            .unwrap();
        sim.bonds.insert((a, b));
        sim
    }

    fn geometry(sim: &Simulation) -> (f32, f32, f32) {
        let a = *sim.world.get::<&Position>(sim.entities[0].1).unwrap();
        let b = *sim.world.get::<&Position>(sim.entities[1].1).unwrap();
        (
            libm::hypotf(a.x - b.x, a.y - b.y),
            b.y - a.y,
            (a.y + b.y) * 0.5,
        )
    }

    #[test]
    fn myocyte_contracts_bends_and_lifts_its_live_bonded_partner() {
        let mut passive = muscle_world(0., 0., 0.);
        let mut contracted = muscle_world(1., 0., 0.);
        let mut bent = muscle_world(0., 1., 0.);
        let mut lifted = muscle_world(0., 0., 1.);
        for world in [&mut passive, &mut contracted, &mut bent, &mut lifted] {
            world.advance(1);
        }
        assert!(geometry(&contracted).0 < geometry(&passive).0 - 2.);
        assert!(geometry(&bent).1.abs() > 0.1);
        assert!(geometry(&lifted).2 < geometry(&passive).2 - 0.1);
    }

    #[test]
    fn myocyte_signal_equation_controls_contraction_and_anchor_stays_fixed() {
        let mut low = muscle_world(0., 0., 0.);
        let mut high = muscle_world(0., 0., 0.);
        for sim in [&mut low, &mut high] {
            let entity = sim.entities[0].1;
            let mut genome = sim.world.get::<&mut Genome>(entity).unwrap();
            let mode = &mut genome.modes_mut()[0];
            mode.program_count = 1;
            mode.programs[0] = Program {
                target: CONTRACTION_TARGET as u8,
                input: 8,
                scale: 1.,
                ..Program::default()
            };
        }
        high.world
            .get::<&mut Cell>(high.entities[0].1)
            .unwrap()
            .signals[7] = 1.;
        low.advance(1);
        high.advance(1);
        assert!(geometry(&high).0 < geometry(&low).0 - 2.);

        let mut anchored = muscle_world(1., 1., 1.);
        let anchor = anchored.entities[0].1;
        anchored.world.get::<&mut Cell>(anchor).unwrap().anchored = true;
        let before = *anchored.world.get::<&Position>(anchor).unwrap();
        anchored.solve_bonds();
        let after = *anchored.world.get::<&Position>(anchor).unwrap();
        assert_eq!([before.x, before.y], [after.x, after.y]);
    }
}
