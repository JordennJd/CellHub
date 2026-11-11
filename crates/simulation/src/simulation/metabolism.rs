//! Tile-owned metabolism jobs, independent of ECS mutation and division order.
use super::Simulation;
use super::interactions::ContactMotion;
use super::program::{SPEED_TARGET, controlled_value};
use super::specialized::{buoyant_drift, corpse_food, differentiate};
use super::step::STEP;
use crate::cell_types::GAMETE;
use crate::cell_types::{
    BUOYOCYTE, COUNT, FLAGELLOCYTE, KERATINOCYTE, LIPOCYTE, PHAGOCYTE, PHOTOCYTE,
};
use crate::components::{Cell, MIN_DIVISION_AGE, NITROGEN_DEATH, NITROGEN_DIVISION, Position};
use crate::genome::Genome;
use crate::terrain::index;
use std::collections::BTreeMap;

const MIN_CELL_MASS: f32 = 0.18;
const MOMENTUM_DRAG: f32 = 4.;

impl Simulation {
    pub(super) fn metabolize(
        &mut self,
        entities: &[(u32, hecs::Entity)],
        degrees: &BTreeMap<u32, usize>,
    ) -> MetabolismScratch {
        let population = entities.len();
        let can_grow = population < self.population_limit;
        let settings = self.settings;
        let grid = (self.boundary.size() / 64.) as usize;
        let chaos = self.chaos.is_some();
        let passive_drift = settings.vertical_drift();
        let momentum_decay = libm::expf(-MOMENTUM_DRAG * settings.viscosity * STEP);
        let mut scratch = std::mem::take(&mut self.metabolism);
        // Dense ECS indices map directly to stable commit slots. Random draws
        // follow the existing ID-ordered entity list, regardless of ECS layout.
        let slots = entities
            .iter()
            .map(|(_, entity)| entity.id() as usize + 1)
            .max()
            .unwrap_or(0);
        scratch.indices.resize(slots, 0);
        scratch.random.clear();
        for (order, &(_, entity)) in entities.iter().enumerate() {
            scratch.indices[entity.id() as usize] = order;
            scratch.random.push([
                super::draw_random(&mut self.rng),
                super::draw_random(&mut self.rng),
                super::draw_random(&mut self.rng),
            ]);
        }
        // Mutable ECS references are disjoint, as are the substrate slices.
        let mut bodies: Vec<_> = self
            .world
            .query_mut::<(&mut Position, &mut Cell, &mut Genome)>()
            .into_iter()
            .map(|(entity, (p, c, g))| {
                let order = scratch.indices[entity.id() as usize];
                Body {
                    order,
                    tile: index(p.x, p.y, grid),
                    result: MetabolismResult {
                        motion: ContactMotion {
                            path: [*p; 2],
                            drive: [0.; 2],
                        },
                        division: None,
                        died: false,
                    },
                    p,
                    c,
                    g,
                    random: scratch.random[order],
                }
            })
            .collect();
        // Sort compact keys/references rather than moving component metadata.
        let mut ordered: Vec<_> = bodies
            .iter_mut()
            .map(|body| (((body.tile as u64) << 32) | body.order as u64, body))
            .collect();
        ordered.sort_unstable_by_key(|(key, _)| *key);
        let mut jobs = Vec::new();
        let mut remaining = ordered.as_mut_slice();
        for (index, tile) in self.tiles.iter_mut().enumerate() {
            let count = remaining.partition_point(|(key, _)| (*key >> 32) as usize == index);
            let (cells, rest) = remaining.split_at_mut(count);
            remaining = rest;
            if !cells.is_empty() {
                jobs.push((tile, cells));
            }
        }
        let boundary = self.boundary;
        super::interactions::for_each_island(&mut jobs, population, |(t, cells)| {
            for (_, item) in cells.iter_mut() {
                let p = &mut *item.p;
                let c = &mut *item.c;
                let g = &mut *item.g;
                let [noise, mutation_roll, mutation_gene] = item.random;
                let start = *p;
                c.age += STEP;
                c.cooldown = (c.cooldown - STEP).max(0.);
                let degree = degrees.get(&c.id).copied().unwrap_or(0);
                let differentiated = differentiate(c, g, degree);
                let m = *g.mode(c.mode);
                let programmed_speed = controlled_value(c, &m, SPEED_TARGET, degree).clamp(0., 60.);
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
                    let index =
                        ((mutation_gene * modes.len() as f32) as usize).min(modes.len() - 1);
                    let modes_count = modes.len();
                    let target = &mut modes[index];
                    if chaos {
                        super::chaos::mutate(
                            target,
                            modes_count,
                            noise,
                            settings.only_point_mutations,
                        );
                    } else {
                        if !settings.only_point_mutations {
                            target.kind = (target.kind + 1) % COUNT;
                        }
                        target.threshold = (target.threshold + (noise - 0.5) * 0.8).clamp(1., 8.);
                    }
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
                let (drive_x, drive_y) = if speed == 0. {
                    (0., 0.)
                } else {
                    (
                        libm::cosf(p.angle) * speed * STEP / settings.viscosity,
                        libm::sinf(p.angle) * speed * STEP / settings.viscosity,
                    )
                };
                let mut next_x = p.x + drive_x;
                let mut next_y = p.y + drive_y + vertical_drift * STEP;
                if c.recoil != [0.; 2] {
                    let scale = STEP / c.mass.max(MIN_CELL_MASS);
                    next_x += c.recoil[0] * scale;
                    next_y += c.recoil[1] * scale;
                }
                let (x, y, reached_edge) = if c.anchored {
                    (p.x, p.y, false)
                } else {
                    boundary.constrain(next_x, next_y, c.mass, 12.)
                };
                p.x = x;
                p.y = y;
                if c.anchored || reached_edge {
                    c.recoil = [0.; 2];
                } else {
                    c.recoil = c.recoil.map(|momentum| momentum * momentum_decay);
                }
                let died = c.mass < MIN_CELL_MASS
                    || c.nitrogen < NITROGEN_DEATH
                    || c.toxin >= 1.
                    || c.injury >= 1.
                    || c.telomeres == 0
                    || (settings.aging && c.age > m.lifespan)
                    || (settings.kill_at_edge && reached_edge);
                if died {
                    corpse_food(t, c, m.kind);
                }
                item.result.motion = ContactMotion {
                    path: [start, *p],
                    drive: if c.anchored || reached_edge {
                        [0.; 2]
                    } else {
                        [drive_x / STEP, drive_y / STEP + vertical_drift]
                    },
                };
                item.result.died = died;
                if can_grow
                    && !died
                    && m.can_divide
                    && m.kind != GAMETE
                    && !differentiated
                    && c.age + 0.000001 >= MIN_DIVISION_AGE
                    && c.mass >= m.threshold
                    && c.nitrogen >= NITROGEN_DIVISION
                    && m.max_connections.is_none_or(|limit| degree < limit)
                {
                    item.result.division = Some(Box::new(m));
                }
            }
        });
        drop(jobs);
        drop(ordered);
        scratch
            .results
            .resize_with(population, || MetabolismResult {
                motion: ContactMotion {
                    path: [Position {
                        x: 0.,
                        y: 0.,
                        angle: 0.,
                    }; 2],
                    drive: [0.; 2],
                },
                division: None,
                died: false,
            });
        for body in bodies {
            scratch.results[body.order] = body.result;
        }
        scratch
    }
}

struct Body<'a> {
    order: usize,
    tile: usize,
    p: &'a mut Position,
    c: &'a mut Cell,
    g: &'a mut Genome,
    random: [f32; 3],
    result: MetabolismResult,
}

pub(super) struct MetabolismResult {
    pub(super) motion: ContactMotion,
    pub(super) division: Option<Box<crate::genome::Mode>>,
    pub(super) died: bool,
}

#[derive(Default)]
pub(super) struct MetabolismScratch {
    indices: Vec<usize>,
    random: Vec<[f32; 3]>,
    pub(super) results: Vec<MetabolismResult>,
}
