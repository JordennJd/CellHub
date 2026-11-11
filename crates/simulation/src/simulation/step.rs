//! One simulation tick: metabolism, interactions, death, division and bonds.

use super::Simulation;
use super::spatial::BUCKET_SIZE;
use crate::cell_types::{
    BUOYOCYTE, CILIOCYTE, COUNT, DEVOROCYTE, FLAGELLOCYTE, GAMETE, GLUEOCYTE, KERATINOCYTE,
    LIPOCYTE, NITROCYTE, PHAGOCYTE, PHOTOCYTE, SECROCYTE, VIROCYTE, is_motile,
};
use crate::components::{Cell, LIMIT, MIN_DIVISION_AGE, Position, radius};
use crate::genome::Genome;
use crate::terrain::tile_index;
use std::collections::{BTreeMap, HashMap, HashSet};

const STEP: f32 = 1. / 30.;
const SIGNAL_DECAY: f32 = 0.997;
const TOXIN_DECAY: f32 = 0.999;

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
        let gravity = self.settings.gravity;
        let viscosity = self.settings.viscosity;
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
            c.signal = c.signal.map(|v| (v * SIGNAL_DECAY).clamp(-100., 100.));
            c.toxin = (c.toxin * TOXIN_DECAY).clamp(0., 100.);
            c.mass -= c.toxin * STEP * 0.02;
            let t = &mut self.tiles[tile_index(p.x, p.y)];
            let gain = match m.kind {
                PHOTOCYTE => t.light * 0.14 * STEP,
                PHAGOCYTE => {
                    let eaten = (0.18 * STEP).min(t.food);
                    t.food -= eaten;
                    eaten * 1.6
                }
                // Nitrogen fixation is independent of light and food. The
                // resulting mass is shared through adhesion like any nutrient.
                NITROCYTE => m.signal.max(0.) / 60. * 0.06 * STEP,
                _ => 0.,
            };
            let upkeep = match m.kind {
                LIPOCYTE => 0.004,
                KERATINOCYTE => 0.014,
                _ => 0.025,
            };
            let effort = if is_motile(m.kind) {
                m.speed * 0.0003
            } else {
                0.
            };
            c.mass += gain - STEP * (upkeep + effort);
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
            // A bonded flagellum or ciliocyte supplies directed propulsion to
            // the organism. Passive members have drag, but no independent
            // random locomotion.
            let linked = bonded.contains(&c.id);
            if !linked {
                p.angle += (noise - 0.5) * 0.35;
            }
            let (speed, steering) = match m.kind {
                FLAGELLOCYTE => (m.speed * (1. + (c.signal[0] / 60.).clamp(-0.8, 1.)), 0.),
                CILIOCYTE => (
                    m.speed * 0.5,
                    ((c.signal[1] - c.signal[0]) * 0.04).clamp(-0.15, 0.15),
                ),
                _ => (0., 0.),
            };
            if is_motile(m.kind) {
                p.angle += steering * if c.mirrored { -1. } else { 1. };
            }
            let buoyancy = if m.kind == BUOYOCYTE {
                -20. * gravity * m.signal.clamp(-3.3, 3.3) / viscosity
            } else {
                0.
            };
            let next_x = p.x + p.angle.cos() * speed * STEP / viscosity;
            let next_y =
                p.y + p.angle.sin() * speed * STEP / viscosity + (vertical_drift + buoyancy) * STEP;
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
                && m.kind != GAMETE
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
        let mut removed: HashSet<_> = dead.iter().copied().collect();
        self.update_signals(&entities, &removed, &spatial);
        let protected = self.keratin_protection(&removed);
        let mut infections = Vec::new();
        let mut infected = HashSet::new();
        let mut fused = HashSet::new();
        let mut fusions: Vec<(hecs::Entity, hecs::Entity, f32, usize)> = Vec::new();
        let mut cut_bonds = Vec::new();
        let mut glue_counts: HashMap<u32, usize> = HashMap::new();
        for &(a, b) in &self.bonds {
            *glue_counts.entry(a).or_default() += 1;
            *glue_counts.entry(b).or_default() += 1;
        }
        // Stable traversal and local spatial grid bound the interaction search.
        for &(_, e) in &entities {
            if removed.contains(&e) || fused.contains(&e) {
                continue;
            }
            let (pos, kind, cooldown, mode, age, mself, mass) = {
                let (p, c, g) = self
                    .world
                    .query_one_mut::<(&Position, &Cell, &Genome)>(e)
                    .unwrap();
                (
                    *p,
                    g.mode(c.mode).kind,
                    c.cooldown,
                    *g.mode(c.mode),
                    c.age,
                    c.clone(),
                    c.mass,
                )
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
                            if e == other || removed.contains(&other) || fused.contains(&other) {
                                continue;
                            }
                            let d2 = (pos.x - op.x).powi(2) + (pos.y - op.y).powi(2);
                            if d2 > 24. * 24. {
                                continue;
                            }
                            let other_id = self.world.get::<&Cell>(other).unwrap().id;
                            if kind == VIROCYTE
                                && okind != VIROCYTE
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
                            if kind == DEVOROCYTE
                                && okind != DEVOROCYTE
                                && !protected.contains(&other_id)
                            {
                                let rate = 0.09 + mode.speed / 60. * 0.12;
                                let taken = {
                                    let mut c = self.world.get::<&mut Cell>(other).unwrap();
                                    let amount = (rate * STEP).min((c.mass - 0.1).max(0.));
                                    c.mass -= amount;
                                    amount
                                };
                                self.world.get::<&mut Cell>(e).unwrap().mass += taken * 0.8;
                            }
                            if kind == GLUEOCYTE
                                && okind != GLUEOCYTE
                                && !protected.contains(&other_id)
                            {
                                let other_mass = self.world.get::<&Cell>(other).unwrap().mass;
                                let contact = radius(mass) + radius(other_mass) + 4.;
                                let limit = (mode.signal.round() as i32).clamp(1, 5) as usize;
                                let key = (other_id.min(mself.id), other_id.max(mself.id));
                                if d2 <= contact * contact
                                    && !self.bonds.contains(&key)
                                    && glue_counts.get(&mself.id).copied().unwrap_or(0) < limit
                                {
                                    self.bonds.insert(key);
                                    *glue_counts.entry(mself.id).or_default() += 1;
                                    *glue_counts.entry(other_id).or_default() += 1;
                                }
                            }
                            if kind == SECROCYTE && age >= 0.05 {
                                match mode.subtype {
                                    0 => {
                                        if !protected.contains(&other_id) {
                                            let amount = mode.signal.max(1.) / 60. * STEP * 2.;
                                            let mut target =
                                                self.world.get::<&mut Cell>(other).unwrap();
                                            target.toxin = (target.toxin + amount).clamp(0., 100.);
                                        }
                                    }
                                    1 => {
                                        if !protected.contains(&other_id) {
                                            let amount = mode.signal.max(1.) / 60. * STEP * 0.5;
                                            let mut target =
                                                self.world.get::<&mut Cell>(other).unwrap();
                                            target.toxin = (target.toxin + amount).clamp(0., 100.);
                                            target.mass -= STEP * 0.006;
                                        }
                                    }
                                    2 if !protected.contains(&other_id) => {
                                        cut_bonds
                                            .push((other_id.min(mself.id), other_id.max(mself.id)));
                                    }
                                    _ => {}
                                }
                            }
                            if kind == GAMETE && okind == GAMETE {
                                let (other_mass, other_mode) = {
                                    let (other_cell, other_genome) = self
                                        .world
                                        .query_one_mut::<(&Cell, &Genome)>(other)
                                        .unwrap();
                                    (other_cell.mass, *other_genome.mode(other_cell.mode))
                                };
                                if mode.subtype == other_mode.subtype {
                                    let (survivor, victim, total, next_mode) = if mass >= other_mass
                                    {
                                        (e, other, mass + other_mass, mode.child_a)
                                    } else {
                                        (other, e, mass + other_mass, other_mode.child_a)
                                    };
                                    fused.insert(survivor);
                                    fused.insert(victim);
                                    fusions.push((survivor, victim, total, next_mode));
                                    break;
                                }
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
        for (survivor, victim, total, next_mode) in fusions {
            if removed.contains(&survivor) || removed.contains(&victim) {
                continue;
            }
            let ok = {
                let (cell, genome) = self
                    .world
                    .query_one_mut::<(&mut Cell, &Genome)>(survivor)
                    .unwrap();
                if next_mode < genome.len() {
                    cell.mode = next_mode;
                    cell.mass = total;
                    cell.age = 0.;
                    cell.cooldown = 3.;
                    cell.signal = [0.; 4];
                    true
                } else {
                    false
                }
            };
            if ok {
                dead.push(victim);
                removed.insert(victim);
            }
        }
        for pair in cut_bonds {
            self.bonds.remove(&pair);
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
