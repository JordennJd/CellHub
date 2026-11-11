//! Synchronous signals, sensory responses, and contact secretions.

use super::Simulation;
use super::boundary::Boundary;
use super::program::{SECRETION_RATE_TARGET, SIGNAL_CHANNELS, SPEED_TARGET, controlled_value};
use crate::cell_types::{
    LIPOCYTE, NEUROCYTE, SECROCYTE, SENSEOCYTE, STEREOCYTE, blocks_signals, is_motile,
};
use crate::components::{Cell, Position, radius};
use crate::genome::{Genome, Mode};
use crate::terrain::Tile;
#[cfg(test)]
use crate::terrain::tile_index;
use hecs::Entity;
use std::collections::{BTreeMap, BTreeSet, HashSet};

const SENSE_RANGE: f32 = 96.;
const SENSE_LIPOCYTES: u8 = 7;
const SENSE_MOTILE_CELLS: u8 = 8;
const POINT_BUCKET: f32 = 128.;
const SECRETION_NITROGEN: f32 = 0.2;

struct Body {
    entity: Entity,
    position: Position,
    cell: Cell,
    genome: Genome,
}

impl Body {
    fn kind(&self) -> u8 {
        self.genome.mode(self.cell.mode).kind
    }
}

struct Updated {
    signals: [f32; SIGNAL_CHANNELS],
    toxin: f32,
    injury: f32,
    mass: f32,
    nitrogen: f32,
}

/// Static point buckets suit sensory queries, whose range is larger than a
/// contact radius. Query order depends only on bucket order and stable cell IDs.
struct PointGrid {
    columns: usize,
    rows: usize,
    buckets: Vec<Vec<usize>>,
}

fn bucket(value: f32, count: usize) -> usize {
    ((value / POINT_BUCKET) as usize).min(count - 1)
}

impl PointGrid {
    fn new(bodies: &[Body], boundary: Boundary) -> Self {
        let columns = (boundary.width() / POINT_BUCKET).ceil() as usize;
        let rows = (boundary.size() / POINT_BUCKET).ceil() as usize;
        let mut buckets = vec![Vec::new(); columns * rows];
        for (index, body) in bodies.iter().enumerate() {
            buckets[bucket(body.position.y, rows) * columns + bucket(body.position.x, columns)]
                .push(index);
        }
        Self {
            columns,
            rows,
            buckets,
        }
    }

    fn nearby(&self, p: Position, range: f32) -> impl Iterator<Item = usize> + '_ {
        let left = bucket(p.x - range, self.columns);
        let right = bucket(p.x + range, self.columns);
        let top = bucket(p.y - range, self.rows);
        let bottom = bucket(p.y + range, self.rows);
        (top..=bottom).flat_map(move |y| {
            (left..=right).flat_map(move |x| self.buckets[y * self.columns + x].iter().copied())
        })
    }
}

fn distance_squared(a: Position, b: Position) -> f32 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

fn wall_distance(p: Position, boundary: Boundary) -> f32 {
    boundary.wall_distance(p.x, p.y)
}

fn field_sample(tiles: &[Tile], p: Position, sensor: u8, boundary: Boundary) -> f32 {
    let tile = tiles[boundary.tile(p.x, p.y)];
    match sensor {
        0 => tile.light,
        1 => tile.food,
        2 => tile.covered_food,
        4 => (1. - wall_distance(p, boundary) / SENSE_RANGE).clamp(0., 1.),
        6 => tile.nitrates,
        _ => 0.,
    }
}

fn color_matches(source: &Mode, target: &Mode) -> bool {
    if source.target_color[0] < 0. {
        return true;
    }
    target
        .resolved_color()
        .iter()
        .zip(source.target_color)
        .all(|(a, b)| (a - b).abs() <= source.sense_tolerance)
}

fn mimic_sensor(secretion: u8) -> Option<u8> {
    match secretion {
        3 => Some(1),
        4 => Some(2),
        5 => Some(0),
        6 => Some(4),
        _ => None,
    }
}

