use super::Simulation;

use crate::components::{
    Cell, INITIAL_NITROGEN, LIMIT, MIN_DIVISION_AGE, Position, SIZE, SNAPSHOT_STRIDE,
};
use crate::genome::{Genome, KIND_COUNT, MODE_STRIDE, NITROCYTE_KIND, decode_checked};
use crate::terrain::tile_index;
use hecs::Entity;
use std::collections::{BTreeMap, BTreeSet, HashSet};

fn stationary_genes(kind: u8) -> Vec<f32> {
    let mut genes = modern_genes(1);
    genes[0] = kind as f32;
    genes[8] = 0.;
    genes
}

#[test]
fn only_phagocytes_consume_substrate_food() {
    for kind in 0..KIND_COUNT {
        let mut s = quiet_world();
        s.configure_environment(super::environment::Environment {
            light: 0.,
            food: 2.,
            radiation: 0.,
            ..Default::default()
        })
        .unwrap();
        let mut genes = decode_checked(&stationary_genes(0)).unwrap().values();
        genes[2] = f32::from(kind);
        s.spawn(2000., 1300., 0, &genes).unwrap();
        let tile = tile_index(2000., 1300.);
        let food = s.tiles[tile].food;
        s.advance(30);
        if kind == 1 {
            assert!(s.tiles[tile].food < food);
            assert!(s.snapshot()[5] > 1.);
        } else {
            assert_eq!(s.tiles[tile].food, food, "kind={kind}");
            assert!(s.snapshot()[5] < 1., "kind={kind}");
        }
    }
}

#[test]
fn bond_energy_is_synchronous_conservative_and_independent_of_entity_order() {
    let make = |reverse: bool, masses: &[f32], edges: &[(u32, u32)]| {
        let mut s = quiet_world();
        let mut indices: Vec<_> = (0..masses.len()).collect();
        if reverse {
            indices.reverse();
        }
        for i in indices {
            let id = s.spawn(500., 500., 0, &stationary_genes(2)).unwrap();
            let entity = entity(&s, id);
            let mut cell = s.world.get::<&mut Cell>(entity).unwrap();
            // Stable IDs deliberately differ from ECS insertion order.
            cell.id = (i + 1) as u32 + 100;
            cell.mass = masses[i];
        }
        s.bonds = edges.iter().map(|&(a, b)| (a + 100, b + 100)).collect();
        s.exchange_bond_energy(0.5, &s.bond_degrees());
        s.world
            .query::<&Cell>()
            .iter()
            .map(|(_, c)| (c.id, c.mass))
            .collect::<BTreeMap<_, _>>()
    };
    let edges = [(1, 2), (2, 3)];
    let forward = make(false, &[4., 1., 1.], &edges);
    assert_eq!(forward, make(true, &[4., 1., 1.], &edges));
    assert_eq!(
        forward[&103], 1.,
        "energy cannot traverse two edges in one exchange"
    );
    assert!((forward.values().sum::<f32>() - 6.).abs() < 0.000001);
    let mut masses = vec![0.001; 101];
    masses[0] = 1.;
    let star: Vec<_> = (2..=101).map(|id| (1, id)).collect();
    let shared = make(false, &masses, &star);
    assert!(shared.values().all(|mass| *mass > 0.));
    assert!(
        (shared.values().map(|v| *v as f64).sum::<f64>()
            - masses.iter().map(|v| *v as f64).sum::<f64>())
        .abs()
            < 0.000001
    );
}

#[test]
fn bond_energy_respects_base_and_starvation_priorities() {
    let mass_after_exchange = |mass, priority, emergency| {
        let mut s = quiet_world();
        let donor = s.spawn(500., 500., 0, &stationary_genes(0)).unwrap();
        let mut genes = decode_checked(&stationary_genes(0)).unwrap().values();
        genes[22] = priority;
        genes[23] = emergency;
        let receiver = s.spawn(522., 500., 0, &genes).unwrap();
        let target = entity(&s, receiver);
        s.world.get::<&mut Cell>(target).unwrap().mass = mass;
        s.bonds.insert((donor, receiver));
        s.exchange_bond_energy(0.5, &s.bond_degrees());
        let result = s.world.get::<&Cell>(target).unwrap().mass;
        let remainder = s.world.get::<&Cell>(entity(&s, donor)).unwrap().mass;
        assert!((result + remainder - (1. + mass)).abs() < 0.000001);
        result
    };
    assert!(mass_after_exchange(1., 2., 0.) > mass_after_exchange(1., 1., 0.));
    assert!(mass_after_exchange(0.5, 1., 1.) > mass_after_exchange(0.5, 1., 0.));
    assert_eq!(
        mass_after_exchange(0.9, 1., 1.),
        mass_after_exchange(0.9, 1., 0.)
    );
}

#[test]
fn nitrogen_uptake_and_nitrocyte_supply_follow_environment_and_bonds() {
    let mut s = quiet_world();
    s.configure_environment(super::environment::Environment {
        radiation: 0.,
        nitrogen: 0.,
        ..Default::default()
    })
    .unwrap();
    let mut genes = decode_checked(&stationary_genes(0)).unwrap().values();
    genes[2] = f32::from(NITROCYTE_KIND);
    let source = s.spawn(500., 500., 0, &genes).unwrap();
    let recipients: Vec<_> = [522., 544., 566.]
        .into_iter()
        .map(|x| s.spawn(x, 500., 0, &stationary_genes(0)).unwrap())
        .collect();
    for (_, cell) in s.world.query_mut::<&mut Cell>() {
        cell.nitrogen = 0.5;
    }
    s.bonds
        .extend([(source, recipients[0]), (recipients[0], recipients[1])]);
    s.advance(120);
    let amount = |s: &Simulation, id| s.world.get::<&Cell>(entity(s, id)).unwrap().nitrogen;
    assert!(amount(&s, source) > 0.5);
    assert!(amount(&s, recipients[1]) > amount(&s, recipients[2]) + 0.05);
    let unfed = amount(&s, recipients[2]);
    let patch = tile_index(566., 500.);
    assert_eq!(s.tiles[patch].nitrates, 0.);
    s.configure_environment(super::environment::Environment {
        radiation: 0.,
        ..Default::default()
    })
    .unwrap();
    let nitrates = s.tiles[patch].nitrates;
    s.advance(30);
    assert!(amount(&s, recipients[2]) > unfed);
    assert!(s.tiles[patch].nitrates < nitrates);
}

