//! Deterministic scheduling and contact resolution on disjoint interaction islands.

use super::Simulation;
use super::spatial::SpatialGrid;
use crate::components::{Cell, Position, radius};
use crate::genome::Genome;
use hecs::Entity;
#[cfg(any(not(target_arch = "wasm32"), feature = "wasm-threads"))]
use rayon::prelude::*;
use std::collections::{BTreeMap, HashSet};
#[cfg(all(target_arch = "wasm32", feature = "wasm-threads"))]
use std::sync::atomic::{AtomicBool, Ordering};

const ATTACK_RANGE: f32 = 24.;
const SEPARATION: f32 = 0.3;

#[cfg(all(target_arch = "wasm32", feature = "wasm-threads"))]
static THREAD_POOL_READY: AtomicBool = AtomicBool::new(false);

#[cfg(all(target_arch = "wasm32", feature = "wasm-threads"))]
pub(super) fn enable_parallel() {
    THREAD_POOL_READY.store(true, Ordering::Relaxed);
}

/// Both execution modes run identical jobs. Only their scheduling differs.
pub(super) fn for_each_island<T: Send>(
    islands: &mut [T],
    active_bodies: usize,
    solve: impl Fn(&mut T) + Send + Sync,
) {
    #[cfg(any(not(target_arch = "wasm32"), feature = "wasm-threads"))]
    {
        #[cfg(all(target_arch = "wasm32", feature = "wasm-threads"))]
        let ready = THREAD_POOL_READY.load(Ordering::Relaxed);
        #[cfg(not(target_arch = "wasm32"))]
        let ready = true;

        if ready && active_bodies >= 128 && islands.len() > 1 && rayon::current_num_threads() > 1 {
            islands.par_iter_mut().for_each(solve);
            return;
        }
    }
    #[cfg(all(target_arch = "wasm32", not(feature = "wasm-threads")))]
    let _ = active_bodies;
    islands.iter_mut().for_each(solve);
}

pub(super) struct Components {
    parent: Vec<usize>,
    size: Vec<usize>,
}

#[derive(Clone, Copy)]
pub(super) struct Slot {
    pub(super) island: usize,
    pub(super) index: usize,
}

impl Components {
    pub(super) fn new(count: usize) -> Self {
        Self {
            parent: (0..count).collect(),
            size: vec![1; count],
        }
    }

    fn root(&mut self, mut item: usize) -> usize {
        while self.parent[item] != item {
            self.parent[item] = self.parent[self.parent[item]];
            item = self.parent[item];
        }
        item
    }

    pub(super) fn join(&mut self, a: usize, b: usize) {
        let mut a = self.root(a);
        let mut b = self.root(b);
        if a == b {
            return;
        }
        if self.size[a] < self.size[b] {
            std::mem::swap(&mut a, &mut b);
        }
        self.parent[b] = a;
        self.size[a] += self.size[b];
    }

    /// Preserve stable input order in each island; the first member determines
    /// island order. Unconnected singletons require no work.
    pub(super) fn islands<T>(
        mut self,
        values: impl IntoIterator<Item = T>,
    ) -> (Vec<Vec<T>>, Vec<Option<Slot>>) {
        let mut islands = Vec::<Vec<T>>::new();
        let mut roots = vec![None; self.parent.len()];
        let mut slots = Vec::with_capacity(self.parent.len());
        for (index, value) in values.into_iter().enumerate() {
            let root = self.root(index);
            if self.size[root] == 1 {
                slots.push(None);
                continue;
            }
            let island = *roots[root].get_or_insert_with(|| {
                islands.push(Vec::with_capacity(self.size[root]));
                islands.len() - 1
            });
            slots.push(Some(Slot {
                island,
                index: islands[island].len(),
            }));
            islands[island].push(value);
        }
        (islands, slots)
    }
}

struct ContactBody {
    entity: Entity,
    index: usize,
    kind: u8,
    mass: f32,
    initial_mass: f32,
    radius: f32,
    cooldown: f32,
    protected: bool,
    anchored: bool,
    infected_by: Option<Entity>,
    path: [Position; 3],
    correction: [f32; 2],
    overlaps: u32,
}

fn viral_fragment(genome: &Genome, root: usize) -> Genome {
    let mut indices = vec![root];
    let mut seen = BTreeMap::from([(root, 0)]);
    let mut frontier = vec![root];
    for _ in 0..2 {
        let mut next = Vec::new();
        for parent in frontier {
            let mode = genome.mode(parent);
            for child in [mode.child_a, mode.child_b] {
                if !seen.contains_key(&child) {
                    seen.insert(child, indices.len());
                    indices.push(child);
                    next.push(child);
                }
            }
        }
        frontier = next;
    }
    let reference = |index| seen.get(&index).copied().unwrap_or(0);
    let modes = indices
        .into_iter()
        .map(|index| {
            let mut mode = *genome.mode(index);
            mode.child_a = reference(mode.child_a);
            mode.child_b = reference(mode.child_b);
            mode.transition_a = reference(mode.transition_a);
            mode.transition_b = reference(mode.transition_b);
            mode.offspring_mode = reference(mode.offspring_mode);
            mode.compatible_mode = reference(mode.compatible_mode);
            if mode.virus_copy_from != usize::MAX {
                mode.virus_copy_from = reference(mode.virus_copy_from);
            }
            mode
        })
        .collect();
    Genome::from_modes(modes)
}

