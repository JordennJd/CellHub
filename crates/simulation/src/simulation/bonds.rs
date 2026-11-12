//! Adhesion graph maintenance and positional constraints.

use super::Simulation;
use super::boundary::Boundary;
use super::interactions::{Components, ContactMotion, for_each_island};
use super::program::{BENDING_TARGET, CONTRACTION_TARGET, LIFT_TARGET, controlled_value};
use crate::cell_types::{BUOYOCYTE, MYOCYTE, is_motile};
use crate::components::{Cell, MAX_RECOIL_MOMENTUM, Position, STEP, radius};
use crate::genome::Genome;
use hecs::Entity;
use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::{PI, TAU};

pub(super) const ADHESION_GAP: f32 = 2.;

fn wrap_angle(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

fn local_bearings([a, b]: [Position; 2]) -> [f32; 2] {
    let bearing = libm::atan2f(b.y - a.y, b.x - a.x);
    [
        wrap_angle(bearing - a.angle),
        wrap_angle(bearing + PI - b.angle),
    ]
}

pub(super) struct BondDivision {
    pub(super) parent: Position,
    pub(super) parent_start: Position,
    pub(super) parent_radius: f32,
    pub(super) daughters: [u32; 2],
    pub(super) keep: [bool; 2],
    pub(super) bind_children: bool,
}

#[derive(Clone, Copy)]
struct BondEnd {
    id: u32,
    position: Position,
    radius: f32,
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

    fn side_of(self, to: Self, point: Self) -> f64 {
        let (dx, dy) = self.displacement(to);
        let (px, py) = self.displacement(point);
        dx * py - dy * px
    }
}

struct DividedBondEnds<'a> {
    division: &'a BondDivision,
    daughters: [BondEnd; 2],
    neighbors: Vec<(u32, [f64; 2])>,
}

impl DividedBondEnds<'_> {
    fn candidates(&self) -> [Option<BondEnd>; 2] {
        std::array::from_fn(|i| self.division.keep[i].then_some(self.daughters[i]))
    }

    fn sibling_blocks(&self, daughter: BondEnd, other: BondEnd) -> bool {
        let [a, b] = self.daughters;
        let sibling = if daughter.id == a.id { b } else { a };
        let (dx, dy) = daughter.displacement(other);
        let (sx, sy) = daughter.displacement(sibling);
        let length_squared = dx * dx + dy * dy;
        let projection = dx * sx + dy * sy;
        let cross = dx * sy - dy * sx;
        let radius = f64::from(sibling.radius);
        // The inner daughter intercepts a connection to the outer daughter
        // when the line between centers runs through her membrane.
        projection > 0.
            && projection < length_squared
            && cross * cross < radius * radius * length_squared
    }

    fn preferred_daughter(&self, neighbor: u32) -> Option<u32> {
        let [(first, first_cost), (second, second_cost)] = self.neighbors.as_slice() else {
            return None;
        };
        // If both neighbors can be reached on different sides, pass one old
        // edge to each sister. This keeps an existing path or loop continuous.
        let direct = first_cost[0] + second_cost[1];
        let swapped = first_cost[1] + second_cost[0];
        if !direct.is_finite() && !swapped.is_finite() {
            return None;
        }
        let a_neighbor = if direct <= swapped { *first } else { *second };
        Some(self.daughters[usize::from(a_neighbor != neighbor)].id)
    }
}

#[derive(Clone, Copy)]
struct BondContact {
    ends: [BondEnd; 2],
    gap: f64,
}

impl BondContact {
    fn edge(self) -> (u32, u32) {
        self.ends[0].edge(self.ends[1])
    }

    fn crosses(self, other: Self) -> bool {
        let [a, b] = self.ends;
        let [c, d] = other.ends;
        a.side_of(b, c) * a.side_of(b, d) < 0. && c.side_of(d, a) * c.side_of(d, b) < 0.
    }
}

struct BondTransfer {
    previous: [BondEnd; 2],
    contacts: [Option<BondContact>; 4],
}