#[test]
fn nitrogen_deficit_blocks_division_or_kills_and_daughters_split_reserve_evenly() {
    let run = |reserve| {
        let mut s = quiet_world();
        s.configure_environment(super::environment::Environment {
            radiation: 0.,
            nitrogen: 0.,
            ..Default::default()
        })
        .unwrap();
        let mut genes = modern_genes(1);
        genes[1] = 1.;
        genes[4] = 0.25;
        let id = s.spawn(500., 500., 0, &genes).unwrap();
        s.world.get::<&mut Cell>(entity(&s, id)).unwrap().nitrogen = reserve;
        mature_all(&mut s);
        s.advance(1);
        s.world
            .query::<&Cell>()
            .iter()
            .map(|(_, c)| (c.mass, c.nitrogen))
            .collect::<Vec<_>>()
    };
    assert!(run(0.09).is_empty());
    assert_eq!(run(0.15).len(), 1);
    let daughters = run(0.8);
    assert_eq!(daughters.len(), 2);
    assert_ne!(daughters[0].0, daughters[1].0);
    assert_eq!(daughters[0].1, daughters[1].1);
    assert!((daughters[0].1 - 0.4).abs() < 0.001);
}

#[test]
fn connection_limit_counts_only_permanent_bonds_and_preserves_them() {
    for (limit, bonded, divides) in [
        (-1., true, true),
        (0., false, false),
        (1., true, false),
        (1., false, true),
        (2., true, true),
    ] {
        let mut s = quiet_world();
        let mut genes = decode_checked(&modern_genes(1)).unwrap().values();
        genes[3] = 1.;
        genes[21] = limit;
        let parent = s.spawn(500., 500., 0, &genes).unwrap();
        let neighbor = s.spawn(518., 500., 0, &stationary_genes(0)).unwrap();
        if bonded {
            s.bonds.insert((parent, neighbor));
        }
        mature_all(&mut s);
        s.advance(1);
        assert_eq!(s.world.len(), if divides { 3 } else { 2 });
        if !divides && bonded {
            assert_eq!(s.links(), [parent, neighbor]);
        }
    }
}

#[test]
fn bonded_photocyte_sustains_a_flagellocyte_that_starves_alone() {
    let mut organism = quiet_world();
    let mut isolated = quiet_world();
    let photo = organism.spawn(500., 500., 0, &stationary_genes(0)).unwrap();
    let flag = organism.spawn(522., 500., 0, &stationary_genes(2)).unwrap();
    organism.bonds.insert((photo, flag));
    isolated.spawn(522., 500., 0, &stationary_genes(2)).unwrap();
    for _ in 0..15 {
        organism.advance(120);
        isolated.advance(120);
    }
    assert_eq!(organism.world.len(), 2);
    assert_eq!(isolated.world.len(), 0);
    assert!(
        organism
            .world
            .get::<&Cell>(entity(&organism, flag))
            .unwrap()
            .mass
            > 1.
    );
    assert_eq!(organism.bonds.len(), 1);
}

#[test]
fn keratin_protection_is_direct_and_requires_a_living_shield() {
    let mut s = quiet_world();
    let shield = s.spawn(500., 500., 0, &stationary_genes(4)).unwrap();
    let direct = s.spawn(522., 500., 0, &stationary_genes(0)).unwrap();
    let indirect = s.spawn(544., 500., 0, &stationary_genes(0)).unwrap();
    s.bonds.extend([(shield, direct), (direct, indirect)]);
    let protected = s.keratin_protection(&HashSet::new());
    assert_eq!(protected, HashSet::from([shield, direct]));
    assert!(
        s.keratin_protection(&HashSet::from([entity(&s, shield)]))
            .is_empty()
    );
}

#[test]
fn keratin_blocks_predation_and_infection_for_itself_and_bonded_neighbors() {
    let run = |attack_kind: u8, target_kind: u8, shielded: bool| {
        let mut s = quiet_world();
        s.configure_environment(super::environment::Environment {
            light: 0.,
            radiation: 0.,
            ..Default::default()
        })
        .unwrap();
        let target = s
            .spawn(500., 500., 0, &stationary_genes(target_kind))
            .unwrap();
        s.spawn(480., 500., 0, &stationary_genes(attack_kind))
            .unwrap();
        if shielded {
            let shield = s.spawn(540., 500., 0, &stationary_genes(4)).unwrap();
            s.bonds.insert((target, shield));
        }
        s.advance(1);
        let cell = s.world.get::<&Cell>(entity(&s, target)).unwrap();
        (cell.mass, s.infections)
    };
    let unprotected = run(5, 0, false);
    let protected = run(5, 0, true);
    assert!(protected.0 > unprotected.0 + 0.002);
    assert!((protected.0 - (1. - 0.025 / 30.)).abs() < 0.000001);
    assert!((run(5, 4, false).0 - (1. - 0.014 / 30.)).abs() < 0.000001);
    assert_eq!(run(3, 0, false).1, 1);
    assert_eq!(run(3, 0, true).1, 0);
    assert_eq!(run(3, 4, false).1, 0);
    // Two predators cannot feed each other in an energy-generating loop.
    assert!((run(5, 5, false).0 - (1. - 0.025 / 30.)).abs() < 0.000001);
}

#[test]
fn minimum_age_prevents_instant_division_after_boost() {
    let mut s = quiet_world();
    let mut genes = modern_genes(1);
    genes[1] = 1.;
    let id = s.spawn(500., 500., 0, &genes).unwrap();
    s.boost_cell(id).unwrap();
    s.advance(14);
    assert_eq!(s.world.len(), 1);
    s.advance(1);
    assert_eq!(s.world.len(), 2);
    assert!(s.world.query::<&Cell>().iter().all(|(_, c)| c.age == 0.));
}