fn segment_distance_squared(a: &[Position; 3], b: &[Position; 3], start: usize) -> f32 {
    let x = a[start].x - b[start].x;
    let y = a[start].y - b[start].y;
    let dx = (a[start + 1].x - b[start + 1].x) - x;
    let dy = (a[start + 1].y - b[start + 1].y) - y;
    let movement = dx * dx + dy * dy;
    if movement == 0. {
        return x * x + y * y;
    }
    let t = (-(x * dx + y * dy) / movement).clamp(0., 1.);
    let x = x + dx * t;
    let y = y + dy * t;
    x * x + y * y
}

impl ContactBody {
    fn overlap(&mut self, other: &Self) {
        if self.anchored {
            return;
        }
        let dx = self.path[2].x - other.path[2].x;
        let dy = self.path[2].y - other.path[2].y;
        let distance_squared = dx * dx + dy * dy;
        let separation = self.radius + other.radius;
        if distance_squared >= separation * separation {
            return;
        }
        let distance = libm::sqrtf(distance_squared);
        let (nx, ny) = if distance > 0.0001 {
            (dx / distance, dy / distance)
        } else if self.index < other.index {
            (-1., 0.)
        } else {
            (1., 0.)
        };
        let push = (separation - distance).min(SEPARATION);
        self.correction[0] += nx * push;
        self.correction[1] += ny * push;
        self.overlaps += 1;
    }
}

fn solve_contacts(
    bodies: &mut [ContactBody],
    slots: &[Option<Slot>],
    spatial: &SpatialGrid,
    predation: f32,
) {
    let island = slots[bodies[0].index].unwrap().island;
    for index in 0..bodies.len() {
        for other in spatial.candidates(bodies[index].index) {
            let Some(slot) = slots[other].filter(|slot| slot.island == island) else {
                continue;
            };
            let (body, target) = if index < slot.index {
                let (left, right) = bodies.split_at_mut(slot.index);
                (&mut left[index], &mut right[0])
            } else {
                let (left, right) = bodies.split_at_mut(index);
                (&mut right[0], &mut left[slot.index])
            };
            body.overlap(target);
            let infects = body.kind == 3
                && body.cooldown <= 0.
                && target.kind != 3
                && target.infected_by.is_none();
            let eats = body.kind == 5 && target.kind != 5;
            if target.protected || !(infects || eats) {
                continue;
            }
            let in_range = (0..2).any(|segment| {
                segment_distance_squared(&body.path, &target.path, segment)
                    <= ATTACK_RANGE * ATTACK_RANGE
            });
            if in_range && infects {
                target.infected_by = Some(body.entity);
                body.cooldown = 3.;
            } else if in_range && eats {
                let taken = predation.min((target.mass - 0.1).max(0.));
                target.mass -= taken;
                body.mass += taken * 0.8;
            }
        }
    }
}