impl BondTransfer {
    fn new(old: [BondEnd; 2], splits: [Option<&DividedBondEnds<'_>>; 2]) -> Self {
        let candidates: [[Option<BondEnd>; 2]; 2] = std::array::from_fn(|side| {
            splits[side].map_or([Some(old[side]), None], DividedBondEnds::candidates)
        });
        let former_span = f64::from(old[0].radius + old[1].radius + ADHESION_GAP);
        let contacts = std::array::from_fn(|i| {
            let a = candidates[0][i / 2]?;
            let b = candidates[1][i % 2]?;
            let membranes = f64::from(a.radius + b.radius);
            let new_span = membranes + f64::from(ADHESION_GAP);
            // An old joint can still reach across the daughters' smaller
            // membranes and the sideways displacement of a single split.
            // Use the nominal old span so a stretched bond cannot add reach.
            let shift_squared = old[0].distance_squared(a).max(old[1].distance_squared(b));
            let reach_squared =
                (former_span * former_span + shift_squared).max(new_span * new_span);
            let distance_squared = a.distance_squared(b);
            if distance_squared > reach_squared + 0.001
                || splits[0].is_some_and(|split| split.sibling_blocks(a, b))
                || splits[1].is_some_and(|split| split.sibling_blocks(b, a))
            {
                return None;
            }
            Some(BondContact {
                ends: [a, b],
                gap: libm::sqrt(distance_squared) - membranes,
            })
        });
        Self {
            previous: old,
            contacts,
        }
    }

    fn costs(&self, side: usize, daughters: [BondEnd; 2]) -> [f64; 2] {
        daughters.map(|daughter| {
            self.contacts
                .iter()
                .flatten()
                .filter(|contact| contact.ends[side].id == daughter.id)
                .map(|contact| contact.gap)
                .fold(f64::INFINITY, f64::min)
        })
    }

    fn retain(&self, splits: [Option<&DividedBondEnds<'_>>; 2], mut keep: impl FnMut(BondContact)) {
        let preferred: [Option<u32>; 2] = std::array::from_fn(|side| {
            splits[side].and_then(|split| split.preferred_daughter(self.previous[1 - side].id))
        });
        let preferred_contacts = self.contacts.map(|contact| {
            contact.filter(|contact| {
                preferred
                    .iter()
                    .zip(contact.ends)
                    .all(|(&id, end)| id.is_none_or(|id| id == end.id))
            })
        });
        // Preferences at opposite ends can disagree. In that case use a real
        // contact even if it means one sister inherits more than one edge.
        let contacts = if preferred_contacts.iter().any(Option::is_some) {
            preferred_contacts
        } else {
            self.contacts
        };

        if splits.iter().any(Option::is_none) {
            contacts.into_iter().flatten().for_each(keep);
            return;
        }
        let mut closest_pair = None;
        // A simultaneous division can keep two side-by-side connections,
        // with each daughter participating at most once in the old joint.
        for (i, j) in [(0, 3), (1, 2)] {
            if let (Some(a), Some(b)) = (contacts[i], contacts[j])
                && !a.crosses(b)
            {
                let score = (a.gap + b.gap, a.edge(), b.edge());
                if closest_pair.as_ref().is_none_or(|(best, _)| score < *best) {
                    closest_pair = Some((score, [a, b]));
                }
            }
        }
        if let Some((_, pair)) = closest_pair {
            pair.into_iter().for_each(keep);
        } else if let Some(contact) = contacts.into_iter().flatten().min_by(|a, b| {
            a.gap
                .total_cmp(&b.gap)
                .then_with(|| a.edge().cmp(&b.edge()))
        }) {
            keep(contact);
        }
    }
}

impl Simulation {
    pub(super) fn derive_bond_attachment(&self, (a, b): (u32, u32)) -> [f32; 2] {
        local_bearings([a, b].map(|id| {
            let index = self
                .entities
                .binary_search_by_key(&id, |&(key, _)| key)
                .unwrap();
            *self.world.get::<&Position>(self.entities[index].1).unwrap()
        }))
    }

    pub(super) fn bond_attachment(&self, edge: (u32, u32)) -> [f32; 2] {
        self.bond_anchors
            .get(&edge)
            .copied()
            .unwrap_or_else(|| self.derive_bond_attachment(edge))
    }

    pub(super) fn sync_bond_anchors(&mut self) {
        self.bond_anchors
            .retain(|edge, _| self.bonds.contains(edge));
        for &edge in &self.bonds {
            if !self.bond_anchors.contains_key(&edge) {
                let anchors = self.derive_bond_attachment(edge);
                self.bond_anchors.insert(edge, anchors);
            }
        }
    }

    /// Rebuild the graph after deaths and divisions, then solve constraints.
    pub(super) fn replace_bonds(
        &mut self,
        divisions: &BTreeMap<u32, BondDivision>,
        degrees: &BTreeMap<u32, usize>,
        motions: &[ContactMotion],
    ) {
        if self.bonds.is_empty() && divisions.is_empty() {
            self.bond_anchors.clear();
            return;
        }
        let entities = &self.entities;
        let entity_for = |id| {
            entities
                .binary_search_by_key(&id, |&(key, _)| key)
                .ok()
                .map(|index| entities[index].1)
        };
        if divisions.is_empty() {
            self.bonds
                .retain(|&(a, b)| entity_for(a).is_some() && entity_for(b).is_some());
            self.bond_anchors
                .retain(|edge, _| self.bonds.contains(edge));
            self.solve_bonds(divisions, motions);
            return;
        }
        let end_for = |id| {
            let entity = entity_for(id)?;
            let mut query = self.world.query_one::<(&Position, &Cell)>(entity).unwrap();
            let (position, cell) = query.get().unwrap();
            Some(BondEnd {
                id,
                position: *position,
                radius: radius(cell.mass),
            })
        };
        let former_end = |id| match divisions.get(&id) {
            Some(division) => division.keep.contains(&true).then_some(BondEnd {
                id,
                position: division.parent,
                radius: division.parent_radius,
            }),
            None => end_for(id),
        };
        let mut divided = BTreeMap::new();
        let mut bonds = BTreeSet::new();
        let mut anchors = BTreeMap::new();
        for (&id, division) in divisions {
            if division.bind_children {
                let [a, b] = division.daughters;
                bonds.insert((a, b));
                anchors.insert((a, b), self.derive_bond_attachment((a, b)));
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
        let mut transfers = Vec::new();
        for &(a, b) in &self.bonds {
            let splits = [divided.get(&a), divided.get(&b)];
            if splits.iter().all(Option::is_none) {
                if entity_for(a).is_some() && entity_for(b).is_some() {
                    bonds.insert((a, b));
                    anchors.insert((a, b), self.bond_attachment((a, b)));
                }
                continue;
            }

            let (Some(old_a), Some(old_b)) = (former_end(a), former_end(b)) else {
                continue;
            };
            let transfer = BondTransfer::new([old_a, old_b], splits);
            if transfer.contacts.iter().all(Option::is_none) {
                continue;
            }
            for (side, (parent, neighbor)) in [(a, b), (b, a)].into_iter().enumerate() {
                if let Some(split) = divided.get_mut(&parent)
                    && split.division.bind_children
                    && split.division.keep == [true; 2]
                    && degrees[&parent] == 2
                {
                    split
                        .neighbors
                        .push((neighbor, transfer.costs(side, split.daughters)));
                }
            }
            transfers.push(transfer);
        }
        for transfer in transfers {
            let previous = transfer.previous;
            let [a, b] = previous.map(|end| end.id);
            let former = self
                .bond_anchors
                .get(&(a, b))
                .copied()
                .unwrap_or_else(|| local_bearings(previous.map(|end| end.position)));
            transfer.retain(previous.map(|end| divided.get(&end.id)), |contact| {
                let mut local = std::array::from_fn(|side| {
                    let old = previous[side];
                    let new = contact.ends[side];
                    if old.id == new.id {
                        former[side]
                    } else {
                        // The old membrane direction is expressed in the daughter’s frame.
                        wrap_angle(former[side] + old.position.angle - new.position.angle)
                    }
                });
                if contact.ends[0].id > contact.ends[1].id {
                    local.swap(0, 1);
                }
                let edge = contact.edge();
                bonds.insert(edge);
                anchors.entry(edge).or_insert(local);
            });
        }
        self.bonds = bonds;
        self.bond_anchors = anchors;
        self.solve_bonds(divisions, motions);
    }

    /// Fit each cell's heading to its membrane attachments after contacts have
    /// moved the centers. One bond fixes the local bearing exactly. At a fork,
    /// the circular mean shares the angular correction among the attachments.
    pub(super) fn align_bond_headings(&mut self) {
        if self.bonds.is_empty() {
            return;
        }
        let mut headings = vec![[0.; 2]; self.entities.len()];
        let index = |id| {
            self.entities
                .binary_search_by_key(&id, |&(key, _)| key)
                .unwrap()
        };
        for &(a, b) in &self.bonds {
            let indices = [index(a), index(b)];
            let [pa, pb] =
                indices.map(|i| *self.world.get::<&Position>(self.entities[i].1).unwrap());
            let (dx, dy) = (pb.x - pa.x, pb.y - pa.y);
            let length = libm::hypotf(dx, dy);
            if length <= 0.0001 {
                continue;
            }
            for (side, (i, local)) in indices
                .into_iter()
                .zip(self.bond_attachment((a, b)))
                .enumerate()
            {
                let (sin, cos) = libm::sincosf(local);
                let scale = if side == 0 {
                    length.recip()
                } else {
                    -length.recip()
                };
                headings[i][0] += (dx * cos + dy * sin) * scale;
                headings[i][1] += (dy * cos - dx * sin) * scale;
            }
        }
        for (&(_, entity), [x, y]) in self.entities.iter().zip(headings) {
            if x * x + y * y > 0.000001 {
                let (position, cell) = self
                    .world
                    .query_one_mut::<(&mut Position, &Cell)>(entity)
                    .unwrap();
                if !cell.anchored {
                    position.angle += wrap_angle(libm::atan2f(y, x) - position.angle);
                }
            }
        }
    }

    fn solve_bonds(&mut self, divisions: &BTreeMap<u32, BondDivision>, motions: &[ContactMotion]) {
        if self.bonds.is_empty() {
            return;
        }
        let parents: BTreeMap<_, _> = divisions
            .iter()
            .flat_map(|(&id, division)| {
                division
                    .daughters
                    .map(|daughter| (daughter, (id, division)))
            })
            .collect();
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
                let parent = parents.get(id);
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
                            .constrain(p.x, p.y - muscle[2] * (8. * STEP), cell.mass, 12.);
                }
                BondBody {
                    entity,
                    position: p,
                    start: parent.map_or_else(
                        || {
                            motions
                                .get(index)
                                .map_or(*position, |motion| motion.path[0])
                        },
                        |(_, division)| division.parent_start,
                    ),
                    parent: parent.map(|&(id, division)| (id, division.parent)),
                    mass: cell.mass,
                    recoil: cell.recoil,
                    anchored: cell.anchored,
                    muscle,
                    driven: is_motile(mode.kind)
                        || (mode.kind == BUOYOCYTE && self.settings.gravity != 0.),
                }
            })
            .collect::<Vec<_>>();
        let index = |id| bonded_ids.binary_search(&id).unwrap();
        let edges: Vec<_> = self
            .bonds
            .iter()
            .map(|&(a, b)| {
                let [ai, bi] = [index(a), index(b)];
                let from = &bodies[ai];
                let to = &bodies[bi];
                let [p, q] = if from.sisters(to) {
                    [from.position, to.position]
                } else {
                    [from.start, to.start]
                };
                let (dx, dy) = (q.x - p.x, q.y - p.y);
                let length = libm::hypotf(dx, dy);
                let axis = if length > 0.0001 {
                    [dx / length, dy / length]
                } else {
                    attachment_axis(from.position, to.position, self.bond_attachment((a, b)))
                };
                (ai, bi, axis)
            })
            .collect();
        let mut graph = Components::new(bodies.len());
        for &(a, b, _) in &edges {
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
        for (a, b, axis) in edges {
            let a = slots[a].unwrap();
            let b = slots[b].unwrap();
            islands[a.island].edges.push(BondEdge {
                a: a.index,
                b: b.index,
                axis,
            });
        }
        let boundary = self.boundary;
        for_each_island(&mut islands, active, |island| {
            island.solve(boundary);
            island.share_recoil();
        });
        for body in islands.into_iter().flat_map(|island| island.bodies) {
            let (position, cell) = self
                .world
                .query_one_mut::<(&mut Position, &mut Cell)>(body.entity)
                .unwrap();
            *position = body.position;
            cell.recoil = body.recoil;
        }
    }
}

fn attachment_axis(a: Position, b: Position, local: [f32; 2]) -> [f32; 2] {
    let (sa, ca) = libm::sincosf(a.angle + local[0]);
    let (sb, cb) = libm::sincosf(b.angle + local[1]);
    let (x, y) = (ca - cb, sa - sb);
    let length = libm::hypotf(x, y);
    if length > 0.0001 {
        [x / length, y / length]
    } else {
        [ca, sa]
    }
}

struct BondEdge {
    a: usize,
    b: usize,
    axis: [f32; 2],
}

struct BondIsland {
    bodies: Vec<BondBody>,
    edges: Vec<BondEdge>,
}

struct BondBody {
    entity: Entity,
    position: Position,
    start: Position,
    parent: Option<(u32, Position)>,
    mass: f32,
    recoil: [f32; 2],
    anchored: bool,
    muscle: [f32; 3],
    driven: bool,
}

impl BondBody {
    fn sisters(&self, other: &Self) -> bool {
        matches!((self.parent, other.parent), (Some((a, _)), Some((b, _))) if a == b)
    }

    fn joint_direction(&self, other: &Self) -> (f64, f64) {
        // Sisters start on their new division axis. Inherited joints keep
        // their pre-division direction even when newborns initially overlap.
        let (from, to) = match (self.parent, other.parent) {
            (Some((a, _)), Some((b, _))) if a == b => (self.position, other.position),
            (a, b) => (
                a.map_or(self.position, |(_, p)| p),
                b.map_or(other.position, |(_, p)| p),
            ),
        };
        (
            f64::from(to.x) - f64::from(from.x),
            f64::from(to.y) - f64::from(from.y),
        )
    }
}

impl BondIsland {
    fn share_recoil(&mut self) {
        if !self.bodies.iter().any(|body| body.recoil != [0.; 2]) {
            return;
        }
        // Adhesion transmits translational recoil across the whole organism.
        // An anchor absorbs that momentum into the dish.
        let mut momentum = [0.; 2];
        let mut mass = 0.;
        for body in &self.bodies {
            if body.anchored {
                momentum = [0.; 2];
                break;
            }
            mass += f64::from(body.mass);
            momentum[0] += f64::from(body.recoil[0]);
            momentum[1] += f64::from(body.recoil[1]);
        }
        let velocity = if mass > 0. {
            momentum.map(|value| value / mass)
        } else {
            [0.; 2]
        };
        let limit = f64::from(MAX_RECOIL_MOMENTUM);
        for body in &mut self.bodies {
            body.recoil =
                velocity.map(|value| (value * f64::from(body.mass)).clamp(-limit, limit) as f32);
        }
    }

    fn solve(&mut self, boundary: Boundary) {
        if self.solve_passive_path(boundary) {
            return;
        }
        const BOND_PASSES: usize = 12;
        const TENSION_PASSES: usize = 6;
        // Additional sweeps let tension propagate along driven chains while
        // preserving the existing response to compression and muscle bending.
        for pass in 0..BOND_PASSES + TENSION_PASSES {
            let tension_only = pass >= BOND_PASSES;
            let mut pulled = false;
            for &BondEdge { a, b, axis } in &self.edges {
                let (left, right) = self.bodies.split_at_mut(b);
                let a = &mut left[a];
                let b = &mut right[0];
                let pa = &mut a.position;
                let pb = &mut b.position;
                let dx = pb.x - pa.x;
                let dy = pb.y - pa.y;
                let distance = libm::hypotf(dx, dy);
                let contraction = (a.muscle[0] + b.muscle[0]) * 0.5;
                let rest = (radius(a.mass) + radius(b.mass) + ADHESION_GAP)
                    * (1. - contraction * 0.35).clamp(0.6, 1.4);
                let [ax, ay] = axis;
                let along = dx * ax + dy * ay;
                let across = dx * ay - dy * ax;
                // Compression can carry one center through the other in a
                // single tick. Solve the signed gap along the membrane axis
                // when the projected membranes overlap.
                let (nx, ny, span) = if distance <= 0.0001 || (along < 0. && across.abs() < rest) {
                    (ax, ay, along)
                } else {
                    (dx / distance, dy / distance, distance)
                };
                let error = span - rest;
                if tension_only && error <= 0.001 {
                    continue;
                }
                let (share_a, share_b) = match (a.anchored, b.anchored) {
                    (true, true) => continue,
                    (true, false) => (0., 1.),
                    (false, true) => (1., 0.),
                    (false, false) => {
                        let share = b.mass / (a.mass + b.mass);
                        (share, 1. - share)
                    }
                };
                let bend = if tension_only {
                    pulled = true;
                    0.
                } else {
                    (a.muscle[1] - b.muscle[1]) * rest * (0.012 / BOND_PASSES as f32)
                };
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
            if tension_only && !pulled {
                break;
            }
        }
    }

    /// Every joint in an open path can reach its rest length along its own
    /// direction. Translate the result to preserve the center of mass.
    fn solve_passive_path(&mut self, boundary: Boundary) -> bool {
        let count = self.bodies.len();
        if count < 3 || self.edges.len() + 1 != count {
            return false;
        }
        let mut total_mass = 0.;
        let mut center = [0.; 2];
        for body in &self.bodies {
            if body.anchored || body.driven || body.muscle != [0.; 3] {
                return false;
            }
            let mass = f64::from(body.mass);
            total_mass += mass;
            center[0] += f64::from(body.position.x) * mass;
            center[1] += f64::from(body.position.y) * mass;
        }

        let mut neighbors = vec![[count; 2]; count];
        for &BondEdge { a, b, .. } in &self.edges {
            for (index, neighbor) in [(a, b), (b, a)] {
                let Some(slot) = neighbors[index].iter_mut().find(|slot| **slot == count) else {
                    return false;
                };
                *slot = neighbor;
            }
        }
        let first = neighbors.iter().position(|ends| ends[1] == count).unwrap();
        let mut offsets = vec![[0.; 2]; count];
        let mut weighted_offset = [0.; 2];
        let (mut previous, mut current) = (count, first);
        loop {
            let next = neighbors[current]
                .iter()
                .copied()
                .find(|&id| id != previous);
            let Some(next) = next.filter(|&id| id != count) else {
                break;
            };
            let from = &self.bodies[current];
            let to = &self.bodies[next];
            let (dx, dy) = from.joint_direction(to);
            let span = libm::hypot(dx, dy);
            if span == 0. {
                return false;
            }
            let scale = f64::from(radius(from.mass) + radius(to.mass) + ADHESION_GAP) / span;
            offsets[next] = [
                offsets[current][0] + dx * scale,
                offsets[current][1] + dy * scale,
            ];
            let mass = f64::from(to.mass);
            weighted_offset[0] += offsets[next][0] * mass;
            weighted_offset[1] += offsets[next][1] * mass;
            previous = current;
            current = next;
        }

        let cx = (center[0] - weighted_offset[0]) / total_mass;
        let cy = (center[1] - weighted_offset[1]) / total_mass;
        let place = |[x, y]: [f64; 2]| ((cx + x) as f32, (cy + y) as f32);
        // Let the general constraints resolve contacts with the dish rim.
        for (body, &offset) in self.bodies.iter().zip(&offsets) {
            let (x, y) = place(offset);
            if boundary.constrain(x, y, body.mass, 12.).2 {
                return false;
            }
        }
        for (body, offset) in self.bodies.iter_mut().zip(offsets) {
            (body.position.x, body.position.y) = place(offset);
        }
        true
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
    fn a_pair_can_turn_without_its_attachments_sliding() {
        let mut sim = muscle_world(0., 1., 0.);
        sim.sync_bond_anchors();
        let edge = (1, 2);
        let initial = sim.bond_attachment(edge);
        sim.advance(10);
        assert!(geometry(&sim).1.abs() > 1.);
        assert_eq!(sim.bond_attachment(edge), initial);
        let actual = sim.derive_bond_attachment(edge);
        for (actual, initial) in actual.into_iter().zip(initial) {
            assert!(wrap_angle(actual - initial).abs() < 0.0001);
        }
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
        anchored.solve_bonds(&BTreeMap::new(), &[]);
        let after = *anchored.world.get::<&Position>(anchor).unwrap();
        assert_eq!([before.x, before.y], [after.x, after.y]);
    }
}