#[test]
fn daughter_orientation_mirroring_and_rgb_survive_generations_and_save() {
    let mut s = quiet_world();
    let mut genes = crate::genome::decode_checked(&modern_genes(1))
        .unwrap()
        .values();
    genes[2 + 1] = 1.;
    genes[2 + 6] = 0.2;
    genes[2 + 12] = 0.3;
    genes[2 + 13] = -0.4;
    genes[2 + 14] = 1.;
    genes[2 + 16..2 + 19].copy_from_slice(&[0.2, 0.6, 1.]);
    let parent = s.spawn(500., 500., 0, &genes).unwrap();
    mature_all(&mut s);
    s.world
        .get::<&mut Position>(entity(&s, parent))
        .unwrap()
        .angle = 0.;
    let rng = s.rng;
    let noise = s.random();
    s.rng = rng;
    s.advance(1);
    let rows = s.snapshot();
    assert_eq!(rows.len(), 24);
    let split_axis = (noise - 0.5) * 0.35 + 0.2;
    assert!((rows[9] - (split_axis - 0.3)).abs() < 0.00001);
    assert!((rows[21] - (split_axis + std::f32::consts::PI - 0.4)).abs() < 0.00001);
    assert_eq!(rows[11], 1.);
    assert_eq!(rows[23], 0.);
    assert_eq!(rows[10], 0x3399ff as f32);
    assert_eq!(rows[22], rows[10]);
    let state = s.encode_state();
    let mut restored = quiet_world();
    restored.restore_checked(&state).unwrap();
    assert_eq!(restored.snapshot(), rows);
    assert_eq!(restored.encode_state(), state);
    // Divide the mirrored daughter. Its axis reverses and its first child toggles back.
    s.remove_cell(3).unwrap();
    s.boost_cell(2).unwrap();
    mature_all(&mut s);
    s.world.get::<&mut Position>(entity(&s, 2)).unwrap().angle = 0.;
    let rng = s.rng;
    let noise = s.random();
    s.rng = rng;
    s.advance(1);
    let rows = s.snapshot();
    let mirrored_axis = (noise - 0.5) * 0.35 - 0.2;
    assert_eq!(rows[11], 0.);
    assert_eq!(rows[23], 1.);
    assert!((rows[9] - (mirrored_axis + 0.3)).abs() < 0.00001);
    assert!((rows[21] - (mirrored_axis + std::f32::consts::PI + 0.4)).abs() < 0.00001);
}

#[test]
fn version_one_world_migrates_without_changing_living_cells() {
    let mut original = quiet_world();
    let genes = modern_genes(2);
    original.spawn(500., 500., 1, &genes).unwrap();
    original.advance(10);
    for tile in &mut original.tiles {
        tile.nitrates = tile.nitrate_capacity;
    }
    let legacy = legacy_checkpoint(&original, 1);
    let mut restored = Simulation::new(123);
    restored.restore_checked(&legacy).unwrap();
    assert_eq!(restored.encode_state(), original.encode_state());
    original.advance(120);
    restored.advance(120);
    assert_eq!(restored.encode_state(), original.encode_state());
}

#[test]
fn version_two_world_retains_mirroring_with_default_environment_extensions() {
    let mut original = quiet_world();
    let id = original.spawn(500., 500., 0, &modern_genes(1)).unwrap();
    original
        .world
        .get::<&mut Cell>(entity(&original, id))
        .unwrap()
        .mirrored = true;
    let legacy = legacy_checkpoint(&original, 2);
    let mut restored = Simulation::new(123);
    restored.restore_checked(&legacy).unwrap();
    assert_eq!(restored.encode_state(), original.encode_state());
    assert_eq!(restored.snapshot()[11], 1.);
}

fn legacy_checkpoint(simulation: &Simulation, version: u32) -> Vec<f64> {
    assert!((1..=4).contains(&version));
    let settings = simulation.settings;
    let mut legacy = vec![
        f64::from(version),
        simulation.seed as f64,
        simulation.rng as f64,
        simulation.tick as f64,
        simulation.next_id as f64,
        simulation.births as f64,
        simulation.deaths as f64,
        simulation.infections as f64,
        settings.light as f64,
        settings.food as f64,
        settings.radiation as f64,
        settings.viscosity as f64,
        settings.aging as u8 as f64,
        settings.kill_at_edge as u8 as f64,
    ];
    if version >= 3 {
        legacy.extend([
            settings.only_point_mutations as u8 as f64,
            settings.gravity as f64,
            settings.density as f64,
        ]);
    }
    if version >= 4 {
        legacy.extend([
            settings.nitrogen as f64,
            matches!(simulation.boundary, super::boundary::Boundary::Circle) as u8 as f64,
        ]);
    }
    legacy.push(simulation.tiles.len() as f64);
    for tile in &simulation.tiles {
        legacy.extend(
            [
                tile.light,
                tile.radiation,
                tile.food,
                tile.capacity,
                tile.biome,
            ]
            .map(f64::from),
        );
        if version >= 4 {
            legacy.extend([tile.nitrates, tile.nitrate_capacity].map(f64::from));
        }
    }
    let mut query = simulation.world.query::<(&Position, &Cell, &Genome)>();
    let mut cells: Vec<_> = query.iter().map(|(_, values)| values).collect();
    cells.sort_by_key(|(_, cell, _)| cell.id);
    legacy.push(cells.len() as f64);
    for (position, cell, genome) in cells {
        legacy.extend([
            cell.id as f64,
            position.x as f64,
            position.y as f64,
            position.angle as f64,
            cell.mode as f64,
            cell.mass as f64,
            cell.age as f64,
            cell.generation as f64,
            cell.mutations as f64,
            cell.cooldown as f64,
        ]);
        if version >= 2 {
            legacy.push(cell.mirrored as u8 as f64);
        }
        if version >= 4 {
            legacy.push(cell.nitrogen as f64);
        }
        let genes = match version {
            1 => genome.legacy_values(),
            2 | 3 => genome.values_v3(),
            _ => genome.values_v4(),
        };
        legacy.push(genes.len() as f64);
        legacy.extend(genes.into_iter().map(f64::from));
    }
    legacy.push(simulation.bonds.len() as f64);
    for &(a, b) in &simulation.bonds {
        legacy.extend([a as f64, b as f64]);
    }
    legacy
}

