//! One simulation tick: metabolism, interactions, death, division and bonds.

use super::Simulation;
use super::bonds::{ADHESION_GAP, BondDivision};
use crate::components::{Cell, LIMIT, Position, radius};
use crate::genome::Genome;
use std::collections::{BTreeMap, HashSet};

pub(super) const STEP: f32 = 1. / 30.;

impl Simulation {
    pub(super) fn step(&mut self) {
        self.tick += 1;
        self.update_signals_and_secretions(STEP);
        let degrees = self.bond_degrees();
        self.exchange_bond_energy(STEP as f64 * 1.5, &degrees);
        for t in &mut self.tiles {
            t.food = (t.food + STEP * 0.018 * t.capacity).min(t.capacity);
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
                divisions.insert(
                    c.id,
                    BondDivision {
                        parent: *p,
                        parent_radius: radius(c.mass),
                        daughters: [a.id, b.id],
                        keep: [m.keep_a, m.keep_b],
                        bind_children: m.bind_children,
                    },
                );
                a.mass = total * m.ratio;
                b.mass = total * (1. - m.ratio);
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
                let angle = p.angle + m.angle * if c.mirrored { -1. } else { 1. };
                let distance = (radius(a.mass) + radius(b.mass) + ADHESION_GAP) / 2.;
                let (sin, cos) = libm::sincosf(angle);
                let (ax, ay, _) = self.boundary.constrain(
                    p.x + cos * distance,
                    p.y + sin * distance,
                    a.mass,
                    12.,
                );
                let (bx, by, _) = self.boundary.constrain(
                    p.x - cos * distance,
                    p.y - sin * distance,
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
            paths.push([item.start, item.end]);
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
            paths.push([p, p]);
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
    }
}
