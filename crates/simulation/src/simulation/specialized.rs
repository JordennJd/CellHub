//! Stable contact events for adhesion, sexual reproduction, and cilia.

use super::Simulation;
use super::biology::GameteContact;
use super::boundary::Boundary;
use super::level::LevelEvent;
use super::program::{
    CILIA_LEFT_TARGET, CILIA_RIGHT_TARGET, DENSITY_TARGET, DIFFERENTIATION_A_TARGET,
    DIFFERENTIATION_B_TARGET, controlled_value,
};
use crate::cell_types::{BUOYOCYTE, CILIOCYTE, GAMETE, GLUEOCYTE, LIPOCYTE, STEMOCYTE};
use crate::components::{Cell, MAX_RECOIL_MOMENTUM, MIN_DIVISION_AGE, Position, radius};
use crate::genome::{Genome, Mode};
use crate::terrain::Tile;
use hecs::Entity;
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub(super) fn corpse_food(tile: &mut Tile, cell: &Cell, kind: u8) {
    let nutrients = cell.mass.max(0.) * 0.09;
    if kind == LIPOCYTE {
        tile.covered_food = (tile.covered_food + nutrients).min(5.);
    } else {
        tile.food = (tile.food + nutrients).min(tile.capacity);
    }
}

pub(super) fn differentiate(cell: &mut Cell, genome: &Genome, connections: usize) -> bool {
    let mode = genome.mode(cell.mode);
    if mode.kind != STEMOCYTE || cell.age + 0.000001 < MIN_DIVISION_AGE {
        return false;
    }
    let target = if controlled_value(cell, mode, DIFFERENTIATION_A_TARGET, connections) > 0. {
        mode.transition_a
    } else if controlled_value(cell, mode, DIFFERENTIATION_B_TARGET, connections) > 0. {
        mode.transition_b
    } else {
        return false;
    };
    if target == cell.mode {
        return false;
    }
    cell.mode = target;
    cell.age = 0.;
    true
}

pub(super) fn buoyant_drift(
    cell: &Cell,
    mode: &Mode,
    connections: usize,
    gravity: f32,
    density: f32,
    viscosity: f32,
) -> f32 {
    if gravity == 0. {
        return 0.;
    }
    let cell_density = if mode.kind == BUOYOCYTE {
        libm::powf(
            10.,
            controlled_value(cell, mode, DENSITY_TARGET, connections).clamp(-3.3, 3.3),
        )
    } else {
        1.
    };
    let fluid_density = libm::powf(10., density);
    20. * gravity * (cell_density - fluid_density) / (cell_density + fluid_density) / viscosity
}

fn on_wall(boundary: Boundary, position: Position, mass: f32) -> bool {
    let gap = boundary.wall_distance(position.x, position.y);
    gap <= radius(mass).max(12.) + 0.5
}

#[derive(Clone)]
struct Body {
    id: u32,
    entity: Entity,
    position: Position,
    cell: Cell,
    genome: Genome,
    radius: f32,
    gamete: Option<GameteContact>,
}

impl Body {
    fn mode(&self) -> &Mode {
        self.genome.mode(self.cell.mode)
    }
}

fn hybrid(a: &Body, b: &Body) -> Genome {
    let mut modes = a.genome.offspring().cloned_modes();
    let offset = modes.len();
    for mut mode in b.genome.offspring().cloned_modes() {
        mode.child_a += offset;
        mode.child_b += offset;
        mode.transition_a += offset;
        mode.transition_b += offset;
        mode.offspring_mode += offset;
        // Malformed/legacy genomes may carry an out-of-range sentinel here.
        // Keep it out of range instead of overflowing debug builds.
        if mode.compatible_mode != usize::MAX {
            mode.compatible_mode += offset;
        }
        if mode.virus_copy_from != usize::MAX {
            mode.virus_copy_from += offset;
        }
        modes.push(mode);
    }
    // Both inherited developmental programs are reachable from the founding
    // mode, while their internal references retain their original meaning.
    modes[a.mode().offspring_mode].child_b = offset + b.mode().offspring_mode;
    Genome::from_modes(modes)
}