#[test]
fn version_four_world_preserves_nitrogen_boundary_and_legacy_modes() {
    let mut original = quiet_world();
    original.set_circular_dish(true);
    let mut genes = decode_checked(&modern_genes(2)).unwrap().values();
    genes[2] = f32::from(NITROCYTE_KIND);
    let a = original.spawn(500., 800., 0, &genes).unwrap();
    let b = original.spawn(522., 800., 1, &genes).unwrap();
    original.bonds.insert((a, b));
    {
        let mut cell = original
            .world
            .get::<&mut Cell>(entity(&original, a))
            .unwrap();
        cell.mirrored = true;
        cell.nitrogen = 0.65;
    }
    original.tiles[7].nitrates *= 0.5;
    let mut restored = Simulation::new(999);
    restored
        .restore_checked(&legacy_checkpoint(&original, 4))
        .unwrap();
    assert_eq!(restored.encode_state(), original.encode_state());
    original.advance(120);
    restored.advance(120);
    assert_eq!(restored.encode_state(), original.encode_state());
}

#[test]
fn gravity_density_and_viscosity_control_buoyant_motion() {
    let position = |gravity, density, viscosity| {
        let mut s = quiet_world();
        s.configure_environment(super::environment::Environment {
            gravity,
            density,
            viscosity,
            radiation: 0.,
            ..Default::default()
        })
        .unwrap();
        s.spawn(500., 500., 0, &modern_genes(1)).unwrap();
        s.advance(30);
        s.snapshot()[2]
    };
    let sunk = position(1., -1., 1.) - 500.;
    let risen = position(1., 1., 1.) - 500.;
    assert!(sunk > 15.);
    assert!(risen < -15.);
    assert!((sunk + risen).abs() < 0.001);
    assert!(((position(1., -1., 2.) - 500.) * 2. - sunk).abs() < 0.001);
    assert_eq!(position(0., -1., 1.), 500.);
    assert_eq!(position(1., 0., 1.), 500.);
}

#[test]
fn point_only_mutation_changes_values_without_changing_cell_types() {
    let mut point = quiet_world();
    let mut full = quiet_world();
    point.settings.only_point_mutations = true;
    let mut genes = modern_genes(1);
    genes[0] = 4.;
    genes[1] = 4.;
    genes[8] = 0.;
    for s in [&mut point, &mut full] {
        for tile in &mut s.tiles {
            tile.radiation = 5.;
        }
        for i in 0..20 {
            let id = s.spawn(300. + i as f32 * 30., 500., 0, &genes).unwrap();
            s.world.get::<&mut Cell>(entity(s, id)).unwrap().mass = 10.;
        }
        for _ in 0..5 {
            s.advance(120);
        }
    }
    assert!(
        point
            .world
            .query::<&Cell>()
            .iter()
            .any(|(_, c)| c.mutations > 0)
    );
    assert!(
        point
            .world
            .query::<&crate::genome::Genome>()
            .iter()
            .all(|(_, g)| g.mode(0).kind == 4)
    );
    assert!(
        point
            .world
            .query::<&crate::genome::Genome>()
            .iter()
            .any(|(_, g)| g.mode(0).threshold != 4.)
    );
    assert!(
        full.world
            .query::<&crate::genome::Genome>()
            .iter()
            .any(|(_, g)| g.mode(0).kind != 4)
    );
}

#[test]
fn version_four_genomes_reject_malformed_extra_fields() {
    let genes = crate::genome::decode_checked(&modern_genes(1))
        .unwrap()
        .values();
    for (index, value) in [
        (1, 2.),
        (2 + 12, 4.),
        (2 + 14, 0.5),
        (2 + 16, 0.5),
        (2 + 19, -2.),
        (2 + 19, 0.5),
        (2 + 20, 0.),
        (2 + 21, 0.5),
    ] {
        let mut invalid = genes.clone();
        invalid[index] = value;
        assert!(
            crate::genome::decode_checked(&invalid).is_err(),
            "index={index}"
        );
    }
    assert_eq!(
        crate::genome::decode_checked(&genes).unwrap().values(),
        genes
    );
}

#[test]
fn world_save_restore_preserves_future_evolution() {
    let mut original = quiet_world();
    original
        .configure_environment(super::environment::Environment {
            light: 1.4,
            food: 0.7,
            radiation: 0.2,
            viscosity: 1.5,
            aging: false,
            kill_at_edge: true,
            only_point_mutations: true,
            gravity: 0.1,
            density: -0.5,
            nitrogen: 1.5,
        })
        .unwrap();
    let mut genes = modern_genes(3);
    genes[1] = 1.;
    genes[2] = 1.;
    genes[3] = 2.;
    genes[9] = 1.;
    genes[12] = 2.;
    genes[12 + 8] = 0.;
    original.spawn(400., 500., 0, &genes).unwrap();
    original.spawn(1800., 500., 0, &modern_genes(1)).unwrap();
    original.advance(40);
    assert!(!original.bonds.is_empty());
    let state = original.encode_state();
    let mut restored = Simulation::new(999);
    restored.restore_checked(&state).unwrap();
    assert_eq!(restored.encode_state(), state);
    for _ in 0..10 {
        original.advance(120);
        restored.advance(120);
        assert_eq!(original.encode_state(), restored.encode_state());
    }
    assert_eq!(
        original.spawn(100., 200., 0, &modern_genes(1)).unwrap(),
        restored.spawn(100., 200., 0, &modern_genes(1)).unwrap()
    );
    assert_eq!(original.snapshot(), restored.snapshot());
}

#[test]
fn corrupt_world_restore_is_atomic() {
    let mut s = quiet_world();
    s.spawn(200., 300., 0, &modern_genes(1)).unwrap();
    let state = s.encode_state();
    let mut variants = vec![vec![], state[..state.len() - 1].to_vec()];
    for (index, value) in [
        (0, 7.),
        (2, 0.),
        (4, 1.),
        (8, f64::NAN),
        (11, 0.),
        (12, 0.5),
        (14, 0.5),
        (15, 2.),
        (16, -4.),
        (17, -1.),
        (18, 2.),
        (19, 0.),
    ] {
        let mut corrupt = state.clone();
        corrupt[index] = value;
        variants.push(corrupt);
    }
    let mut extra = state.clone();
    extra.push(0.);
    variants.push(extra);
    for corrupt in variants {
        assert!(s.restore_checked(&corrupt).is_err());
        assert_eq!(s.encode_state(), state);
    }
    // A connection to a nonexistent cell must not be accepted.
    let mut orphan = state.clone();
    *orphan.last_mut().unwrap() = 1.;
    orphan.extend([1., 999.]);
    assert!(s.restore_checked(&orphan).is_err());
    assert_eq!(s.encode_state(), state);
}