impl Simulation {
    pub(super) fn resolve_contacts(&mut self, paths: Vec<[Position; 2]>, step: f32) {
        if paths.len() < 2 {
            return;
        }
        self.spatial.clear();
        let protected = self.keratin_protection(&HashSet::new());
        let bodies: Vec<_> = self
            .entities
            .iter()
            .zip(paths)
            .enumerate()
            .map(|(index, (&(id, entity), [start, driven]))| {
                let (position, cell, genome) = self
                    .world
                    .query_one_mut::<(&Position, &Cell, &Genome)>(entity)
                    .unwrap();
                let path = [start, driven, *position];
                let radius = radius(cell.mass);
                self.spatial.insert(&path, radius.max(ATTACK_RANGE / 2.));
                ContactBody {
                    entity,
                    index,
                    kind: genome.mode(cell.mode).kind,
                    mass: cell.mass,
                    initial_mass: cell.mass,
                    radius,
                    cooldown: cell.cooldown,
                    protected: protected.contains(&id),
                    anchored: cell.anchored,
                    infected_by: None,
                    path,
                    correction: [0.; 2],
                    overlaps: 0,
                }
            })
            .collect();

        let mut graph = Components::new(bodies.len());
        for &(a, b) in &self.bonds {
            let index = |id| {
                self.entities
                    .binary_search_by_key(&id, |&(key, _)| key)
                    .unwrap()
            };
            graph.join(index(a), index(b));
        }
        for body in &bodies {
            for other in self
                .spatial
                .candidates(body.index)
                .filter(|&other| other > body.index)
            {
                graph.join(body.index, other);
            }
        }
        let (mut islands, slots) = graph.islands(bodies);
        let active = islands.iter().map(Vec::len).sum();
        let spatial = &self.spatial;
        for_each_island(&mut islands, active, |bodies| {
            solve_contacts(bodies, &slots, spatial, 0.08 * step);
        });

        // No job mutates ECS storage. Commit in stable island and cell order.
        let boundary = self.boundary;
        for body in islands.into_iter().flatten() {
            let infection = body.infected_by.map(|donor| {
                let (source, genome) = self.world.query_one_mut::<(&Cell, &Genome)>(donor).unwrap();
                let root = genome.mode(source.mode).virus_copy_from;
                if root == usize::MAX {
                    (genome.clone(), false)
                } else {
                    (viral_fragment(genome, root), true)
                }
            });
            let (position, cell, genome) = self
                .world
                .query_one_mut::<(&mut Position, &mut Cell, &mut Genome)>(body.entity)
                .unwrap();
            cell.mass = body.mass;
            cell.cooldown = body.cooldown;
            if body.overlaps > 0 || body.mass != body.initial_mass {
                let count = body.overlaps.max(1) as f32;
                (position.x, position.y, _) = boundary.constrain(
                    position.x + body.correction[0] / count,
                    position.y + body.correction[1] / count,
                    cell.mass,
                    12.,
                );
            }
            if let Some((infection, fragment)) = infection {
                *genome = infection;
                cell.mode = if fragment {
                    0
                } else {
                    cell.mode.min(genome.len() - 1)
                };
                cell.mutations += 1;
                cell.cooldown = 3.;
                self.infections += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::environment::Environment;

    fn genes(kind: u8) -> [f32; 12] {
        [kind as f32, 8., 0., 0., 0.5, 0., 0., 1000., 0., 0., 0., 0.]
    }

    #[test]
    fn viral_fragment_keeps_two_generations_and_remaps_mode_references() {
        let mut modes: Vec<_> = (0..8)
            .map(|kind| crate::genome::Mode {
                kind,
                ..crate::genome::Mode::default()
            })
            .collect();
        modes[2].child_a = 3;
        modes[2].child_b = 4;
        modes[3].child_a = 5;
        modes[3].child_b = 6;
        modes[4].child_a = 6;
        modes[4].child_b = 7;
        let copied = viral_fragment(&Genome::from_modes(modes), 2);
        assert_eq!(copied.len(), 6);
        assert_eq!(copied.mode(0).kind, 2);
        assert_eq!(copied.mode(0).child_a, 1);
        assert_eq!(copied.mode(0).child_b, 2);
        assert_eq!(copied.mode(1).child_a, 3);
        assert_eq!(copied.mode(2).child_b, 5);
        assert_eq!(copied.mode(3).child_a, 0);
        assert!(
            copied
                .cloned_modes()
                .iter()
                .all(|mode| mode.child_a < copied.len()
                    && mode.child_b < copied.len()
                    && mode.transition_a < copied.len())
        );
    }

    #[test]
    fn detects_swept_contact_from_bond_constraints() {
        let mut s = Simulation::new(42);
        s.configure_environment(Environment {
            radiation: 0.,
            ..Environment::default()
        })
        .unwrap();
        let virus = s.spawn_checked(100., 500., 0, &genes(3)).unwrap();
        let partner = s.spawn_checked(500., 500., 0, &genes(0)).unwrap();
        let bystander = s.spawn_checked(200., 500., 0, &genes(0)).unwrap();
        s.bonds.insert((virus, partner));

        s.advance(1);

        assert_eq!(s.infections, 1);
        assert_eq!(s.genome(bystander), s.genome(virus));
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn checkpoint_and_replay_match_between_one_and_four_threads() {
        let run = |threads, checkpoint: Option<&[f64]>| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| {
                    let mut s = Simulation::new(73421);
                    if let Some(state) = checkpoint {
                        s.restore_checked(state).unwrap();
                    } else {
                        s.set_circular_dish(true);
                        for i in 0..72 {
                            let x = 880. + 120. * (i % 12) as f32;
                            let y = 1100. + 130. * (i / 12) as f32;
                            let photo = s.spawn_checked(x, y, 0, &genes(0)).unwrap();
                            let mut swimming = genes(2);
                            swimming[5] = 12.;
                            let flag = s.spawn_checked(x + 22., y, 0, &swimming).unwrap();
                            s.spawn_checked(x - 22., y, 0, &genes(5)).unwrap();
                            s.bonds.insert((photo, flag));
                        }
                    }
                    s.advance(75);
                    s.encode_state()
                })
        };
        let initial = run(1, None);
        assert_eq!(initial, run(4, None));
        assert_eq!(run(1, Some(&initial)), run(4, Some(&initial)));
    }
}
