//! Adhesion graph maintenance and positional constraints.

use super::Simulation;
use super::boundary::Boundary;
use super::interactions::{Components, for_each_island};
use super::program::{BENDING_TARGET, CONTRACTION_TARGET, LIFT_TARGET, controlled_value};
use crate::cell_types::MYOCYTE;
use crate::components::{Cell, Position, radius};
use crate::genome::Genome;
use hecs::Entity;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct BondDivision {
    pub(super) parent: Position,
    pub(super) daughters: [u32; 2],
    pub(super) keep: [bool; 2],
    pub(super) bind_children: bool,
}

#[derive(Clone, Copy)]
struct BondEnd {
    id: u32,
    position: Position,
}

impl BondEnd {
    fn displacement(self, other: Self) -> (f64, f64) {
        (
            f64::from(other.position.x) - f64::from(self.position.x),
            f64::from(other.position.y) - f64::from(self.position.y),
        )
    }

    fn distance_squared(self, other: Self) -> f64 {
        let (dx, dy) = self.displacement(other);
        dx * dx + dy * dy
    }

    fn edge(self, other: Self) -> (u32, u32) {
        (self.id.min(other.id), self.id.max(other.id))
    }
}

struct DividedBondEnds<'a> {
    division: &'a BondDivision,
    daughters: [BondEnd; 2],
    neighbors: Vec<BondEnd>,
}

impl DividedBondEnds<'_> {
    fn candidates(&self, neighbor: u32) -> &[BondEnd] {
        let [a, b] = self.daughters;
        match (self.division.keep, self.neighbors.as_slice()) {
            ([true, true], [first, second]) => {
                // Replace a vertex with two connected daughters. Assign one old
                // bond to each daughter so a path or loop stays continuous.
                let direct = a.distance_squared(*first) + b.distance_squared(*second);
                let swapped = a.distance_squared(*second) + b.distance_squared(*first);
                let a_neighbor = if direct <= swapped { first } else { second };
                if a_neighbor.id == neighbor {
                    &self.daughters[..1]
                } else {
                    &self.daughters[1..]
                }
            }
            ([true, true], _) => &self.daughters,
            ([true, false], _) => &self.daughters[..1],
            ([false, true], _) => &self.daughters[1..],
            ([false, false], _) => &[],
        }
    }
}

fn is_crosswise(candidates: &[BondEnd], bond: (f64, f64)) -> bool {
    let [a, b] = candidates else { return false };
    let (dx, dy) = a.displacement(*b);
    let (bx, by) = bond;
    let dot = dx * bx + dy * by;
    let norm_squared = (dx * dx + dy * dy) * (bx * bx + by * by);
    // A bond within about 11.5° of the cleavage line can reach both daughters.
    norm_squared > 0. && dot * dot <= norm_squared * (0.2 * 0.2)
}

fn inherit_bond(
    bonds: &mut BTreeSet<(u32, u32)>,
    left: &[BondEnd],
    right: &[BondEnd],
    crosswise: bool,
) {
    if crosswise {
        match (left, right) {
            ([a, b], [c, d]) => {
                let direct = a.distance_squared(*c) + b.distance_squared(*d);
                let swapped = a.distance_squared(*d) + b.distance_squared(*c);
                let (first, second) = if direct <= swapped { (c, d) } else { (d, c) };
                bonds.extend([a.edge(*first), b.edge(*second)]);
                return;
            }
            ([a, b], [c]) | ([c], [a, b]) => {
                bonds.extend([a.edge(*c), b.edge(*c)]);
                return;
            }
            _ => {}
        }
    }

    let mut closest = None;
    for &a in left {
        for &b in right {
            let candidate = (a.distance_squared(b), a.edge(b));
            if closest.is_none_or(|best| candidate < best) {
                closest = Some(candidate);
            }
        }
    }
    if let Some((_, edge)) = closest {
        bonds.insert(edge);
    }
}

impl Simulation {
    /// Rebuild the graph after deaths and divisions, then solve constraints.
    pub(super) fn replace_bonds(
        &mut self,
        divisions: &BTreeMap<u32, BondDivision>,
        degrees: &BTreeMap<u32, usize>,
    ) {
        if self.bonds.is_empty() && divisions.is_empty() {
            return;
        }
        let entity_for = |id| {
            self.entities
                .binary_search_by_key(&id, |&(key, _)| key)
                .ok()
                .map(|index| self.entities[index].1)
        };
        let end_for = |id| {
            let entity = entity_for(id)?;
            Some(BondEnd {
                id,
                position: *self.world.get::<&Position>(entity).unwrap(),
            })
        };
        let former_end = |id| match divisions.get(&id) {
            Some(division) => division.keep.contains(&true).then_some(BondEnd {
                id,
                position: division.parent,
            }),
            None => end_for(id),
        };
        let mut divided = BTreeMap::new();
        let mut bonds = BTreeSet::new();
        for (&id, division) in divisions {
            if division.bind_children {
                let [a, b] = division.daughters;
                bonds.insert((a, b));
            }
            if degrees.contains_key(&id) {
                divided.insert(
                    id,
                    DividedBondEnds {
                        division,
                        daughters: division
                            .daughters
                            .map(|daughter| end_for(daughter).unwrap()),
                        neighbors: Vec::new(),
                    },
                );
            }
        }
        if !divided.is_empty() {
            for &(a, b) in &self.bonds {
                for (parent, neighbor) in [(a, b), (b, a)] {
                    if let Some(split) = divided.get_mut(&parent)
                        && split.division.bind_children
                        && split.division.keep == [true; 2]
                        && degrees[&parent] == 2
                        && let Some(neighbor) = former_end(neighbor)
                    {
                        split.neighbors.push(neighbor);
                    }
                }
            }
        }
        for &(a, b) in &self.bonds {
            let next_a = divided.get(&a);
            let next_b = divided.get(&b);
            if next_a.is_none() && next_b.is_none() {
                if entity_for(a).is_some() && entity_for(b).is_some() {
                    bonds.insert((a, b));
                }
                continue;
            }

            let (Some(old_a), Some(old_b)) = (former_end(a), former_end(b)) else {
                continue;
            };
            let left = next_a.map_or(std::slice::from_ref(&old_a), |split| split.candidates(b));
            let right = next_b.map_or(std::slice::from_ref(&old_b), |split| split.candidates(a));
            let bond = old_a.displacement(old_b);
            // A transverse cut of an isolated pair can retain the connection
            // on both sides. If only one cell divides, the two sides share its neighbor.
            let crosswise = degrees[&a] == 1
                && degrees[&b] == 1
                && next_a.is_none_or(|_| is_crosswise(left, bond))
                && next_b.is_none_or(|_| is_crosswise(right, bond));
            inherit_bond(&mut bonds, left, right, crosswise);
        }
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
                let muscle = if mode.kind == MYOCYTE {
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