#[test]
fn substrate_changes_preserve_depletion_and_affect_growth() {
    let mut fed = quiet_world();
    let mut dark = quiet_world();
    fed.spawn(500., 500., 0, &modern_genes(1)).unwrap();
    dark.spawn(500., 500., 0, &modern_genes(1)).unwrap();
    let at = tile_index(500., 500.);
    fed.tiles[at].food = fed.tiles[at].capacity * 0.25;
    fed.configure_environment(super::environment::Environment {
        food: 2.,
        ..Default::default()
    })
    .unwrap();
    assert!((fed.tiles[at].food / fed.tiles[at].capacity - 0.25).abs() < 0.0001);
    dark.configure_environment(super::environment::Environment {
        light: 0.,
        radiation: 0.,
        ..Default::default()
    })
    .unwrap();
    fed.advance(120);
    dark.advance(120);
    assert!(fed.snapshot()[5] > dark.snapshot()[5]);
    let before = fed.encode_state();
    assert!(
        fed.configure_environment(super::environment::Environment {
            light: f32::NAN,
            ..Default::default()
        })
        .is_err()
    );
    assert_eq!(before, fed.encode_state());
}

#[test]
fn tools_and_sterilization_do_not_reset_environment_or_time() {
    let mut s = quiet_world();
    let a = s.spawn(400., 500., 0, &modern_genes(1)).unwrap();
    let b = s.spawn(422., 500., 0, &modern_genes(1)).unwrap();
    s.bonds.insert((a, b));
    s.advance(10);
    let mass = s.world.get::<&Cell>(entity(&s, a)).unwrap().mass;
    s.world.get::<&mut Cell>(entity(&s, a)).unwrap().nitrogen = 0.2;
    s.boost_cell(a).unwrap();
    assert_eq!(s.world.get::<&Cell>(entity(&s, a)).unwrap().mass, mass + 1.);
    assert_eq!(
        s.world.get::<&Cell>(entity(&s, a)).unwrap().nitrogen,
        INITIAL_NITROGEN
    );
    s.reposition_cell(a, 700., 800.).unwrap();
    assert_eq!(s.world.get::<&Position>(entity(&s, a)).unwrap().x, 700.);
    s.remove_cell(a).unwrap();
    assert!(s.bonds.is_empty());
    assert!(s.remove_cell(a).is_err());
    assert_eq!(s.world.len(), 1);
    let terrain = s.terrain();
    let tick = s.tick;
    let next = s.next_id;
    s.sterilize();
    assert_eq!(s.world.len(), 0);
    assert_eq!(s.terrain(), terrain);
    assert_eq!(s.tick, tick);
    assert_eq!(s.next_id, next);
    s.reset();
    assert_eq!(s.tick, 0);
    assert_eq!(s.next_id, 1);
}

#[test]
fn simulation_continues_after_removal_and_sterilization() {
    let mut s = quiet_world();
    let removed = s.spawn(400., 500., 0, &stationary_genes(0)).unwrap();
    let survivor = s.spawn(600., 500., 0, &stationary_genes(0)).unwrap();
    s.advance(1);
    s.remove_cell(removed).unwrap();
    s.advance(30);
    assert_eq!(s.world.len(), 1);
    assert!(s.world.get::<&Cell>(entity(&s, survivor)).unwrap().age > 1.);

    s.sterilize();
    s.advance(30);
    assert_eq!(s.world.len(), 0);
    let replacement = s.spawn(400., 500., 0, &stationary_genes(0)).unwrap();
    s.advance(30);
    assert_eq!(s.world.len(), 1);
    assert!(s.world.get::<&Cell>(entity(&s, replacement)).unwrap().age > 0.99);
}

fn assert_inside_dish(s: &Simulation) {
    for (_, (position, cell)) in s.world.query::<(&Position, &Cell)>().iter() {
        let distance = (position.x - SIZE / 2.).hypot(position.y - SIZE / 2.);
        assert!(distance + crate::components::radius(cell.mass) <= SIZE / 2. + 0.001);
    }
}

#[test]
fn circular_dish_contains_placement_movement_division_and_bonds() {
    let mut s = quiet_world();
    let mut genes = modern_genes(1);
    genes[1] = 1.;
    genes[9] = 1.;
    s.set_circular_dish(true);
    let parent = s.spawn(0., 0., 0, &genes).unwrap();
    assert_inside_dish(&s);
    s.reposition_cell(parent, SIZE, 0.).unwrap();
    s.boost_cell(parent).unwrap();
    assert_inside_dish(&s);
    mature_all(&mut s);
    s.advance(1);
    assert_eq!(s.world.len(), 2);
    assert_eq!(s.bonds.len(), 1);
    assert_inside_dish(&s);
    for _ in 0..5 {
        s.advance(120);
        assert_inside_dish(&s);
    }
}

#[test]
fn circular_dish_edge_death_does_not_change_native_square_worlds() {
    let mut dish = quiet_world();
    dish.set_circular_dish(true);
    dish.settings.kill_at_edge = true;
    let mut genes = stationary_genes(2);
    genes[5] = 60.;
    let id = dish.spawn(SIZE - 12.5, SIZE / 2., 0, &genes).unwrap();
    dish.world
        .get::<&mut Position>(entity(&dish, id))
        .unwrap()
        .angle = 0.;
    dish.advance(1);
    assert_eq!(dish.world.len(), 0);

    let mut square = quiet_world();
    square.settings.kill_at_edge = true;
    square.spawn(20., 20., 0, &stationary_genes(0)).unwrap();
    square.advance(30);
    assert_eq!(square.world.len(), 1);
    assert_eq!(square.snapshot()[1..3], [20., 20.]);
}

