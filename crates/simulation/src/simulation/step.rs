//! One simulation tick: metabolism, interactions, death, division and bonds.

use super::Simulation;
use super::bonds::{ADHESION_GAP, BondDivision};
use super::interactions::ContactMotion;
use crate::components::{Cell, LIMIT, MAX_RECOIL_MOMENTUM, Position, radius};
use crate::genome::Genome;
use std::collections::{BTreeMap, HashSet};

pub(super) const STEP: f32 = 1. / 30.;
const DIVISION_SEPARATION_SPEED: f32 = 100.;

impl Simulation {
    pub(super) fn step(&mut self) {
        self.tick += 1;
        self.update_chaos();
        self.update_signals_and_secretions(STEP);
        let degrees = self.bond_degrees();
        self.exchange_bond_energy(STEP as f64 * 1.5, &degrees);
        for t in &mut self.tiles {
            let regeneration = if self.chaos.is_some() {
                crate::terrain::BIOMES[t.biome as usize][4]
            } else {
                1.
            };
            t.food = (t.food + STEP * 0.018 * t.capacity * regeneration).min(t.capacity);
            t.regenerate_nitrates(STEP);
        }
        // IDs only grow, and children are appended in ID order, so this list
        // stays sorted without re-querying and sorting the whole population.
        let mut entities = std::mem::take(&mut self.entities);
        let population = self.world.len() as usize;
        let mut dead = Vec::new();
        let mut children = Vec::new();
        let mut divisions = BTreeMap::new();
        let mut paths = Vec::with_capacity(population);
        let mut scratch = self.metabolize(&entities, &degrees);
        for (item, &(_, e)) in scratch.results.drain(..).zip(&entities) {
            if item.died {
                dead.push(e);
                continue;
            }
            if let Some(m) = item
                .division
                .filter(|_| population + children.len() / 2 < LIMIT)
            {
                let (p, c, g) = self
                    .world
                    .query_one_mut::<(&mut Position, &mut Cell, &mut Genome)>(e)
                    .unwrap();
                // parent is replaced by two daughters
                let total = c.mass;
                let mut a = c.clone();
                let mut b = c.clone();
                a.id = self.next_id;
                b.id = self.next_id + 1;
                self.next_id += 2;
                let parent_radius = radius(total);
                if let Some(events) = &mut self.division_events {
                    if events.len() == LIMIT {
                        events.pop_front();
                    }
                    events.push_back([
                        self.tick as f32,
                        c.id as f32,
                        a.id as f32,
                        b.id as f32,
                        p.x,
                        p.y,
                        parent_radius,
                        p.angle,
                    ]);
                }
                divisions.insert(
                    c.id,
                    BondDivision {
                        parent: *p,
                        parent_radius,
                        daughters: [a.id, b.id],
                        keep: [m.keep_a, m.keep_b],
                        bind_children: m.bind_children,
                    },
                );
                a.mass = total * m.ratio;
                b.mass = total * (1. - m.ratio);
                a.recoil = c.recoil.map(|momentum| momentum * m.ratio);
                b.recoil = c.recoil.map(|momentum| momentum * (1. - m.ratio));
                a.nitrogen = c.nitrogen * 0.5;
                b.nitrogen = a.nitrogen;
                a.mode = m.child_a;
                b.mode = m.child_b;
                a.age = 0.;
                b.age = 0.;
                a.anchored = false;
                b.anchored = false;
                if c.telomeres > 0 {
                    a.telomeres = c.telomeres - 1;
                    b.telomeres = a.telomeres;
                }
                a.mirrored = c.mirrored ^ m.mirror_a;
                b.mirrored = c.mirrored ^ m.mirror_b;
                a.generation += 1;
                b.generation += 1;
                // Both newborn directions are set in the parent's frame.
                // A daughter's own mirror flag applies to her next division.
                let handedness = if c.mirrored { -1. } else { 1. };
                let angle = p.angle + m.angle * handedness;
                let (sin, cos) = libm::sincosf(angle);
                let span = radius(a.mass) + radius(b.mass) + ADHESION_GAP;
                let (distance_a, distance_b) = if m.bind_children {
                    (span / 2., span / 2.)
                } else {
                    // Reduced mass times relative speed gives equal and opposite
                    // momenta, with the lighter daughter moving faster.
                    let impulse = a.mass * (1. - m.ratio) * DIVISION_SEPARATION_SPEED;
                    for (component, axis) in [cos, sin].into_iter().enumerate() {
                        let momentum = impulse * axis;
                        a.recoil[component] = (a.recoil[component] + momentum)
                            .clamp(-MAX_RECOIL_MOMENTUM, MAX_RECOIL_MOMENTUM);
                        b.recoil[component] = (b.recoil[component] - momentum)
                            .clamp(-MAX_RECOIL_MOMENTUM, MAX_RECOIL_MOMENTUM);
                    }
                    (span * (1. - m.ratio), span * m.ratio)
                };
                let (ax, ay, _) = self.boundary.constrain(
                    p.x + cos * distance_a,
                    p.y + sin * distance_a,
                    a.mass,
                    12.,
                );
                let (bx, by, _) = self.boundary.constrain(
                    p.x - cos * distance_b,
                    p.y - sin * distance_b,
                    b.mass,
                    12.,
                );
                let pa = Position {
                    x: ax,
                    y: ay,
                    angle: angle + m.child_angle_a * handedness,
                };
                let pb = Position {
                    x: bx,
                    y: by,
                    angle: angle + std::f32::consts::PI + m.child_angle_b * handedness,
                };
                children.push((pa, a, g.clone()));
                children.push((pb, b, g.clone()));
                dead.push(e);
                continue;
            }
            paths.push(item.motion);
        }
        let removed: HashSet<_> = dead.iter().copied().collect();
        self.deaths += (dead.len() - divisions.len()) as u32;
        entities.retain(|(_, entity)| !removed.contains(entity));
        for e in dead {
            self.world.despawn(e).unwrap();
        }
        for (p, c, g) in children {
            let id = c.id;
            let entity = self.world.spawn((p, c, g));
            paths.push(ContactMotion {
                path: [p; 2],
                drive: [0.; 2],
            });
            entities.push((id, entity));
            self.births += 1;
        }
        self.entities = entities;
        self.replace_bonds(&divisions, &degrees);
        self.metabolism = scratch;
        // Constraint solving can move cells across buckets or into a different
        // colony. Build the contact graph only after those positions are known.
        self.resolve_contacts(paths, STEP);
        self.specialized_contacts(STEP);
        self.record_chaos_history();
    }
}
