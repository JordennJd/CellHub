//! One simulation tick: metabolism, interactions, death, division and bonds.

use super::Simulation;
use super::bonds::{ADHESION_GAP, BondDivision};
use super::program::{SPEED_TARGET, controlled_value};
use super::specialized::{buoyant_drift, corpse_food, differentiate};
use crate::cell_types::{
    BUOYOCYTE, COUNT, FLAGELLOCYTE, GAMETE, KERATINOCYTE, LIPOCYTE, PHAGOCYTE, PHOTOCYTE,
};
use crate::components::{
    Cell, LIMIT, MAX_RECOIL_MOMENTUM, MIN_DIVISION_AGE, NITROGEN_DEATH, NITROGEN_DIVISION,
    Position, radius,
};
use crate::genome::Genome;
use crate::terrain::tile_index;
use std::collections::{BTreeMap, HashSet};

const STEP: f32 = 1. / 30.;
const MIN_CELL_MASS: f32 = 0.18;
const DIVISION_SEPARATION_SPEED: f32 = 100.;
const RECOIL_DRAG: f32 = 4.;

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
        let settings = self.settings;
        let passive_drift = settings.vertical_drift();
        let recoil_decay = libm::expf(-RECOIL_DRAG * settings.viscosity * STEP);
        let mut paths = Vec::with_capacity(population);
        for &(_, e) in &entities {
            let noise = self.random();
            let mutation_roll = self.random();
            let mutation_gene = self.random();
            let (p, c, g) = self
                .world
                .query_one_mut::<(&mut Position, &mut Cell, &mut Genome)>(e)
                .unwrap();
            let start = *p;
            c.age += STEP;
            c.cooldown = (c.cooldown - STEP).max(0.);
            let degree = degrees.get(&c.id).copied().unwrap_or(0);
            let differentiated = differentiate(c, g, degree);
            let m = *g.mode(c.mode);
            let programmed_speed = controlled_value(c, &m, SPEED_TARGET, degree).clamp(0., 60.);
            let t = &mut self.tiles[tile_index(p.x, p.y)];
            t.feed_nitrogen(c, m.kind, STEP);
            let gain = if m.kind == PHOTOCYTE {
                t.light * 0.14 * STEP
            } else if m.kind == PHAGOCYTE {
                let eaten = (0.18 * STEP).min(t.food);
                t.food -= eaten;
                eaten * 1.6
            } else {
                0.
            };
            let maintenance = match m.kind {
                LIPOCYTE => 0.004 + programmed_speed * 0.00005,
                KERATINOCYTE => 0.014 + programmed_speed * 0.0003,
                _ => 0.025 + programmed_speed * 0.0003,
            };
            c.mass += gain - STEP * maintenance;
            if mutation_roll < t.radiation * STEP * 0.035 {
                let modes = g.modes_mut();
                let index = ((mutation_gene * modes.len() as f32) as usize).min(modes.len() - 1);
                let target = &mut modes[index];
                if !self.settings.only_point_mutations {
                    target.kind = (target.kind + 1) % COUNT;
                }
                target.threshold = (target.threshold + (noise - 0.5) * 0.8).clamp(1., 8.);
                c.mutations += 1;
            }
            c.mass -= t.radiation * STEP * 0.015;
            // A bonded flagellum supplies directed propulsion to the organism.
            // Passive members have drag, but no independent random locomotion.
            let linked = degree != 0;
            if !linked {
                p.angle += (noise - 0.5) * 0.35;
            }
            let speed = if m.kind == FLAGELLOCYTE {
                programmed_speed
            } else if linked {
                0.
            } else {
                programmed_speed * 0.15
            };
            let vertical_drift = if m.kind == BUOYOCYTE {
                buoyant_drift(
                    c,
                    &m,
                    degree,
                    settings.gravity,
                    settings.density,
                    settings.viscosity,
                )
            } else {
                passive_drift
            };
            let mut next_x = p.x + libm::cosf(p.angle) * speed * STEP / settings.viscosity;
            let mut next_y = p.y
                + libm::sinf(p.angle) * speed * STEP / settings.viscosity
                + vertical_drift * STEP;
            if c.recoil != [0.; 2] {
                let scale = STEP / c.mass.max(MIN_CELL_MASS);
                next_x += c.recoil[0] * scale;
                next_y += c.recoil[1] * scale;
            }
            let (x, y, reached_edge) = if c.anchored {
                (p.x, p.y, false)
            } else {
                self.boundary.constrain(next_x, next_y, c.mass, 12.)
            };
            p.x = x;
            p.y = y;
            if c.anchored || reached_edge {
                c.recoil = [0.; 2];
            } else {
                c.recoil = c.recoil.map(|momentum| momentum * recoil_decay);
            }
            if c.mass < MIN_CELL_MASS
                || c.nitrogen < NITROGEN_DEATH
                || c.toxin >= 1.
                || c.injury >= 1.
                || c.telomeres == 0
                || (self.settings.aging && c.age > m.lifespan)
                || (self.settings.kill_at_edge && reached_edge)
            {
                corpse_food(t, c, m.kind);
                dead.push(e);
                continue;
            }
            if m.can_divide
                && m.kind != GAMETE
                && !differentiated
                && c.age + 0.000001 >= MIN_DIVISION_AGE
                && c.mass >= m.threshold
                && c.nitrogen >= NITROGEN_DIVISION
                && m.max_connections.is_none_or(|limit| degree < limit)
                && population + children.len() / 2 < LIMIT
            {
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
            paths.push([start, *p]);
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
        // Constraint solving can move cells across buckets or into a different
        // colony. Build the contact graph only after those positions are known.
        self.resolve_contacts(paths, STEP);
        self.specialized_contacts(STEP);
    }
}