#[test]
fn circular_dish_migrates_square_saves_and_survives_reset() {
    let mut square = quiet_world();
    let id = square.spawn(20., 20., 0, &stationary_genes(0)).unwrap();
    square.advance(10);
    let mut dish = quiet_world();
    dish.set_circular_dish(true);
    dish.restore_checked(&legacy_checkpoint(&square, 3))
        .unwrap();
    assert_eq!(dish.world.len(), 1);
    assert_eq!(dish.tick, square.tick);
    assert_eq!(dish.genome(id), square.genome(id));
    assert_inside_dish(&dish);
    dish.advance(30);
    assert_inside_dish(&dish);

    let mut restored = quiet_world();
    restored.set_circular_dish(true);
    restored.restore_checked(&dish.encode_state()).unwrap();
    for _ in 0..3 {
        dish.advance(30);
        restored.advance(30);
        assert_eq!(dish.encode_state(), restored.encode_state());
    }
    dish.reset();
    dish.spawn(0., 0., 0, &stationary_genes(0)).unwrap();
    assert_inside_dish(&dish);
}

#[test]
fn restoring_an_edge_cell_keeps_geometry_and_next_tick() {
    let mut dish = quiet_world();
    dish.set_circular_dish(true);
    dish.settings.kill_at_edge = true;
    dish.spawn(SIZE - 10., SIZE / 2., 0, &stationary_genes(0))
        .unwrap();
    let saved = dish.encode_state();
    let mut restored = quiet_world();
    restored.restore_checked(&saved).unwrap();
    assert_eq!(restored.encode_state(), saved);
    dish.advance(1);
    restored.advance(1);
    assert_eq!(restored.world.len(), 0);
    assert_eq!(restored.encode_state(), dish.encode_state());
}

#[test]
fn aging_and_edge_controls_have_distinct_effects() {
    let mut s = quiet_world();
    s.settings.aging = false;
    let id = s.spawn(500., 500., 0, &modern_genes(1)).unwrap();
    s.world.get::<&mut Cell>(entity(&s, id)).unwrap().age = 1001.;
    s.advance(1);
    assert_eq!(s.world.len(), 1);
    s.settings.aging = true;
    s.advance(1);
    assert_eq!(s.world.len(), 0);
    let mut genes = modern_genes(1);
    genes[0] = 2.;
    genes[5] = 60.;
    let id = s.spawn(12., 500., 0, &genes).unwrap();
    s.world.get::<&mut Position>(entity(&s, id)).unwrap().angle = std::f32::consts::PI;
    s.settings.kill_at_edge = true;
    s.advance(1);
    assert_eq!(s.world.len(), 0);
}