fn stimulus(
    index: usize,
    bodies: &[Body],
    tiles: &[Tile],
    grid: Option<&PointGrid>,
    boundary: Boundary,
    degree: &BTreeMap<u32, usize>,
) -> f32 {
    let body = &bodies[index];
    let mode = body.genome.mode(body.cell.mode);
    let stereo = mode.kind == STEREOCYTE;
    if mode.sensor == 5 {
        let speed = if stereo {
            0.
        } else {
            controlled_value(
                &body.cell,
                mode,
                SPEED_TARGET,
                degree.get(&body.cell.id).copied().unwrap_or(0),
            ) / 60.
        };
        return speed * mode.sensor_gain;
    }
    let side = if stereo {
        let hand = if body.cell.mirrored { -1. } else { 1. };
        [
            -libm::sinf(body.position.angle) * hand,
            libm::cosf(body.position.angle) * hand,
        ]
    } else {
        [0.; 2]
    };
    let mut result = if stereo {
        let offset = |sign: f32| Position {
            x: (body.position.x + side[0] * 32. * sign).clamp(0., boundary.width()),
            y: (body.position.y + side[1] * 32. * sign).clamp(0., boundary.size()),
            angle: body.position.angle,
        };
        field_sample(tiles, offset(1.), mode.sensor, boundary)
            - field_sample(tiles, offset(-1.), mode.sensor, boundary)
    } else {
        field_sample(tiles, body.position, mode.sensor, boundary)
    };
    let Some(grid) = grid else {
        return result * mode.sensor_gain;
    };
    let range = match mode.sensor {
        SENSE_LIPOCYTES | SENSE_MOTILE_CELLS => (20. + mode.speed * 2.).clamp(20., 180.),
        _ => SENSE_RANGE,
    };
    for other in grid.nearby(body.position, range) {
        if other == index {
            continue;
        }
        let target = &bodies[other];
        let d2 = distance_squared(body.position, target.position);
        if d2 > range * range {
            continue;
        }
        let target_mode = target.genome.mode(target.cell.mode);
        let cells = match mode.sensor {
            3 => true,
            SENSE_LIPOCYTES => target_mode.kind == LIPOCYTE,
            SENSE_MOTILE_CELLS => is_motile(target_mode.kind),
            _ => false,
        } && color_matches(mode, target_mode);
        let mimic = target_mode.kind == SECROCYTE
            && target.cell.nitrogen >= SECRETION_NITROGEN
            && mimic_sensor(target_mode.secretion) == Some(mode.sensor);
        if !(cells || mimic) {
            continue;
        }
        let distance = libm::sqrtf(d2);
        let mut contribution = (1. - distance / range).max(0.);
        if mimic {
            contribution *= controlled_value(
                &target.cell,
                target_mode,
                SECRETION_RATE_TARGET,
                degree.get(&target.cell.id).copied().unwrap_or(0),
            );
        }
        if stereo && distance > 0.0001 {
            contribution *= ((target.position.x - body.position.x) * side[0]
                + (target.position.y - body.position.y) * side[1])
                / distance;
        }
        result += contribution;
    }
    result * mode.sensor_gain
}

