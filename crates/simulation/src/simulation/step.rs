//! One simulation tick: metabolism, interactions, death, division and bonds.

use super::Simulation;
use super::spatial::BUCKET_SIZE;
use crate::components::{Cell, LIMIT, MIN_DIVISION_AGE, Position, radius};
use crate::genome::Genome;
use crate::terrain::tile_index;
use std::collections::{BTreeMap, HashSet};

const STEP: f32 = 1. / 30.;

impl Simulation {
    pub(super) fn step(&mut self) {
        self.tick += 1;
        self.exchange_bond_energy(STEP as f64 * 1.5);
        for t in &mut self.tiles {
            t.food = (t.food + STEP * 0.018 * t.capacity).min(t.capacity);
        }
        // IDs only grow, and children are appended in ID order, so this list
        // stays sorted without re-querying and sorting the whole population.
        let mut entities = std::mem::take(&mut self.entities);
        let population = self.world.len() as usize;
        let mut dead = Vec::new();
        let mut children = Vec::new();
        let mut inheritors: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        let mut new_bonds = Vec::new();
        let bonded = self.linked_ids();
        let vertical_drift = self.settings.vertical_drift();
        let mut spatial = std::mem::take(&mut self.spatial);
        spatial.clear();
        for &(_, e) in &entities {
            let noise = self.random();
            let mutation_roll = self.random();
            let mutation_gene = self.random();
            let (p, c, g) = self
                .world
                .query_one_mut::<(&mut Position, &mut Cell, &mut Genome)>(e)
                .unwrap();
            let m = *g.mode(c.mode);
            c.age += STEP;
            c.cooldown = (c.cooldown - STEP).max(0.);
            let t = &mut self.tiles[tile_index(p.x, p.y)];
            let gain = if m.kind == 0 {
                t.light * 0.14 * STEP
            } else if m.kind == 1 {
                let eaten = (0.18 * STEP).min(t.food);
                t.food -= eaten;
                eaten * 1.6
            } else {
                0.
            };
            c.mass += gain - STEP * (if m.kind == 4 { 0.014 } else { 0.025 } + m.speed * 0.0003);
            if mutation_roll < t.radiation * STEP * 0.035 {
                let modes = g.modes_mut();
                let index = ((mutation_gene * modes.len() as f32) as usize).min(modes.len() - 1);
                let target = &mut modes[index];
                if !self.settings.only_point_mutations {
                    target.kind = (target.kind + 1) % 6;
                }
                target.threshold = (target.threshold + (noise - 0.5) * 0.8).clamp(1., 8.);
                c.mutations += 1;
            }
            c.mass -= t.radiation * STEP * 0.015;
            // A bonded flagellum supplies directed propulsion to the organism.
            // Passive members have drag, but no independent random locomotion.
            let linked = bonded.contains(&c.id);
            if !linked {
                p.angle += (noise - 0.5) * 0.35;
            }
            let speed = if m.kind == 2 {
                m.speed
            } else if linked {
                0.
            } else {
                m.speed * 0.15
            };
            let next_x = p.x + p.angle.cos() * speed * STEP / self.settings.viscosity;
            let next_y = p.y
                + p.angle.sin() * speed * STEP / self.settings.viscosity
                + vertical_drift * STEP;
            let (x, y, reached_edge) = self.boundary.constrain(next_x, next_y, c.mass, 12.);
            p.x = x;
            p.y = y;
            if c.mass < 0.18
                || (self.settings.aging && c.age > m.lifespan)
                || (self.settings.kill_at_edge && reached_edge)
            {
                dead.push(e);
                continue;
            }
            if m.can_divide
                && c.age + 0.000001 >= MIN_DIVISION_AGE
                && c.mass >= m.threshold
                && population + children.len() / 2 < LIMIT
            {
                // parent is replaced by two daughters
                let total = c.mass;
                let mut a = c.clone();
                let mut b = c.clone();
                a.id = self.next_id;
                b.id = self.next_id + 1;
                self.next_id += 2;
                let successors = inheritors.entry(c.id).or_default();
                if m.keep_a {
                    successors.push(a.id);
                }
                if m.keep_b {
                    successors.push(b.id);
                }
                if m.bind_children {
                    new_bonds.push((a.id, b.id));
                }
                a.mass = total * m.ratio;
                b.mass = total * (1. - m.ratio);
                a.mode = m.child_a;
                b.mode = m.child_b;
                a.age = 0.;
                b.age = 0.;
                a.mirrored = c.mirrored ^ m.mirror_a;
                b.mirrored = c.mirrored ^ m.mirror_b;
                a.generation += 1;
                b.generation += 1;
                let angle = p.angle + m.angle * if c.mirrored { -1. } else { 1. };
                let distance = (radius(a.mass) + radius(b.mass) + 2.) / 2.;
                let (ax, ay, _) = self.boundary.constrain(
                    p.x + angle.cos() * distance,
                    p.y + angle.sin() * distance,
                    a.mass,
                    12.,
                );
                let (bx, by, _) = self.boundary.constrain(
                    p.x - angle.cos() * distance,
                    p.y - angle.sin() * distance,
                    b.mass,
                    12.,
                );
                let pa = Position {
                    x: ax,
                    y: ay,
                    angle: angle + m.child_angle_a * if a.mirrored { -1. } else { 1. },
                };
                let pb = Position {
                    x: bx,
                    y: by,
                    angle: angle
                        + std::f32::consts::PI
                        + m.child_angle_b * if b.mirrored { -1. } else { 1. },
                };
                children.push((pa, a, g.clone()));
                children.push((pb, b, g.clone()));
                dead.push(e);
                continue;
            }
            spatial.insert(*p, e, m.kind);
        }
        let removed: HashSet<_> = dead.iter().copied().collect();
        let protected = self.keratin_protection(&removed);
        let mut infections = Vec::new();
        let mut infected = HashSet::new();
        // Stable traversal and local spatial grid bound the interaction search.
        for &(_, e) in &entities {
            if removed.contains(&e) {
                continue;
            }
            let (pos, kind, cooldown) = {
                let (p, c, g) = self
                    .world
                    .query_one_mut::<(&Position, &Cell, &Genome)>(e)
                    .unwrap();
                (*p, g.mode(c.mode).kind, c.cooldown)
            };
            let bx = (pos.x / BUCKET_SIZE) as isize;
            let by = (pos.y / BUCKET_SIZE) as isize;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(neighbors) = spatial.bucket(bx + dx, by + dy) {
                        for neighbor in neighbors {
                            let other = neighbor.entity;
                            let op = neighbor.position;
                            let okind = neighbor.kind;
                            if e == other || removed.contains(&other) {
                                continue;
                            }
                            let d2 = (pos.x - op.x).powi(2) + (pos.y - op.y).powi(2);
                            if d2 > 24. * 24. {
                                continue;
                            }
                            let other_id = self.world.get::<&Cell>(other).unwrap().id;
                            if kind == 3
                                && okind != 3
                                && !protected.contains(&other_id)
                                && !infected.contains(&other)
                                && cooldown <= 0.
                                && self.world.get::<&Cell>(e).unwrap().cooldown <= 0.
                            {
                                let donor = (*self.world.get::<&Genome>(e).unwrap()).clone();
                                infections.push((other, donor));
                                infected.insert(other);
                                self.world.get::<&mut Cell>(e).unwrap().cooldown = 3.;
                                self.infections += 1;
                                break;
                            }
                            if kind == 5 && okind != 5 && !protected.contains(&other_id) {
                                let taken = {
                                    let mut c = self.world.get::<&mut Cell>(other).unwrap();
                                    let amount = (0.08 * STEP).min((c.mass - 0.1).max(0.));
                                    c.mass -= amount;
                                    amount
                                };
                                self.world.get::<&mut Cell>(e).unwrap().mass += taken * 0.8;
                            }
                            if d2 > 0.001 && d2 < 18. * 18. {
                                let d = d2.sqrt();
                                let (p, c) = self
                                    .world
                                    .query_one_mut::<(&mut Position, &Cell)>(e)
                                    .unwrap();
                                (p.x, p.y, _) = self.boundary.constrain(
                                    p.x + (pos.x - op.x) / d * 0.3,
                                    p.y + (pos.y - op.y) / d * 0.3,
                                    c.mass,
                                    12.,
                                );
                            }
                        }
                    }
                }
            }
        }
        // Commit infections together: a newly infected cell cannot infect again in this tick.
        for (entity, genome) in infections {
            let (cell, target) = self
                .world
                .query_one_mut::<(&mut Cell, &mut Genome)>(entity)
                .unwrap();
            *target = genome;
            // Infection can replace a long genome with a shorter one.
            cell.mode = cell.mode.min(target.len() - 1);
            cell.mutations += 1;
            cell.cooldown = 3.;
        }
        let divisions = children.len() / 2;
        self.deaths += (dead.len() - divisions) as u32;
        entities.retain(|(_, entity)| !removed.contains(entity));
        for e in dead {
            self.world.despawn(e).unwrap();
        }
        for (p, c, g) in children {
            let id = c.id;
            let entity = self.world.spawn((p, c, g));
            entities.push((id, entity));
            self.births += 1;
        }
        self.replace_bonds(&inheritors, new_bonds);
        self.entities = entities;
        self.spatial = spatial;
    }
}