fn modern_genes(count: usize) -> Vec<f32> {
    (0..count)
        .flat_map(|i| {
            [
                0., 8., i as f32, i as f32, 0.5, 0., 0., 1000., 1., 0., 0., 0.,
            ]
        })
        .collect()
}
fn entity(s: &Simulation, id: u32) -> Entity {
    s.world
        .query::<&Cell>()
        .iter()
        .find(|(_, c)| c.id == id)
        .unwrap()
        .0
}
fn quiet_world() -> Simulation {
    let mut s = Simulation::new(42);
    for tile in &mut s.tiles {
        tile.radiation = 0.;
    }
    s
}
fn mature_all(s: &mut Simulation) {
    for (_, c) in s.world.query_mut::<&mut Cell>() {
        c.age = MIN_DIVISION_AGE;
    }
}
#[test]
fn all_three_adhesion_flags_are_independent() {
    for bits in 0..8 {
        let mut s = quiet_world();
        let mut g = modern_genes(1);
        g[1] = 1.;
        g[9] = (bits & 1 != 0) as u8 as f32;
        g[10] = (bits & 2 != 0) as u8 as f32;
        g[11] = (bits & 4 != 0) as u8 as f32;
        let parent = s.spawn(500., 500., 0, &g).unwrap();
        let neighbor = s.spawn(522., 500., 0, &modern_genes(1)).unwrap();
        s.bonds.insert((parent, neighbor));
        mature_all(&mut s);
        s.advance(1);
        let mut expected = BTreeSet::new();
        if bits & 1 != 0 {
            expected.insert((3, 4));
        }
        if bits & 2 != 0 {
            expected.insert((neighbor, 3));
        }
        if bits & 4 != 0 {
            expected.insert((neighbor, 4));
        }
        assert_eq!(s.bonds, expected, "flags={bits}");
        assert_eq!(s.world.len(), 3);
    }
}
#[test]
fn adjacent_parents_can_divide_on_the_same_tick() {
    let mut s = quiet_world();
    let mut g = modern_genes(1);
    g[1] = 1.;
    g[10] = 1.;
    let a = s.spawn(500., 500., 0, &g).unwrap();
    let b = s.spawn(522., 500., 0, &g).unwrap();
    s.bonds.insert((a, b));
    mature_all(&mut s);
    s.advance(1);
    assert_eq!(s.links(), vec![3, 5]);
    assert_eq!(s.world.len(), 4);
}
#[test]
fn organism_keeps_flagellum_and_releases_photo_offspring() {
    let mut s = quiet_world();
    let mut g = modern_genes(3);
    g[1] = 1.;
    g[2] = 1.;
    g[3] = 2.;
    g[9] = 1.;
    g[12] = 2.;
    g[12 + 8] = 0.;
    g[24] = 1.;
    g[24 + 1] = 2.4;
    g[24 + 2] = 2.;
    g[24 + 3] = 0.;
    g[24 + 10] = 1.;
    s.spawn(500., 500., 0, &g).unwrap();
    mature_all(&mut s);
    s.advance(1);
    assert_eq!(s.links(), vec![2, 3]);
    let flagellum = entity(&s, 2);
    let phagocyte = entity(&s, 3);
    s.world.get::<&mut Cell>(flagellum).unwrap().mass = 10.;
    s.world.get::<&mut Cell>(phagocyte).unwrap().mass = 3.;
    mature_all(&mut s);
    s.advance(1);
    assert_eq!(s.links(), vec![2, 4]);
    assert_eq!(s.cell_mode(2), 1); // non-dividing flagellum survives above its threshold
    assert_eq!(s.cell_mode(4), 2); // continuing phagocyte inherits the bond
    assert_eq!(s.cell_mode(5), 0); // detached photo offspring
    assert_eq!(s.world.len(), 3);
    assert_eq!(s.deaths, 0);
}
#[test]
fn flagellum_pulls_a_passive_cell_and_bonds_survive_growth() {
    let mut s = quiet_world();
    let mut g = modern_genes(1);
    g[8] = 0.;
    let passive = s.spawn(522., 500., 0, &g).unwrap();
    g[0] = 2.;
    g[5] = 30.;
    let flagellum = s.spawn(500., 500., 0, &g).unwrap();
    let flag_entity = entity(&s, flagellum);
    s.world.get::<&mut Position>(flag_entity).unwrap().angle = 0.;
    s.bonds.insert((passive, flagellum));
    s.advance(30);
    let rows = s.snapshot();
    assert!(rows[1] > 527., "passive cell must be pulled by propulsion");
    let distance = (rows[1] - rows[13]).hypot(rows[2] - rows[14]);
    assert!((distance - rows[3] - rows[15] - 2.).abs() < 0.01);
    assert_eq!(s.links(), vec![passive, flagellum]);
}
#[test]
fn death_and_reset_remove_bonds() {
    let mut s = quiet_world();
    let a = s.spawn(500., 500., 0, &modern_genes(1)).unwrap();
    let b = s.spawn(522., 500., 0, &modern_genes(1)).unwrap();
    s.bonds.insert((a, b));
    let e = entity(&s, a);
    s.world.get::<&mut Cell>(e).unwrap().age = 1001.;
    s.advance(1);
    assert!(s.links().is_empty());
    assert_eq!(s.world.len(), 1);
    let c = s.spawn(544., 500., 0, &modern_genes(1)).unwrap();
    s.bonds.insert((b, c));
    s.reset();
    assert!(s.links().is_empty());
}
#[test]
fn forty_archetypes_and_shorter_infection_are_supported() {
    let mut s = quiet_world();
    let g = modern_genes(40);
    let recipient = s.spawn(500., 500., 39, &g).unwrap();
    assert_eq!(s.genome(recipient).len(), 2 + 40 * MODE_STRIDE);
    let mut donor = modern_genes(1);
    donor[0] = 3.;
    s.spawn(505., 500., 0, &donor).unwrap();
    s.advance(2);
    assert_eq!(s.genome(recipient).len(), 2 + MODE_STRIDE);
    assert_eq!(s.cell_mode(recipient), 0);
}
#[test]
fn bonded_chain_at_boundary_is_finite_and_deterministic() {
    let run = || {
        let mut s = quiet_world();
        let mut g = modern_genes(1);
        g[8] = 0.;
        g[0] = 2.;
        g[5] = 60.;
        let mut previous = None;
        for i in 0..24 {
            let id = s.spawn(12. + 22. * i as f32, 12., 0, &g).unwrap();
            if let Some(prev) = previous {
                s.bonds.insert((prev, id));
            }
            previous = Some(id);
        }
        for _ in 0..3 {
            s.advance(120);
        }
        let snapshot = s.snapshot();
        assert!(snapshot.iter().all(|v| v.is_finite()));
        assert_eq!(s.bonds.len(), 23);
        for row in snapshot.chunks_exact(SNAPSHOT_STRIDE) {
            assert!((12. ..=SIZE - 12.).contains(&row[1]));
            assert!((12. ..=SIZE - 12.).contains(&row[2]));
        }
        (snapshot, s.links())
    };
    assert_eq!(run(), run());
}
fn genes() -> Vec<f32> {
    [0., 2.4, 0., 0., 0.5, 12., 0.8, 240.].repeat(8)
}
#[test]
fn deterministic_replay() {
    let mut a = Simulation::new(42);
    let mut b = Simulation::new(42);
    for s in [&mut a, &mut b] {
        s.spawn(300., 300., 0, &genes()).unwrap();
        s.advance(120);
        s.advance(120);
    }
    assert_eq!(a.snapshot(), b.snapshot());
    assert_eq!(a.terrain(), b.terrain());
}
#[test]
fn division_uses_child_modes_and_conserves_mass() {
    let mut s = Simulation::new(1);
    let mut g = genes();
    g[1] = 1.;
    g[2] = 1.;
    g[3] = 2.;
    g[4] = 0.3;
    s.spawn(100., 100., 0, &g).unwrap();
    mature_all(&mut s);
    s.advance(1);
    let rows = s.snapshot();
    assert_eq!(rows.len(), 2 * SNAPSHOT_STRIDE);
    let mass = rows[5] + rows[17];
    assert!((mass - 1.).abs() < 0.01);
    assert!((rows[5] / mass - 0.3).abs() < 0.001);
    let mut modes: Vec<_> = s
        .world
        .query::<&Cell>()
        .iter()
        .map(|(_, c)| c.mode)
        .collect();
    modes.sort();
    assert_eq!(modes, vec![1, 2]);
}
#[test]
fn reset_repeats_seed() {
    let mut s = Simulation::new(9);
    s.spawn(100., 100., 0, &genes()).unwrap();
    let initial = s.snapshot();
    s.advance(100);
    s.reset();
    s.spawn(100., 100., 0, &genes()).unwrap();
    assert_eq!(initial, s.snapshot());
}
#[test]
fn resource_depletion_and_age_death() {
    let mut s = Simulation::new(2);
    let mut g = genes();
    g[0] = 1.;
    g[7] = 20.;
    g[1] = 8.;
    s.spawn(2000., 2500., 0, &g).unwrap();
    let food = s.tiles[tile_index(2000., 2500.)].food;
    s.advance(30);
    assert!(s.tiles[tile_index(2000., 2500.)].food < food);
    for _ in 0..6 {
        s.advance(120);
    }
    assert_eq!(s.world.len(), 0);
    assert_eq!(s.deaths, 1);
}
#[test]
fn infection_copies_genome_and_obeys_cooldown() {
    let mut s = Simulation::new(12);
    let mut donor = genes();
    donor[0] = 3.;
    donor[1] = 7.;
    donor[5] = 0.;
    let mut recipient = genes();
    recipient[1] = 8.;
    recipient[5] = 0.;
    let id = s.spawn(500., 500., 0, &donor).unwrap();
    let other = s.spawn(505., 500., 0, &recipient).unwrap();
    s.spawn(500., 505., 0, &recipient).unwrap();
    s.advance(1);
    assert_eq!(s.infections, 1);
    assert_eq!(s.genome(id), s.genome(other));
    let c = s
        .world
        .query::<&Cell>()
        .iter()
        .find(|(_, c)| c.id == id)
        .unwrap()
        .1
        .clone();
    assert!(c.cooldown > 2.9);
}
#[test]
fn radiation_mutates_genomes() {
    let mut s = Simulation::new(42);
    let mut g = genes();
    g[1] = 8.;
    g[5] = 0.;
    g[7] = 1000.;
    for i in 0..40 {
        s.spawn(
            2300. + (i % 8) as f32 * 25.,
            700. + (i / 8) as f32 * 25.,
            0,
            &g,
        )
        .unwrap();
    }
    for _ in 0..15 {
        s.advance(120);
    }
    assert!(
        s.world
            .query::<&Cell>()
            .iter()
            .any(|(_, c)| c.mutations > 0)
    );
}
#[test]
fn edit_is_isolated_and_boundary_motion_is_finite() {
    let mut s = Simulation::new(17);
    let a = s.spawn(-100., 5000., 0, &genes()).unwrap();
    let b = s.spawn(100., 100., 0, &genes()).unwrap();
    let original = s.genome(b);
    let mut edited = genes();
    edited[0] = 2.;
    edited[5] = 60.;
    s.edit(a, 0, &edited).unwrap();
    assert_eq!(s.genome(b), original);
    s.advance(120);
    for row in s.snapshot().chunks_exact(SNAPSHOT_STRIDE) {
        assert!(row.iter().all(|v| v.is_finite()));
        assert!((0. ..=SIZE).contains(&row[1]));
        assert!((0. ..=SIZE).contains(&row[2]));
    }
}
#[test]
fn division_respects_population_limit() {
    let mut s = Simulation::new(8);
    let mut g = genes();
    g[1] = 1.;
    g[5] = 0.;
    for i in 0..LIMIT - 1 {
        s.spawn(
            20. + (i % 80) as f32 * 37.,
            20. + (i / 80) as f32 * 37.,
            0,
            &g,
        )
        .unwrap();
    }
    mature_all(&mut s);
    s.advance(1);
    assert_eq!(s.world.len() as usize, LIMIT);
}
#[test]
fn chunk_snapshot_uses_programmed_speed_s8_connections_anchor_and_viscosity() {
    use super::program::SPEED_TARGET;
    use crate::genome::{Mode, Program};

    // The wire speed is in world units per second: prediction multiplies it
    // directly by dt and does not receive the environment's viscosity.
    for (kind, degree, anchored, signal, expected) in [
        (2, 0, false, 0.5, 3.),
        (0, 0, false, 0.5, 0.45),
        (2, 2, false, 0.5, 6.),
        (0, 2, false, 0.5, 0.),
        (2, 0, true, 0.5, 0.),
        (2, 2, true, 0.5, 0.),
        (2, 0, false, 0., 0.),
        (2, 0, false, 8., 30.),
    ] {
        let mut s = quiet_world();
        s.settings.viscosity = 2.;
        let mut mode = Mode {
            kind,
            speed: 40.,
            can_divide: false,
            program_count: 2,
            ..Mode::default()
        };
        mode.programs[0] = Program {
            target: SPEED_TARGET as u8,
            input: 8, // S8, rather than the former mass input at this offset.
            scale: 12.,
            ..Program::default()
        };
        mode.programs[1] = Program {
            target: SPEED_TARGET as u8,
            input: 12, // Connection count after the eight signal inputs.
            scale: 3.,
            ..Program::default()
        };
        let genes = Genome::from_modes(vec![mode]).values();
        let id = s.spawn(100., 100., 0, &genes).unwrap();
        let e = entity(&s, id);
        {
            let mut cell = s.world.get::<&mut Cell>(e).unwrap();
            cell.signals[7] = signal;
            cell.anchored = anchored;
        }
        for i in 0..degree {
            let neighbor = s
                .spawn(140. + i as f32 * 40., 100., 0, &stationary_genes(0))
                .unwrap();
            s.bonds.insert((id, neighbor));
        }
        let chunk = s.chunk_snapshot(0, 0, 512.);
        let row = chunk
            .cells
            .chunks_exact(28)
            .find(|row| row[0] == id as f32)
            .unwrap();
        assert!(
            (row[10] - expected).abs() < 0.000001,
            "kind={kind}, degree={degree}, anchored={anchored}, S8={signal}"
        );
        assert_eq!(row[11], (degree != 0) as u8 as f32);
        assert_eq!(row[27], anchored as u8 as f32);

        if degree == 0 {
            let before = *s.world.get::<&Position>(e).unwrap();
            s.advance(1);
            let after = *s.world.get::<&Position>(e).unwrap();
            let chunk = s.chunk_snapshot(0, 0, 512.);
            let speed = chunk.cells[10];
            assert!((after.x - before.x - libm::cosf(after.angle) * speed / 30.).abs() < 0.00002);
            assert!((after.y - before.y - libm::sinf(after.angle) * speed / 30.).abs() < 0.00002);
        }
    }
}

