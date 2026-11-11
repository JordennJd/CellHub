//! Sensing, signalling, differentiation and signal diffusion.
//!
//! Sensors sample the same spatial hash that powers contact interactions, so a
//! senseocyte never scans the whole population. Producers write S1..S4 into the
//! cell, then `exchange_bond_signals` propagates them across the adhesion graph;
//! lipocytes act as insulators.

use super::Simulation;
use super::boundary::Boundary;
use super::spatial::{BUCKET_SIZE, SpatialGrid};
use crate::cell_types::{
    CILIOCYTE, FLAGELLOCYTE, LIPOCYTE, NEUROCYTE, SECROCYTE, SENSEOCYTE, STEMOCYTE, STEREOCYTE,
};
use crate::components::{Cell, Position, SIZE};
use crate::genome::Genome;
use crate::terrain::{Tile, tile_index};
use hecs::Entity;
use std::collections::HashSet;

const STEP: f32 = 1. / 30.;

impl Simulation {
    pub(super) fn update_signals(
        &mut self,
        entities: &[(u32, Entity)],
        removed: &HashSet<Entity>,
        spatial: &SpatialGrid,
    ) {
        let tiles = &self.tiles;
        let boundary = self.boundary;
        for &(_, entity) in entities {
            if removed.contains(&entity) {
                continue;
            }
            let Ok((position, cell, genome)) = self
                .world
                .query_one_mut::<(&Position, &mut Cell, &Genome)>(entity)
            else {
                continue;
            };
            let mode = *genome.mode(cell.mode);
            match mode.kind {
                NEUROCYTE => {
                    let channel = (mode.channel as usize).min(3);
                    cell.signal[channel] = mode.signal.clamp(-60., 60.);
                }
                SENSEOCYTE | STEREOCYTE => {
                    let channel = (mode.channel as usize).min(3);
                    let range = (20. + mode.speed * 2.).clamp(20., 180.);
                    let value = if mode.kind == STEREOCYTE {
                        directional_stimulus(
                            spatial,
                            tiles,
                            position,
                            mode.subtype,
                            range,
                            boundary,
                        )
                    } else {
                        stimulus(spatial, tiles, position, mode.subtype, range, boundary)
                    };
                    cell.signal[channel] = (value * mode.signal.clamp(0., 60.)).clamp(-100., 100.);
                }
                SECROCYTE => {
                    let amount = mode.signal.max(0.) * STEP;
                    let channel = match mode.subtype {
                        3..=6 => Some((mode.subtype - 3) as usize),
                        7..=10 => Some((mode.subtype - 7) as usize),
                        11..=14 => Some((mode.subtype - 11) as usize),
                        _ => None,
                    };
                    if let Some(channel) = channel {
                        let sign = if (7..=10).contains(&mode.subtype) {
                            -1.
                        } else {
                            1.
                        };
                        cell.signal[channel] =
                            (cell.signal[channel] + sign * amount).clamp(-100., 100.);
                    }
                }
                STEMOCYTE => {
                    // Path 1 has priority, and both paths need a sustained signal.
                    if cell.age >= 0.5 {
                        let target = if cell.signal[0] >= 0.5 {
                            Some(mode.child_a)
                        } else if cell.signal[1] >= 0.5 {
                            Some(mode.child_b)
                        } else {
                            None
                        };
                        if let Some(target) = target
                            && target != cell.mode
                            && target < genome.len()
                        {
                            cell.mode = target;
                            cell.age = 0.;
                        }
                    }
                }
                _ => {}
            }
        }
        self.exchange_bond_signals(0.35);
    }
}

fn stimulus(
    spatial: &SpatialGrid,
    tiles: &[Tile],
    position: &Position,
    subtype: u8,
    range: f32,
    boundary: Boundary,
) -> f32 {
    match subtype {
        0 => neighbour_weight(spatial, position, range, None),
        1 => ambient(tiles, position, subtype, boundary, range),
        2 => neighbour_weight(spatial, position, range, Some(LIPOCYTE)),
        3 | 4 => ambient(tiles, position, subtype, boundary, range),
        5 => {
            neighbour_weight(spatial, position, range, Some(FLAGELLOCYTE))
                + neighbour_weight(spatial, position, range, Some(CILIOCYTE))
        }
        _ => 0.,
    }
}

