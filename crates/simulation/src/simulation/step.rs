//! One simulation tick: metabolism, interactions, death, division and bonds.

use super::Simulation;
use super::bonds::{ADHESION_GAP, BondDivision};
use super::interactions::ContactMotion;
use super::level::LevelEvent;
use crate::components::{Cell, LIMIT, MAX_RECOIL_MOMENTUM, Position, STEP, radius};
use crate::genome::Genome;
use std::collections::{BTreeMap, HashSet};

const DIVISION_SEPARATION_SPEED: f32 = 100.;
/// Up to this fraction of the population is culled per tick while the soft
/// limit is only slightly exceeded. Above twice the limit the whole excess is
/// retired, so a bloom cannot outrun the pressure.
const POPULATION_CULL_DIVISOR: usize = 10;

impl Simulation {
    pub(super) fn step(&mut self) {
        self.dispatch_level_demo();
        self.tick += 1;
        self.update_chaos();
        self.cull_overpopulation();
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
            if let Some(m) = item.division.filter(|_| {
                self.level.is_none() || population + children.len() / 2 < self.population_limit
            }) {
                let (p, c, g) = self
                    .world
                    .query_one_mut::<(&Position, &Cell, &Genome)>(e)
                    .unwrap();
                let offspring = g.offspring();
                let inherited_mode = offspring.mode(c.mode);
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
                a.class = c.class;
                b.class = c.class;
                a.mode = inherited_mode.child_a;
                b.mode = inherited_mode.child_b;
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
                children.push((pa, a, offspring.clone(), c.id));
                children.push((pb, b, offspring, c.id));
                dead.push(e);
                continue;
            }
            paths.push(item.motion);
        }
        let mut death_events = Vec::new();
        if !dead.is_empty() {
            let removed: HashSet<_> = dead.iter().copied().collect();
            self.deaths += (dead.len() - divisions.len()) as u32;
            death_events = dead
                .iter()
                .filter_map(|&entity| {
                    let (position, cell, genome) = self
                        .world
                        .query_one_mut::<(&Position, &Cell, &Genome)>(entity)
                        .ok()?;
                    let replaced = divisions.contains_key(&cell.id);
                    Some(
                        LevelEvent::new(self.tick, if replaced { "divide" } else { "death" })
                            .with_cell(cell)
                            .with_position(position)
                            .with_color(genome, cell.mode)
                            .with_detail(if replaced {
                                "replaced by daughters"
                            } else {
                                "metabolism"
                            }),
                    )
                })
                .collect::<Vec<_>>();
            entities.retain(|(_, entity)| !removed.contains(entity));
            for e in dead {
                self.world.despawn(e).unwrap();
            }
        }
        for event in death_events {
            self.emit_level_event(event);
        }
        for (p, c, g, parent) in children {
            let id = c.id;
            let event = LevelEvent::new(self.tick, "birth")
                .with_cell(&c)
                .with_position(&p)
                .with_color(&g, c.mode)
                .with_parent(parent);
            let entity = self.world.spawn((p, c, g));
            paths.push(ContactMotion {
                path: [p; 2],
                drive: [0.; 2],
            });
            entities.push((id, entity));
            self.births += 1;
            self.emit_level_event(event);
        }
        self.entities = entities;
        self.replace_bonds(&divisions, &degrees);
        self.metabolism = scratch;
        // Constraint solving can move cells across buckets or into a different
        // colony. Build the contact graph only after those positions are known.
        self.resolve_contacts(paths, STEP);
        self.specialized_contacts(STEP);
        self.update_level_status();
        self.accumulate_owner_scores();
        self.record_chaos_history();
    }

    /// Soft population pressure. Sandbox, Chaos and multiplayer worlds accept
    /// growth past `population_limit`, but the oldest cells start dying at a
    /// rate that brings large overflows back within a few ticks. Campaign
    /// substrates keep their authored hard cap and are never culled here.
    fn cull_overpopulation(&mut self) {
        if self.level.is_some() {
            return;
        }
        let population = self.world.len() as usize;
        let excess = population.saturating_sub(self.population_limit);
        if excess == 0 {
            return;
        }
        let budget = if population > 2 * self.population_limit {
            excess
        } else {
            excess.min((population / POPULATION_CULL_DIVISOR).max(1))
        };
        let mut oldest: Vec<_> = self
            .world
            .query::<(&Cell, &Position)>()
            .iter()
            .map(|(entity, (cell, position))| (cell.age, cell.id, entity, *position))
            .collect();
        let compare = |a: &(f32, u32, _, Position), b: &(f32, u32, _, Position)| {
            b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1))
        };
        if budget < oldest.len() {
            oldest.select_nth_unstable_by(budget - 1, compare);
            oldest.truncate(budget);
        }
        oldest.sort_unstable_by(compare);
        let mut killed = HashSet::with_capacity(budget);
        for (_, id, entity, position) in oldest.into_iter().take(budget) {
            let event = self
                .world
                .query_one_mut::<(&Cell, &Genome)>(entity)
                .ok()
                .map(|(cell, genome)| {
                    LevelEvent::new(self.tick, "death")
                        .with_cell(cell)
                        .with_position(&position)
                        .with_color(genome, cell.mode)
                        .with_detail("population pressure")
                });
            if self.world.despawn(entity).is_ok() {
                killed.insert(id);
                if let Some(event) = event {
                    self.emit_level_event(event);
                }
            }
        }
        if killed.is_empty() {
            return;
        }
        self.entities.retain(|&(id, _)| !killed.contains(&id));
        self.bonds
            .retain(|&(a, b)| !killed.contains(&a) && !killed.contains(&b));
        self.deaths += killed.len() as u32;
    }
}