#[test]
fn chunk_snapshot_exports_cells_links_and_terrain() {
    let mut s = Simulation::new(5);
    let g = modern_genes(1);
    let a = s.spawn(100., 100., 0, &g).unwrap();
    let b = s.spawn(600., 100., 0, &g).unwrap();
    s.bonds.insert((a, b));

    let first = s.chunk_snapshot(0, 0, 512.);
    assert_eq!(first.cells.len(), 28);
    assert_eq!(first.cells[0], a as f32);
    assert_eq!(first.cells[11], 1.);
    assert_eq!(first.links, vec![a, b]);
    assert_eq!(&first.cells[12..15], &[-1., 0., INITIAL_NITROGEN]);
    assert_eq!(&first.cells[15..23], &[0.; 8]);
    assert_eq!(&first.cells[23..], &[0., 0., -1., 0., 0.]);
    assert_eq!(first.terrain.len(), 8 * 8 * 6);
    assert_eq!(first.terrain[4], s.tiles[0].nitrates);
    assert_eq!(first.terrain[5], s.tiles[0].covered_food);

    let second = s.chunk_snapshot(1, 0, 512.);
    assert_eq!(second.cells.len(), 28);
    assert_eq!(second.cells[0], b as f32);
    assert!(second.links.is_empty());
}