fn directional_stimulus(
    spatial: &SpatialGrid,
    tiles: &[Tile],
    position: &Position,
    subtype: u8,
    range: f32,
    boundary: Boundary,
) -> f32 {
    let left = (-position.angle.sin(), position.angle.cos());
    match subtype {
        0 | 2 => {
            let filter = (subtype == 2).then_some(LIPOCYTE);
            let mut left_sum = 0.;
            let mut right_sum = 0.;
            visit_neighbours(spatial, position, range, |kind, dx, dy| {
                if filter.is_some_and(|wanted| kind != wanted) {
                    return;
                }
                let side = dx * left.0 + dy * left.1;
                if side > 0. {
                    left_sum += 1.;
                } else if side < 0. {
                    right_sum += 1.;
                }
            });
            normalize_difference(left_sum, right_sum)
        }
        1 | 3 | 4 => {
            let offset = range * 0.35;
            let sample = |x: f32, y: f32| {
                let probe = Position {
                    x: x.clamp(0., SIZE),
                    y: y.clamp(0., SIZE),
                    angle: position.angle,
                };
                ambient(tiles, &probe, subtype, boundary, range.max(1.))
            };
            let left_value = sample(position.x + left.0 * offset, position.y + left.1 * offset);
            let right_value = sample(position.x - left.0 * offset, position.y - left.1 * offset);
            normalize_difference(left_value, right_value)
        }
        5 => {
            let mut left_sum = 0.;
            let mut right_sum = 0.;
            visit_neighbours(spatial, position, range, |kind, dx, dy| {
                if kind != FLAGELLOCYTE && kind != CILIOCYTE {
                    return;
                }
                let side = dx * left.0 + dy * left.1;
                if side > 0. {
                    left_sum += 1.;
                } else if side < 0. {
                    right_sum += 1.;
                }
            });
            normalize_difference(left_sum, right_sum)
        }
        _ => 0.,
    }
}

fn ambient(
    tiles: &[Tile],
    position: &Position,
    subtype: u8,
    boundary: Boundary,
    range: f32,
) -> f32 {
    let tile = tiles[tile_index(position.x, position.y)];
    match subtype {
        1 => (tile.food / tile.capacity.max(0.0001)).clamp(0., 1.),
        3 => tile.light.clamp(0., 1.),
        4 => {
            let distance = match boundary {
                Boundary::Circle => {
                    let center = SIZE / 2.;
                    center - (position.x - center).hypot(position.y - center)
                }
                Boundary::Square => position
                    .x
                    .min(position.y)
                    .min(SIZE - position.x)
                    .min(SIZE - position.y),
            };
            (1. - distance / range.max(1.)).clamp(0., 1.)
        }
        _ => 0.,
    }
}

fn neighbour_weight(
    spatial: &SpatialGrid,
    position: &Position,
    range: f32,
    filter: Option<u8>,
) -> f32 {
    let mut count: f32 = 0.;
    visit_neighbours(spatial, position, range, |kind, _, _| {
        if filter.is_none_or(|wanted| kind == wanted) {
            count += 1.;
        }
    });
    (count / 5.).clamp(0., 1.)
}

fn normalize_difference(left: f32, right: f32) -> f32 {
    let total = left + right;
    if total <= 0.0001 {
        0.
    } else {
        ((left - right) / total).clamp(-1., 1.)
    }
}

fn visit_neighbours(
    spatial: &SpatialGrid,
    position: &Position,
    range: f32,
    mut visit: impl FnMut(u8, f32, f32),
) {
    let bx = (position.x / BUCKET_SIZE) as isize;
    let by = (position.y / BUCKET_SIZE) as isize;
    let reach = (range / BUCKET_SIZE).ceil() as isize;
    let range2 = range * range;
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            if let Some(bucket) = spatial.bucket(bx + dx, by + dy) {
                for neighbour in bucket {
                    let ox = neighbour.position.x - position.x;
                    let oy = neighbour.position.y - position.y;
                    let distance2 = ox * ox + oy * oy;
                    if distance2 > 0.01 && distance2 <= range2 {
                        visit(neighbour.kind, ox, oy);
                    }
                }
            }
        }
    }
}