impl Simulation {
    pub(super) fn specialized_contacts(&mut self, step: f32) {
        let active = self
            .world
            .query::<(&Cell, &Genome)>()
            .iter()
            .any(|(_, (cell, genome))| {
                matches!(genome.mode(cell.mode).kind, GLUEOCYTE | GAMETE | CILIOCYTE)
            });
        if !active {
            return;
        }
        let bodies: Vec<_> = self
            .entities
            .iter()
            .map(|&(id, entity)| {
                let (position, cell, genome) = self
                    .world
                    .query_one_mut::<(&Position, &Cell, &Genome)>(entity)
                    .unwrap();
                Body {
                    id,
                    entity,
                    position: *position,
                    cell: cell.clone(),
                    genome: genome.clone(),
                    radius: radius(cell.mass),
                    gamete: GameteContact::new(cell, genome.mode(cell.mode), genome.len()),
                }
            })
            .collect();
        let mut protected = None;
        let mut degrees = self.bond_degrees();
        self.spatial.clear();
        for body in &bodies {
            self.spatial.insert(&[body.position; 3], body.radius + 2.);
        }
        let mut added_bonds = BTreeSet::new();
        let mut anchors = Vec::new();
        let mut consumed = BTreeSet::new();
        let mut fusions = Vec::new();
        let mut motion = BTreeMap::<u32, [f32; 3]>::new();
        let mut candidates = Vec::new();
        for (index, source) in bodies.iter().enumerate() {
            if consumed.contains(&index) {
                continue;
            }
            let mode = source.mode();
            if mode.kind == GLUEOCYTE && on_wall(self.boundary, source.position, source.cell.mass) {
                anchors.push(source.entity);
            }
            if !matches!(mode.kind, GLUEOCYTE | GAMETE | CILIOCYTE)
                || source.gamete.is_some_and(|contact| !contact.mature())
            {
                continue;
            }
            let glue_limit = mode.max_connections.unwrap_or(5).min(5);
            let degree = degrees.get(&source.id).copied().unwrap_or(0);
            if mode.kind == GLUEOCYTE && degree >= glue_limit {
                continue;
            }
            candidates.clear();
            candidates.extend(self.spatial.candidates(index));
            candidates.sort_unstable();
            let mut cilia = None;
            for &other in &candidates {
                if consumed.contains(&other) {
                    continue;
                }
                let target = &bodies[other];
                let dx = target.position.x - source.position.x;
                let dy = target.position.y - source.position.y;
                let distance = libm::hypotf(dx, dy);
                let contact = source.radius + target.radius;
                if distance > contact + 2. {
                    continue;
                }
                if mode.kind == GLUEOCYTE
                    && !protected
                        .get_or_insert_with(|| self.keratin_protection(&HashSet::new()))
                        .contains(&target.id)
                {
                    let edge = (source.id.min(target.id), source.id.max(target.id));
                    if !self.bonds.contains(&edge) && added_bonds.insert(edge) {
                        *degrees.entry(target.id).or_default() += 1;
                        let source_degree = degrees.entry(source.id).or_default();
                        *source_degree += 1;
                        if *source_degree >= glue_limit {
                            break;
                        }
                    }
                }
                if let (Some(a), Some(b)) = (source.gamete, target.gamete)
                    && b.mature()
                    && let Some(scale) = a.fusion_scale(b)
                    && distance <= contact * scale
                {
                    consumed.insert(index);
                    consumed.insert(other);
                    fusions.push((index, other));
                    break;
                }
                if mode.kind == CILIOCYTE {
                    let (sin, cos, left, right) = *cilia.get_or_insert_with(|| {
                        let (sin, cos) = libm::sincosf(source.position.angle);
                        let left = controlled_value(&source.cell, mode, CILIA_LEFT_TARGET, degree);
                        let right =
                            controlled_value(&source.cell, mode, CILIA_RIGHT_TARGET, degree);
                        (sin, cos, left, right)
                    });
                    let side = (-dx * sin + dy * cos) * if source.cell.mirrored { -1. } else { 1. };
                    let speed = if side >= 0. { left } else { right };
                    let amount = speed * step * 0.12 / self.settings.viscosity;
                    let shift = motion.entry(target.id).or_default();
                    shift[0] -= cos * amount;
                    shift[1] -= sin * amount;
                    let reaction = target.cell.mass / source.cell.mass.max(0.01);
                    let shift = motion.entry(source.id).or_default();
                    shift[0] += cos * amount * reaction.min(4.) * 0.5;
                    shift[1] += sin * amount * reaction.min(4.) * 0.5;
                    shift[2] += speed * side.signum() * step * 0.01;
                }
            }
        }
        self.bonds.extend(added_bonds);
        for entity in anchors {
            let mut cell = self.world.get::<&mut Cell>(entity).unwrap();
            cell.anchored = true;
            cell.recoil = [0.; 2];
        }
        let mut remapped = BTreeMap::new();
        for (a, b) in fusions {
            let a = &bodies[a];
            let b = &bodies[b];
            let id = self.next_id;
            self.next_id += 1;
            let genome = hybrid(a, b);
            let mut cell = Cell::new(id, a.mode().offspring_mode);
            cell.class = crate::components::CLASS_HYBRID;
            cell.owner = a.cell.owner;
            cell.mass = (a.cell.mass + b.cell.mass).min(1_000_000.);
            cell.recoil = std::array::from_fn(|axis| {
                (a.cell.recoil[axis] + b.cell.recoil[axis])
                    .clamp(-MAX_RECOIL_MOMENTUM, MAX_RECOIL_MOMENTUM)
            });
            cell.nitrogen = (a.cell.nitrogen + b.cell.nitrogen).min(1.);
            cell.telomeres = a.mode().telomeres;
            cell.toxin = (a.cell.toxin + b.cell.toxin) * 0.5;
            cell.injury = (a.cell.injury + b.cell.injury) * 0.5;
            cell.signals = std::array::from_fn(|channel| {
                (a.cell.signals[channel] + b.cell.signals[channel]) * 0.5
            });
            cell.generation = a.cell.generation.max(b.cell.generation) + 1;
            cell.mirrored = a.cell.mirrored;
            let share = b.cell.mass / cell.mass;
            let x = a.position.x + (b.position.x - a.position.x) * share;
            let y = a.position.y + (b.position.y - a.position.y) * share;
            let (x, y, _) = self.boundary.constrain(x, y, cell.mass, 12.);
            let position = Position {
                x,
                y,
                angle: a.position.angle,
            };
            let event = LevelEvent::new(self.tick, "fusion")
                .with_cell(&cell)
                .with_position(&position)
                .with_color(&genome, cell.mode)
                .with_detail(format!("parents {} + {}", a.id, b.id));
            let entity = self.world.spawn((position, cell, genome));
            self.entities.push((id, entity));
            self.births += 1;
            self.emit_level_event(event);
            remapped.insert(a.id, id);
            remapped.insert(b.id, id);
        }
        if !consumed.is_empty() {
            for &index in &consumed {
                self.world.despawn(bodies[index].entity).unwrap();
            }
            self.entities.retain(|(id, _)| !remapped.contains_key(id));
            self.bonds = self
                .bonds
                .iter()
                .filter_map(|&(a, b)| {
                    let a = remapped.get(&a).copied().unwrap_or(a);
                    let b = remapped.get(&b).copied().unwrap_or(b);
                    (a != b).then_some((a.min(b), a.max(b)))
                })
                .collect();
        }
        for (id, entity) in &self.entities {
            let Some(shift) = motion.get(id) else {
                continue;
            };
            let (position, cell) = self
                .world
                .query_one_mut::<(&mut Position, &Cell)>(*entity)
                .unwrap();
            if cell.anchored {
                continue;
            }
            (position.x, position.y, _) = self.boundary.constrain(
                position.x + shift[0].clamp(-3., 3.),
                position.y + shift[1].clamp(-3., 3.),
                cell.mass,
                12.,
            );
            position.angle += shift[2].clamp(-0.5, 0.5);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::environment::Environment;

    fn world() -> Simulation {
        let mut simulation = Simulation::new(42);
        simulation
            .configure_environment(Environment {
                light: 0.,
                food: 0.,
                radiation: 0.,
                aging: false,
                ..Environment::default()
            })
            .unwrap();
        simulation
    }

    fn mode(kind: u8) -> Mode {
        Mode {
            kind,
            can_divide: false,
            speed: 0.,
            ..Mode::default()
        }
    }

    fn spawn(sim: &mut Simulation, x: f32, y: f32, modes: Vec<Mode>) -> Entity {
        let id = sim
            .spawn_checked(x, y, 0, &Genome::from_modes(modes).values())
            .unwrap();
        sim.entities.iter().find(|&&(key, _)| key == id).unwrap().1
    }

    #[test]
    fn lipocyte_uses_less_energy_and_leaves_covered_food_only_on_natural_death() {
        let mut sim = world();
        let lipid = spawn(&mut sim, 500., 500., vec![mode(LIPOCYTE)]);
        let phago = spawn(&mut sim, 800., 500., vec![mode(1)]);
        sim.advance(120);
        assert!(
            sim.world.get::<&Cell>(lipid).unwrap().mass
                > sim.world.get::<&Cell>(phago).unwrap().mass
        );
        let tile = crate::terrain::tile_index(500., 500.);
        sim.world.get::<&mut Cell>(lipid).unwrap().age = 1500.;
        sim.settings.aging = true;
        sim.advance(1);
        assert!(sim.tiles[tile].covered_food > 0.);
        let before = sim.tiles[tile].covered_food;
        let another = spawn(&mut sim, 500., 500., vec![mode(LIPOCYTE)]);
        let id = sim.world.get::<&Cell>(another).unwrap().id;
        sim.remove_cell(id).unwrap();
        assert_eq!(sim.tiles[tile].covered_food, before);
    }

    #[test]
    fn buoyancy_uses_cell_density_and_requires_gravity() {
        let mut cell = Cell::new(1, 0);
        cell.signals[7] = 1.;
        let mut buoy = mode(BUOYOCYTE);
        buoy.density = -1.;
        assert_eq!(buoyant_drift(&cell, &buoy, 0, 0., 0., 1.), 0.);
        assert!(buoyant_drift(&cell, &buoy, 0, 1., 0., 1.) < 0.);
        buoy.density = 1.;
        assert!(buoyant_drift(&cell, &buoy, 0, 1., 0., 1.) > 0.);
    }

    #[test]
    fn glue_obeys_five_bond_limit_keratin_protection_and_wall_anchoring() {
        let mut sim = world();
        let glue = spawn(&mut sim, 500., 500., vec![mode(GLUEOCYTE)]);
        let source = sim.world.get::<&Cell>(glue).unwrap().id;
        let keratin = spawn(&mut sim, 515., 500., vec![mode(4)]);
        let protected = sim.world.get::<&Cell>(keratin).unwrap().id;
        for i in 0..8 {
            let angle = i as f32 * std::f32::consts::TAU / 8.;
            spawn(
                &mut sim,
                500. + libm::cosf(angle) * 18.,
                500. + libm::sinf(angle) * 18.,
                vec![mode(0)],
            );
        }
        sim.specialized_contacts(crate::components::STEP);
        assert_eq!(sim.bond_degrees()[&source], 5);
        assert!(
            !sim.bonds
                .contains(&(source.min(protected), source.max(protected)))
        );
        let wall = spawn(&mut sim, 12., 900., vec![mode(GLUEOCYTE)]);
        sim.specialized_contacts(crate::components::STEP);
        assert!(sim.world.get::<&Cell>(wall).unwrap().anchored);
        let original = *sim.world.get::<&Position>(wall).unwrap();
        sim.advance(30);
        assert_eq!(sim.world.get::<&Position>(wall).unwrap().x, original.x);
        assert_eq!(sim.world.get::<&Position>(wall).unwrap().y, original.y);

        let mut limited = mode(GLUEOCYTE);
        limited.max_connections = Some(2);
        let glue = spawn(&mut sim, 1500., 500., vec![limited]);
        let source = sim.world.get::<&Cell>(glue).unwrap().id;
        for (x, y) in [(1482., 500.), (1518., 500.), (1500., 518.)] {
            spawn(&mut sim, x, y, vec![mode(0)]);
        }
        sim.specialized_contacts(crate::components::STEP);
        assert_eq!(sim.bond_degrees()[&source], 2);
    }

    #[test]
    fn stem_transition_precedes_division_and_resets_age() {
        let mut sim = world();
        let mut stem = mode(STEMOCYTE);
        stem.can_divide = true;
        stem.threshold = 1.;
        stem.signal_channel = 7;
        stem.transition_a = 1;
        stem.transition_b = 2;
        stem.transition_threshold_a = 0.5;
        stem.transition_threshold_b = 0.5;
        let entity = spawn(&mut sim, 500., 500., vec![stem, mode(0), mode(1)]);
        {
            let mut cell = sim.world.get::<&mut Cell>(entity).unwrap();
            cell.signals[7] = 1.;
            cell.mass = 4.;
            cell.age = 1.;
        }
        sim.advance(1);
        let cell = sim.world.get::<&Cell>(entity).unwrap();
        assert_eq!(cell.mode, 1);
        assert_eq!(cell.age, 0.);
        assert_eq!(sim.world.len(), 1);
    }

    #[test]
    fn gametes_fuse_both_inherited_genomes_and_do_not_divide() {
        let mut sim = world();
        let mut gamete = mode(GAMETE);
        gamete.can_divide = true;
        gamete.threshold = 1.;
        gamete.offspring_mode = 1;
        gamete.telomeres = 3;
        let a = spawn(&mut sim, 500., 500., vec![gamete, mode(0)]);
        let b = spawn(&mut sim, 519., 500., vec![gamete, mode(1)]);
        for (entity, offspring_kind) in [(a, 2), (b, 4)] {
            let mut genome = sim.world.get::<&mut Genome>(entity).unwrap();
            *genome = genome
                .clone()
                .with_inherited(Genome::from_modes(vec![mode(0), mode(offspring_kind)]));
            assert_eq!(genome.mode(0).kind, GAMETE);
        }
        for entity in [a, b] {
            let mut cell = sim.world.get::<&mut Cell>(entity).unwrap();
            cell.mass = 2.;
            cell.age = 1.;
        }
        sim.advance(1);
        assert_eq!(sim.world.len(), 1);
        let mut query = sim.world.query::<(&Cell, &Genome)>();
        let (_, (cell, genome)) = query.iter().next().unwrap();
        assert_eq!(cell.mode, 1);
        assert_eq!(cell.telomeres, 3);
        assert_eq!(genome.len(), 4);
        assert_eq!(genome.mode(1).kind, 2);
        assert_eq!(genome.mode(3).kind, 4);
        assert!(genome.inherited_genome().is_none());
        assert_eq!(genome.mode(1).child_b, 3);
        assert!(cell.mass > 3.99);
        assert_eq!(cell.age, 0.);
    }

    #[test]
    fn legacy_gametes_require_matching_groups_and_minimum_age() {
        for (group_a, group_b, age_b, population) in [
            (Some(4), Some(4), 1., 1),
            (Some(4), Some(5), 1., 2),
            (Some(4), None, 1., 2),
            (None, Some(4), 1., 2),
            (Some(4), Some(4), 0.4, 2),
        ] {
            let mut sim = world();
            let mut gamete = mode(GAMETE);
            gamete.offspring_mode = 1;
            gamete.legacy_gamete_group = group_a;
            let a = spawn(&mut sim, 500., 500., vec![gamete, mode(0)]);
            gamete.legacy_gamete_group = group_b;
            let b = spawn(&mut sim, 512., 500., vec![gamete, mode(1)]);
            sim.world.get::<&mut Cell>(a).unwrap().age = 1.;
            sim.world.get::<&mut Cell>(b).unwrap().age = age_b;
            sim.specialized_contacts(crate::components::STEP);
            assert_eq!(
                sim.world.len(),
                population,
                "groups {group_a:?}, {group_b:?}, age {age_b}"
            );
        }
    }

    #[test]
    fn finite_telomeres_decrement_and_end_the_lineage() {
        let mut sim = world();
        let mut photo = mode(0);
        photo.can_divide = true;
        photo.threshold = 1.;
        let entity = spawn(&mut sim, 500., 500., vec![photo]);
        {
            let mut cell = sim.world.get::<&mut Cell>(entity).unwrap();
            cell.telomeres = 1;
            cell.age = 1.;
            cell.mass = 3.;
        }
        sim.advance(1);
        assert_eq!(sim.world.len(), 2);
        assert!(
            sim.world
                .query::<&Cell>()
                .iter()
                .all(|(_, cell)| cell.telomeres == 0)
        );
        sim.advance(1);
        assert_eq!(sim.world.len(), 0);
    }

    #[test]
    fn cilia_push_contacting_cells_on_the_programmed_side() {
        let mut sim = world();
        let mut cilia = mode(CILIOCYTE);
        cilia.cilia_left = 60.;
        cilia.cilia_right = -60.;
        let source = spawn(&mut sim, 500., 500., vec![cilia]);
        sim.world.get::<&mut Position>(source).unwrap().angle = 0.;
        let left = spawn(&mut sim, 500., 519., vec![mode(0)]);
        let right = spawn(&mut sim, 500., 481., vec![mode(0)]);
        sim.specialized_contacts(0.1);
        assert!(sim.world.get::<&Position>(left).unwrap().x < 500.);
        assert!(sim.world.get::<&Position>(right).unwrap().x > 500.);
        assert!(sim.world.get::<&Position>(source).unwrap().angle > 0.);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn stress_world() -> Simulation {
        use crate::genome::Program;
        use crate::simulation::program::{CONTRACTION_TARGET, SECRETION_RATE_TARGET};

        let mut sim = Simulation::new(73421);
        sim.set_circular_dish(true);
        sim.configure_environment(Environment {
            light: 2.,
            food: 2.,
            radiation: 0.25,
            gravity: 0.1,
            density: 0.2,
            viscosity: 1.2,
            aging: false,
            ..Environment::default()
        })
        .unwrap();
        for colony in 0..36 {
            let cx = 600. + (colony % 6) as f32 * 300.;
            let cy = 600. + (colony / 6) as f32 * 300.;
            let mut ids = Vec::new();
            for item in 0..32 {
                let kind = if item < 18 {
                    item
                } else if item == 18 {
                    16
                } else {
                    (item * 5) % 18
                } as u8;
                let mut active = mode(kind);
                active.speed = if kind == 2 { 6. } else { 0. };
                active.density = if colony % 2 == 0 { -0.7 } else { 0.7 };
                active.sensor = if kind == 10 { 3 } else { 0 };
                active.signal_channel = 7;
                active.sense_tolerance = 1.;
                active.cilia_left = 8.;
                active.cilia_right = -4.;
                active.contraction = 0.25;
                active.bending = 0.15;
                active.lift = 0.1;
                active.secretion = (colony % 7) as u8;
                active.secretion_rate = 0.15;
                if kind == 13 {
                    active.program_count = 8;
                    for channel in 0..8 {
                        active.programs[channel] = Program {
                            target: channel as u8,
                            input: 0,
                            offset: 0.2 + channel as f32 * 0.1,
                            ..Program::default()
                        };
                    }
                    active.programs[7] = Program {
                        target: 7,
                        input: 10,
                        kind: 1,
                        scale: 0.1,
                        offset: 1.,
                        threshold: 1.,
                        below: -0.25,
                    };
                }
                if kind == 12 || kind == 14 {
                    active.program_count = 1;
                    active.programs[0] = Program {
                        target: if kind == 12 {
                            CONTRACTION_TARGET
                        } else {
                            SECRETION_RATE_TARGET
                        } as u8,
                        input: 8,
                        scale: 0.1,
                        offset: 0.15,
                        ..Program::default()
                    };
                }
                let modes = if kind == STEMOCYTE {
                    active.transition_a = 1;
                    active.transition_b = 1;
                    active.transition_threshold_a = 0.05;
                    active.transition_threshold_b = 0.1;
                    vec![active, mode(0)]
                } else if kind == GAMETE {
                    active.offspring_mode = 1;
                    active.telomeres = 3;
                    let mut offspring = mode(if item == 18 { 1 } else { 0 });
                    offspring.can_divide = true;
                    offspring.child_a = 1;
                    offspring.child_b = 1;
                    offspring.threshold = 1.4;
                    vec![active, offspring]
                } else {
                    vec![active]
                };
                let (x, y) = if kind == GAMETE {
                    (cx + if item == 18 { 127. } else { 110. }, cy + 50.)
                } else {
                    (cx + (item % 6) as f32 * 17., cy + (item / 6) as f32 * 17.)
                };
                let entity = spawn(&mut sim, x, y, modes);
                let mut cell = sim.world.get::<&mut Cell>(entity).unwrap();
                cell.signals = std::array::from_fn(|channel| {
                    if item % 2 == 0 {
                        channel as f32 * 0.03
                    } else {
                        -(channel as f32) * 0.02
                    }
                });
                if kind == GAMETE || kind == STEMOCYTE {
                    cell.age = 1.;
                    cell.signals[7] = 1.;
                }
                if item == 7 {
                    cell.injury = 2.;
                }
                ids.push(cell.id);
            }
            for (a, b) in [
                (13, 12),
                (13, 15),
                (12, 0),
                (9, 1),
                (7, 8),
                (8, 11),
                (4, 15),
            ] {
                sim.bonds.insert((ids[a].min(ids[b]), ids[a].max(ids[b])));
            }
        }
        sim
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn all_types_and_eight_signal_programs_match_on_one_and_four_threads_after_restore() {
        use std::sync::mpsc::sync_channel;

        std::thread::scope(|scope| {
            let mut receivers = Vec::new();
            let mut workers = Vec::new();
            for threads in [1, 4] {
                let (sender, receiver) = sync_channel(1);
                receivers.push(receiver);
                workers.push(scope.spawn(move || {
                    let pool = rayon::ThreadPoolBuilder::new()
                        .num_threads(threads)
                        .build()
                        .unwrap();
                    pool.install(|| {
                        let mut sim = stress_world();
                        assert_eq!(sim.world.len(), 1152);
                        let kinds: BTreeSet<_> = sim
                            .world
                            .query::<(&Cell, &Genome)>()
                            .iter()
                            .map(|(_, (cell, genome))| genome.mode(cell.mode).kind)
                            .collect();
                        assert_eq!(kinds, (0..18).collect());
                        let initial_bonds = sim.bonds.len();
                        let mut witnessed_fusion = false;
                        let mut witnessed_covered_food = false;
                        let mut witnessed_diffusion = false;
                        let mut witnessed_gluing = false;
                        for tick in 0..=240 {
                            if tick == 120 && threads == 4 {
                                let checkpoint = sim.encode_state();
                                let mut loaded = Simulation::new(3);
                                loaded.restore_checked(&checkpoint).unwrap();
                                assert_eq!(loaded.encode_state(), checkpoint);
                                sim = loaded;
                            }
                            if tick > 0 {
                                witnessed_gluing |= sim.bonds.len() > initial_bonds;
                                witnessed_fusion |=
                                    sim.world.query::<(&Cell, &Genome)>().iter().any(
                                        |(_, (cell, genome))| {
                                            genome.len() == 4 && cell.telomeres >= 0
                                        },
                                    );
                                witnessed_covered_food |=
                                    sim.tiles.iter().any(|tile| tile.covered_food > 0.);
                                witnessed_diffusion |= sim
                                    .world
                                    .query::<(&Cell, &Genome)>()
                                    .iter()
                                    .any(|(_, (cell, genome))| {
                                        genome.mode(cell.mode).kind == 12 && cell.signals[7] != 0.
                                    });
                            }
                            if sender.send((tick, sim.encode_state())).is_err() {
                                return;
                            }
                            if tick == 60 || tick == 180 {
                                let mut settings = sim.settings;
                                settings.light = 1.2;
                                settings.food = 3.;
                                settings.radiation = 0.35;
                                settings.nitrogen = 0.8;
                                sim.configure_environment(settings).unwrap();
                            }
                            if tick < 240 {
                                sim.advance(1);
                            }
                        }
                        assert!(witnessed_fusion);
                        assert!(witnessed_covered_food);
                        assert!(witnessed_diffusion);
                        assert!(witnessed_gluing);
                        assert!(sim.world.len() >= 700);
                    });
                }));
            }
            for tick in 0..=240 {
                let (one_tick, one) = receivers[0].recv().unwrap();
                let (four_tick, four) = receivers[1].recv().unwrap();
                assert_eq!((one_tick, four_tick), (tick, tick));
                assert_eq!(
                    one.len(),
                    four.len(),
                    "different checkpoint length at tick {tick}"
                );
                let first = one
                    .iter()
                    .zip(&four)
                    .position(|(a, b)| a.to_bits() != b.to_bits());
                assert_eq!(first, None, "first differing field at tick {tick}");
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
    }
}