impl Simulation {
    pub(super) fn update_signals_and_secretions(&mut self, step: f32) {
        let active = self
            .world
            .query::<(&Cell, &Genome)>()
            .iter()
            .any(|(_, (cell, genome))| {
                matches!(
                    genome.mode(cell.mode).kind,
                    STEREOCYTE | SENSEOCYTE | NEUROCYTE | SECROCYTE
                ) || cell.signals.iter().any(|value| *value != 0.)
                    || cell.toxin != 0.
                    || cell.injury != 0.
            });
        if !active {
            return;
        }
        let mut mimic_sensors = [false; 7];
        let mut has_protease = false;
        let bodies: Vec<_> = self
            .entities
            .iter()
            .map(|&(_, entity)| {
                let (p, c, g) = self
                    .world
                    .query_one_mut::<(&Position, &Cell, &Genome)>(entity)
                    .unwrap();
                let mode = g.mode(c.mode);
                if mode.kind == SECROCYTE && c.nitrogen >= SECRETION_NITROGEN {
                    if let Some(sensor) = mimic_sensor(mode.secretion) {
                        mimic_sensors[usize::from(sensor)] = true;
                    }
                    has_protease |= mode.secretion == 2;
                }
                Body {
                    entity,
                    position: *p,
                    cell: c.clone(),
                    genome: g.clone(),
                }
            })
            .collect();
        let mut updated: Vec<_> = bodies
            .iter()
            .map(|body| Updated {
                signals: body.cell.signals.map(|value| value * (1. - step * 0.15)),
                toxin: (body.cell.toxin - step * 0.015).max(0.),
                injury: (body.cell.injury - step * 0.01).max(0.),
                mass: body.cell.mass,
                nitrogen: body.cell.nitrogen,
            })
            .collect();
        let degree = self.bond_degrees();
        let index = |id| {
            bodies
                .binary_search_by_key(&id, |body| body.cell.id)
                .unwrap()
        };
        let mut deltas = vec![[0_f64; SIGNAL_CHANNELS + 1]; bodies.len()];
        let mut neighbors = BTreeMap::<u32, Vec<u32>>::new();
        for &(a, b) in &self.bonds {
            let ai = index(a);
            let bi = index(b);
            if has_protease {
                neighbors.entry(a).or_default().push(b);
                neighbors.entry(b).or_default().push(a);
            }
            let share = f64::from((step * 1.5).min(0.5)) / degree[&a].max(degree[&b]) as f64;
            if !blocks_signals(bodies[ai].kind()) && !blocks_signals(bodies[bi].kind()) {
                for channel in 0..SIGNAL_CHANNELS {
                    let flow = f64::from(
                        bodies[ai].cell.signals[channel] - bodies[bi].cell.signals[channel],
                    ) * share;
                    deltas[ai][channel] -= flow;
                    deltas[bi][channel] += flow;
                }
            }
            let toxin = f64::from(bodies[ai].cell.toxin - bodies[bi].cell.toxin) * share;
            deltas[ai][SIGNAL_CHANNELS] -= toxin;
            deltas[bi][SIGNAL_CHANNELS] += toxin;
        }
        for (state, delta) in updated.iter_mut().zip(deltas) {
            for (channel, value) in state.signals.iter_mut().enumerate() {
                *value = (f64::from(*value) + delta[channel]) as f32;
            }
            state.toxin = (f64::from(state.toxin) + delta[SIGNAL_CHANNELS]).max(0.) as f32;
        }
        let mut grid = None;
        for (i, body) in bodies.iter().enumerate() {
            let mode = body.genome.mode(body.cell.mode);
            if matches!(mode.kind, STEREOCYTE | SENSEOCYTE) {
                let senses_neighbors =
                    matches!(mode.sensor, 3 | SENSE_LIPOCYTES | SENSE_MOTILE_CELLS)
                        || mimic_sensors.get(usize::from(mode.sensor)) == Some(&true);
                let nearby = senses_neighbors
                    .then(|| &*grid.get_or_insert_with(|| PointGrid::new(&bodies, self.boundary)));
                updated[i].signals[usize::from(mode.signal_channel)] =
                    stimulus(i, &bodies, &self.tiles, nearby, self.boundary, &degree)
                        .clamp(-8., 8.);
            } else if mode.kind == NEUROCYTE {
                for channel in 0..SIGNAL_CHANNELS {
                    updated[i].signals[channel] = controlled_value(
                        &body.cell,
                        mode,
                        channel,
                        degree.get(&body.cell.id).copied().unwrap_or(0),
                    );
                }
            }
        }
        let mut protected = None;
        let mut radii = None;
        let mut broken = BTreeSet::new();
        for (i, source) in bodies.iter().enumerate() {
            let mode = source.genome.mode(source.cell.mode);
            if mode.kind != SECROCYTE || source.cell.nitrogen < SECRETION_NITROGEN {
                continue;
            }
            let rate = controlled_value(
                &source.cell,
                mode,
                SECRETION_RATE_TARGET,
                degree.get(&source.cell.id).copied().unwrap_or(0),
            );
            if rate <= 0. {
                continue;
            }
            updated[i].nitrogen = (updated[i].nitrogen - rate * step * 0.02).max(0.);
            updated[i].mass = (updated[i].mass - rate * step * 0.005).max(0.);
            if mimic_sensor(mode.secretion).is_some() {
                continue;
            }
            if mode.secretion == 1 {
                let tile =
                    &mut self.tiles[self.boundary.tile(source.position.x, source.position.y)];
                let stripped = (rate * step * 0.25)
                    .min(tile.covered_food)
                    .min((tile.capacity - tile.food).max(0.));
                tile.covered_food -= stripped;
                tile.food += stripped;
            }
            let mut contact_ids = BTreeSet::new();
            let grid = grid.get_or_insert_with(|| PointGrid::new(&bodies, self.boundary));
            let (radii, largest_radius) = radii.get_or_insert_with(|| {
                let radii: Vec<_> = bodies.iter().map(|body| radius(body.cell.mass)).collect();
                let largest = radii.iter().copied().fold(0., f32::max);
                (radii, largest)
            });
            // Normal cells fit into neighboring buckets. Very large cells use
            // the full map range so radius-based contacts cannot be skipped.
            let range = (radii[i] + *largest_radius + 2.).max(SENSE_RANGE);
            for j in grid.nearby(source.position, range) {
                if i == j {
                    continue;
                }
                let target = &bodies[j];
                let reach = radii[i] + radii[j] + 2.;
                if distance_squared(source.position, target.position) > reach * reach {
                    continue;
                }
                let shielded = mode.secretion <= 2
                    && protected
                        .get_or_insert_with(|| self.keratin_protection(&HashSet::new()))
                        .contains(&target.cell.id);
                match mode.secretion {
                    0 if !shielded => updated[j].toxin += rate * step * 0.28,
                    1 if !shielded => updated[j].injury += rate * step * 0.14,
                    2 if !shielded => {
                        contact_ids.insert(target.cell.id);
                    }
                    7..=22 => {
                        let channel = usize::from((mode.secretion - 7) / 2);
                        let sign = if mode.secretion % 2 == 1 { 1. } else { -1. };
                        updated[j].signals[channel] += sign * rate * step * 2.;
                    }
                    _ => {}
                }
            }
            if mode.secretion == 2 {
                for &a in &contact_ids {
                    for &b in neighbors.get(&a).into_iter().flatten() {
                        if contact_ids.contains(&b) {
                            broken.insert((a.min(b), a.max(b)));
                        }
                    }
                }
            }
        }
        self.bonds.retain(|edge| !broken.contains(edge));
        for (body, state) in bodies.into_iter().zip(updated) {
            let mut cell = self.world.get::<&mut Cell>(body.entity).unwrap();
            cell.signals = state.signals.map(|v| v.clamp(-8., 8.));
            cell.toxin = state.toxin.clamp(0., 2.);
            cell.injury = state.injury.clamp(0., 2.);
            cell.mass = state.mass;
            cell.nitrogen = state.nitrogen;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell_types::{CILIOCYTE, FLAGELLOCYTE, PHOTOCYTE};
    use crate::genome::Mode;

    fn spawn(sim: &mut Simulation, kind: u8, x: f32, y: f32, set: impl FnOnce(&mut Mode)) -> u32 {
        let mut mode = Mode {
            kind,
            speed: 0.,
            can_divide: false,
            ..Mode::default()
        };
        set(&mut mode);
        sim.spawn_checked(x, y, 0, &Genome::from_modes(vec![mode]).values())
            .unwrap()
    }

    fn state(sim: &Simulation, id: u32) -> Cell {
        let entity = sim.entities.iter().find(|&&(key, _)| key == id).unwrap().1;
        (*sim.world.get::<&Cell>(entity).unwrap()).clone()
    }

    #[test]
    fn eighth_signal_diffuses_through_bonds_and_lipocyte_blocks_only_signals() {
        let mut sim = Simulation::new(11);
        let source = spawn(&mut sim, 13, 500., 500., |m| {
            m.signal_channel = 7;
            m.sensor_gain = -2.;
        });
        let middle = spawn(&mut sim, 0, 522., 500., |_| {});
        let lipid = spawn(&mut sim, 7, 544., 500., |_| {});
        let end = spawn(&mut sim, 0, 566., 500., |_| {});
        sim.bonds
            .extend([(source, middle), (middle, lipid), (lipid, end)]);
        for _ in 0..100 {
            sim.update_signals_and_secretions(1. / 30.);
        }
        assert_eq!(state(&sim, source).signals[7], -2.);
        assert!(state(&sim, middle).signals[7] < -0.5);
        assert_eq!(state(&sim, lipid).signals[7], 0.);
        assert_eq!(state(&sim, end).signals[7], 0.);
        let entity = sim.entities[0].1;
        sim.world.get::<&mut Cell>(entity).unwrap().toxin = 1.;
        for _ in 0..100 {
            sim.update_signals_and_secretions(1. / 30.);
        }
        assert!(state(&sim, lipid).toxin > 0.);
        assert!(state(&sim, end).toxin > 0.);
    }

    #[test]
    fn stereocyte_preserves_left_right_sign_mirroring_and_color_filter() {
        let mut sim = Simulation::new(12);
        let sensor = spawn(&mut sim, 10, 500., 500., |m| {
            m.sensor = 3;
            m.target_color = [1., 0., 0.];
            m.sense_tolerance = 0.01;
        });
        let target = spawn(&mut sim, 0, 500., 530., |m| {
            m.color = [1., 0., 0.];
        });
        sim.world
            .get::<&mut Position>(sim.entities[0].1)
            .unwrap()
            .angle = 0.;
        sim.update_signals_and_secretions(1. / 30.);
        assert!(state(&sim, sensor).signals[0] > 0.5);
        sim.world.get::<&mut Position>(sim.entities[1].1).unwrap().y = 470.;
        sim.update_signals_and_secretions(1. / 30.);
        assert!(state(&sim, sensor).signals[0] < -0.5);
        sim.world
            .get::<&mut Cell>(sim.entities[0].1)
            .unwrap()
            .mirrored = true;
        sim.update_signals_and_secretions(1. / 30.);
        assert!(state(&sim, sensor).signals[0] > 0.5);
        let entity = sim
            .entities
            .iter()
            .find(|&&(id, _)| id == target)
            .unwrap()
            .1;
        sim.world.get::<&mut Genome>(entity).unwrap().modes_mut()[0].color = [0., 0., 1.];
        sim.update_signals_and_secretions(1. / 30.);
        assert_eq!(state(&sim, sensor).signals[0], 0.);
    }

    #[test]
    fn cell_type_stimuli_keep_their_filter_range_and_selected_channel() {
        for (sensor, target_kind) in [
            (SENSE_LIPOCYTES, LIPOCYTE),
            (SENSE_MOTILE_CELLS, FLAGELLOCYTE),
            (SENSE_MOTILE_CELLS, CILIOCYTE),
        ] {
            for kind in [SENSEOCYTE, STEREOCYTE] {
                let mut sim = Simulation::new(13);
                let id = spawn(&mut sim, kind, 500., 500., |mode| {
                    mode.signal_channel = 7;
                    mode.sensor = sensor;
                    mode.speed = 10.; // Sensing range is 40 world units.
                });
                sim.world
                    .get::<&mut Position>(sim.entities[0].1)
                    .unwrap()
                    .angle = 0.;
                spawn(&mut sim, PHOTOCYTE, 500., 485., |_| {});
                spawn(&mut sim, target_kind, 500., 550., |_| {});
                sim.update_signals_and_secretions(1. / 30.);
                assert_eq!(state(&sim, id).signals, [0.; SIGNAL_CHANNELS]);

                spawn(&mut sim, target_kind, 500., 520., |_| {});
                sim.update_signals_and_secretions(1. / 30.);
                let signals = state(&sim, id).signals;
                assert_eq!(signals[..7], [0.; 7]);
                assert_eq!(signals[7], 0.5);
            }
        }
    }

    #[test]
    fn senseocyte_adds_emitters_and_secretion_mimics_are_distinct_stimuli() {
        for (secretion, sensor) in [(3, 1), (4, 2), (5, 0), (6, 4)] {
            let mut sim = Simulation::new(13);
            for tile in &mut sim.tiles {
                tile.light = 0.;
                tile.food = 0.;
                tile.covered_food = 0.;
            }
            let id = spawn(&mut sim, 11, 1500., 1500., |m| {
                m.sensor = sensor;
            });
            sim.update_signals_and_secretions(1. / 30.);
            let baseline = state(&sim, id).signals[0];
            spawn(&mut sim, 14, 1520., 1500., |m| {
                m.secretion = secretion;
                m.secretion_rate = 1.;
            });
            sim.update_signals_and_secretions(1. / 30.);
            let one = state(&sim, id).signals[0] - baseline;
            assert!(one > 0.7);
            spawn(&mut sim, 14, 1480., 1500., |m| {
                m.secretion = secretion;
                m.secretion_rate = 1.;
            });
            sim.update_signals_and_secretions(1. / 30.);
            let two = state(&sim, id).signals[0] - baseline;
            assert!((two - 2. * one).abs() < 0.0001);
        }
    }

    #[test]
    fn lipase_strips_covered_food_and_hcn_respects_direct_keratin_protection() {
        let mut sim = Simulation::new(14);
        let source = spawn(&mut sim, 14, 500., 500., |m| {
            m.secretion = 1;
            m.secretion_rate = 1.;
        });
        let target = spawn(&mut sim, 0, 519., 500., |_| {});
        let tile = &mut sim.tiles[tile_index(500., 500.)];
        tile.food = 0.;
        tile.covered_food = 0.5;
        tile.capacity = 1.;
        sim.update_signals_and_secretions(1.);
        let tile = sim.tiles[tile_index(500., 500.)];
        assert_eq!(tile.food, 0.25);
        assert_eq!(tile.covered_food, 0.25);
        assert!(state(&sim, target).injury > 0.);
        assert!(state(&sim, source).nitrogen < 1.);
        let shield = spawn(&mut sim, 4, 600., 500., |_| {});
        sim.bonds.insert((target, shield));
        sim.world
            .get::<&mut Genome>(sim.entities[0].1)
            .unwrap()
            .modes_mut()[0]
            .secretion = 0;
        sim.update_signals_and_secretions(1.);
        assert_eq!(state(&sim, target).toxin, 0.);
        sim.bonds.clear();
        sim.update_signals_and_secretions(1.);
        assert!(state(&sim, target).toxin > 0.);
    }

    #[test]
    fn protease_requires_contact_with_both_bond_ends() {
        let mut sim = Simulation::new(15);
        spawn(&mut sim, 14, 500., 500., |m| {
            m.secretion = 2;
            m.secretion_rate = 1.;
        });
        let a = spawn(&mut sim, 0, 490., 505., |_| {});
        let b = spawn(&mut sim, 0, 510., 505., |_| {});
        let far = spawn(&mut sim, 0, 540., 500., |_| {});
        sim.bonds.extend([(a, b), (b, far)]);
        sim.update_signals_and_secretions(1. / 30.);
        assert!(!sim.bonds.contains(&(a, b)));
        assert!(sim.bonds.contains(&(b, far)));
    }

    #[test]
    fn contact_secretion_can_set_negative_eighth_signal() {
        let mut sim = Simulation::new(16);
        spawn(&mut sim, 14, 500., 500., |m| {
            m.secretion = 22;
            m.secretion_rate = 1.;
        });
        let target = spawn(&mut sim, 0, 519., 500., |_| {});
        sim.update_signals_and_secretions(1.);
        assert_eq!(state(&sim, target).signals[7], -2.);
        assert_eq!(state(&sim, target).signals[..7], [0.; 7]);
    }

    #[test]
    fn all_eight_neural_outputs_and_contact_signal_actions_survive_save_and_continue() {
        use crate::genome::Program;
        let mut neural = Simulation::new(17);
        let neuron = spawn(&mut neural, 13, 500., 500., |mode| {
            mode.program_count = SIGNAL_CHANNELS;
            for (channel, rule) in mode.programs.iter_mut().enumerate() {
                *rule = Program {
                    target: channel as u8,
                    input: 0,
                    offset: channel as f32 * 0.2 - 0.7,
                    ..Program::default()
                };
            }
        });
        neural.update_signals_and_secretions(1. / 30.);
        for (channel, value) in state(&neural, neuron).signals.into_iter().enumerate() {
            assert_eq!(value, channel as f32 * 0.2 - 0.7);
        }

        let mut contact = Simulation::new(18);
        let target = spawn(&mut contact, 0, 500., 500., |_| {});
        for channel in 0..SIGNAL_CHANNELS {
            let angle = channel as f32 * std::f32::consts::TAU / SIGNAL_CHANNELS as f32;
            spawn(
                &mut contact,
                14,
                500. + libm::cosf(angle) * 18.,
                500. + libm::sinf(angle) * 18.,
                |mode| {
                    mode.secretion = 7 + 2 * channel as u8;
                    mode.secretion_rate = 1.;
                },
            );
        }
        contact.update_signals_and_secretions(1. / 30.);
        assert!(
            state(&contact, target)
                .signals
                .into_iter()
                .all(|value| value > 0.)
        );
        let saved = contact.export_state();
        let mut restored = Simulation::new(999);
        restored.restore_checked(&saved).unwrap();
        assert_eq!(saved, restored.export_state());
        for _ in 0..30 {
            contact.advance(1);
            restored.advance(1);
            assert_eq!(contact.export_state(), restored.export_state());
        }
    }
}
