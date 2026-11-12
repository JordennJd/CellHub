use super::Simulation;

use crate::cell_types::{COUNT as KIND_COUNT, NITROCYTE as NITROCYTE_KIND};
use crate::components::{
    Cell, INITIAL_NITROGEN, MIN_DIVISION_AGE, Position, SIZE, SNAPSHOT_STRIDE, STEP, radius,
};
use crate::genome::{Genome, MODE_STRIDE, Mode, decode_checked};
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
fn all_eighteen_archetypes_round_trip_and_spawn() {
    let mut s = quiet_world();
    for kind in 0..KIND_COUNT {
        let genome = Genome::from_modes(vec![Mode {
            kind,
            ..Mode::default()
        }]);
        let genes = genome.values();
        assert_eq!(decode_checked(&genes).unwrap().values(), genes);
        s.spawn(100. + f32::from(kind) * 32., 500., 0, &genes)
            .unwrap();
    }
    let rows = s.snapshot();
    assert_eq!(rows.len(), usize::from(KIND_COUNT) * SNAPSHOT_STRIDE);
    for (kind, row) in rows.chunks_exact(SNAPSHOT_STRIDE).enumerate() {
        assert_eq!(row[4], kind as f32);
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
    assert!(
        (daughters[0].1 - (0.8 - 0.025 * STEP) / 2.).abs() < 0.001,
        "daughter reserve {}",
        daughters[0].1
    );
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
    assert!((protected.0 - (1. - 0.025 * STEP)).abs() < 0.000001);
    assert!((run(5, 4, false).0 - (1. - 0.014 * STEP)).abs() < 0.000001);
    assert_eq!(run(3, 0, false).1, 1);
    assert_eq!(run(3, 0, true).1, 0);
    assert_eq!(run(3, 4, false).1, 0);
    // Two predators cannot feed each other in an energy-generating loop.
    assert!((run(5, 5, false).0 - (1. - 0.025 * STEP)).abs() < 0.000001);
}

#[test]
fn minimum_age_prevents_instant_division_after_boost() {
    let mut s = quiet_world();
    let mut genes = modern_genes(1);
    genes[1] = 1.;
    let id = s.spawn(500., 500., 0, &genes).unwrap();
    s.boost_cell(id).unwrap();
    s.advance(4);
    assert_eq!(s.world.len(), 1);
    s.advance(1);
    assert_eq!(s.world.len(), 2);
    assert!(s.world.query::<&Cell>().iter().all(|(_, c)| c.age == 0.));
}

fn recoil_fixture(bind_children: bool, keep_a: bool) -> (Simulation, u32, Vec<f32>) {
    let mut sim = quiet_world();
    sim.configure_environment(super::environment::Environment {
        light: 0.,
        radiation: 0.,
        ..Default::default()
    })
    .unwrap();
    let passive = Mode {
        can_divide: false,
        speed: 0.,
        ..Mode::default()
    };
    let genes = Genome::from_modes(vec![
        Mode {
            can_divide: true,
            threshold: 1.,
            speed: 0.,
            angle: 0.,
            ratio: 0.75,
            child_a: 1,
            child_b: 1,
            bind_children,
            keep_a,
            ..Mode::default()
        },
        passive,
    ])
    .values();
    let parent = sim.spawn(500., 500., 0, &genes).unwrap();
    sim.boost_cell(parent).unwrap();
    (sim, parent, genes)
}

#[test]
fn unbound_division_gives_unequal_daughters_equal_and_opposite_momentum() {
    let (mut sim, _, _) = recoil_fixture(false, false);
    sim.advance(5);
    assert_eq!(sim.world.len(), 2);
    let a = entity(&sim, 2);
    let b = entity(&sim, 3);
    let position = |sim: &Simulation, cell| *sim.world.get::<&Position>(cell).unwrap();
    let (a_before, b_before) = (position(&sim, a), position(&sim, b));
    let a_recoil = {
        let a = sim.world.get::<&Cell>(a).unwrap();
        let b = sim.world.get::<&Cell>(b).unwrap();
        assert!(a.mass > b.mass);
        for (a_pos, b_pos) in [(a_before.x, b_before.x), (a_before.y, b_before.y)] {
            let center = (a_pos * a.mass + b_pos * b.mass) / (a.mass + b.mass);
            assert!((center - 500.).abs() < 0.0001);
        }
        assert_eq!(a.recoil, b.recoil.map(|value| -value));
        assert!(
            a.recoil[0] * (a_before.x - b_before.x) + a.recoil[1] * (a_before.y - b_before.y) > 0.
        );
        a.recoil
    };

    sim.advance(1);

    let (a_after, b_after) = (position(&sim, a), position(&sim, b));
    let forward = (a_after.x - a_before.x) * a_recoil[0] + (a_after.y - a_before.y) * a_recoil[1];
    let backward = (b_before.x - b_after.x) * a_recoil[0] + (b_before.y - b_after.y) * a_recoil[1];
    assert!(
        backward > forward && forward > 0.,
        "lighter daughter moves farther"
    );
    let damped = sim.world.get::<&Cell>(a).unwrap().recoil;
    assert!(
        libm::hypotf(damped[0], damped[1]) < libm::hypotf(a_recoil[0], a_recoil[1]),
        "viscosity damps recoil"
    );
}

#[test]
fn bound_sisters_share_parent_velocity_without_new_recoil() {
    for momentum in [[0., 0.], [0.4, -0.2]] {
        let (mut sim, parent, _) = recoil_fixture(true, false);
        sim.advance(4);
        sim.world
            .get::<&mut Cell>(entity(&sim, parent))
            .unwrap()
            .recoil = momentum;
        sim.advance(1);
        assert_eq!(sim.links(), vec![2, 3]);
        let a = sim.world.get::<&Cell>(entity(&sim, 2)).unwrap();
        let b = sim.world.get::<&Cell>(entity(&sim, 3)).unwrap();
        if momentum == [0.; 2] {
            assert_eq!(a.recoil, momentum);
            assert_eq!(b.recoil, momentum);
        } else {
            for (component, initial) in momentum.into_iter().enumerate() {
                let a_speed = a.recoil[component] / a.mass;
                let b_speed = b.recoil[component] / b.mass;
                assert!(a_speed * initial > 0.);
                assert!((a_speed - b_speed).abs() < 0.00001);
            }
        }
    }
}

#[test]
fn ejection_moves_the_retained_organism_through_its_adhesive_bond() {
    let divided = || {
        let (mut sim, parent, genes) = recoil_fixture(false, true);
        let neighbor = sim.spawn(522., 500., 1, &genes).unwrap();
        sim.bonds.insert((parent, neighbor));
        sim.world
            .get::<&mut Position>(entity(&sim, parent))
            .unwrap()
            .angle = 0.;
        sim.advance(15);
        assert_eq!(sim.links(), vec![neighbor, 3]);
        (sim, neighbor)
    };
    let (mut recoiling, neighbor) = divided();
    let (mut control, _) = divided();
    for (_, cell) in control.world.query_mut::<&mut Cell>() {
        cell.recoil = [0.; 2];
    }
    let x = |sim: &Simulation, id| sim.world.get::<&Position>(entity(sim, id)).unwrap().x;
    recoiling.advance(5);
    control.advance(5);
    assert_eq!(recoiling.links(), vec![neighbor, 3]);
    assert!(x(&recoiling, neighbor) > x(&control, neighbor));
    assert!(x(&recoiling, 3) > x(&control, 3));
    assert!(x(&recoiling, 4) < x(&control, 4));
    let neighbor_entity = entity(&recoiling, neighbor);
    assert!(
        recoiling
            .world
            .get::<&Cell>(neighbor_entity)
            .unwrap()
            .recoil[0]
            > 0.
    );
    recoiling.remove_cell(3).unwrap();
    let before = x(&recoiling, neighbor);
    recoiling.advance(1);
    assert!(x(&recoiling, neighbor) > before);
}

fn collision_pair(distance: f32, masses: [f32; 2], momenta: [f32; 2]) -> (Simulation, [Entity; 2]) {
    let mut sim = quiet_world();
    let genes = stationary_genes(0);
    let pair = [500., 500. + distance].map(|x| {
        let id = sim.spawn(x, 500., 0, &genes).unwrap();
        entity(&sim, id)
    });
    for ((entity, mass), momentum) in pair.into_iter().zip(masses).zip(momenta) {
        let mut cell = sim.world.get::<&mut Cell>(entity).unwrap();
        cell.mass = mass;
        cell.recoil = [momentum, 0.];
    }
    (sim, pair)
}

#[test]
fn head_on_collision_rebounds_and_conserves_net_momentum_after_drag() {
    let masses = [1., 4.];
    let momenta = [96., -95.];
    let touching = masses.map(radius).into_iter().sum::<f32>();
    let (mut sim, pair) = collision_pair(touching + 2., masses, momenta);
    let (mut control, control_pair) = collision_pair(touching + 60., masses, momenta);
    let distance = |sim: &Simulation, [a, b]: [Entity; 2]| {
        let a = sim.world.get::<&Position>(a).unwrap();
        let b = sim.world.get::<&Position>(b).unwrap();
        libm::hypotf(b.x - a.x, b.y - a.y)
    };
    let momentum = |sim: &Simulation, pair: [Entity; 2]| {
        pair.map(|e| sim.world.get::<&Cell>(e).unwrap().recoil[0])
    };

    sim.advance(1);
    control.advance(1);
    let [left, right] = momentum(&sim, pair);
    let [control_left, control_right] = momentum(&control, control_pair);
    assert!(
        left < 0. && right > 0.,
        "outgoing momentum: {left}, {right}"
    );
    // Recoil stores momentum. Comparing with a noncolliding pair factors out
    // viscosity, and the different masses catch equal-velocity corrections.
    let net_error = (left + right - (control_left + control_right)).abs();
    assert!(
        net_error < 0.001,
        "collision changed net momentum by {net_error}"
    );
    let separation = distance(&sim, pair);
    assert!(
        separation > touching * 0.99,
        "collision left substantial overlap"
    );
    sim.advance(2);
    assert!(
        distance(&sim, pair) > separation,
        "the rebounding pair must separate"
    );
}

#[test]
fn swept_collision_stops_fast_cells_passing_through_each_other() {
    // Each unimpeded cell would travel about the entire gap in one tick.
    // A solver checking only final overlap would see two separated cells.
    for initial_gap in [40., 16.] {
        let (mut sim, [a, b]) = collision_pair(initial_gap, [1., 1.], [1200., -1200.]);
        sim.advance(1);
        let pa = sim.world.get::<&Position>(a).unwrap();
        let pb = sim.world.get::<&Position>(b).unwrap();
        let a = sim.world.get::<&Cell>(a).unwrap();
        let b = sim.world.get::<&Cell>(b).unwrap();
        let separation = pb.x - pa.x;
        assert!(
            separation > 0.99 * (radius(a.mass) + radius(b.mass)),
            "cells crossed or remained overlapped: x gap {separation}"
        );
        assert!(a.recoil[0] < 0. && b.recoil[0] > 0.);
    }
}

#[test]
fn stationary_overlap_produces_opposing_collision_momentum() {
    let initial_distance = 14.;
    let (mut sim, [a, b]) = collision_pair(initial_distance, [1., 1.], [0., 0.]);
    sim.advance(1);
    let pa = sim.world.get::<&Position>(a).unwrap();
    let pb = sim.world.get::<&Position>(b).unwrap();
    let a = sim.world.get::<&Cell>(a).unwrap();
    let b = sim.world.get::<&Cell>(b).unwrap();
    assert!(pb.x - pa.x > initial_distance);
    assert!(a.recoil[0] < 0. && b.recoil[0] > 0.);
    assert!((a.recoil[0] + b.recoil[0]).abs() < 0.001);
}

#[test]
fn a_deep_mature_gamete_contact_can_fuse_before_separation() {
    let genes = Genome::from_modes(vec![
        Mode {
            kind: crate::cell_types::GAMETE,
            compatible_mode: 1,
            offspring_mode: 1,
            speed: 0.,
            ..Mode::default()
        },
        Mode::default(),
    ])
    .values();
    for (gap, population) in [(12., 1), (16., 2)] {
        let mut sim = quiet_world();
        sim.spawn(500., 500., 0, &genes).unwrap();
        sim.spawn(500. + gap, 500., 0, &genes).unwrap();
        mature_all(&mut sim);
        sim.advance(1);
        assert_eq!(sim.world.len(), population, "gap {gap}");
    }
}

#[test]
fn dense_collision_cluster_resolves_most_penetration_in_a_few_ticks() {
    let mut sim = quiet_world();
    let genes = stationary_genes(0);
    for y in 0..3 {
        for x in 0..3 {
            sim.spawn(500. + x as f32 * 11., 500. + y as f32 * 11., 0, &genes)
                .unwrap();
        }
    }
    let deepest_penetration = |sim: &Simulation| {
        let bodies: Vec<_> = sim
            .world
            .query::<(&Position, &Cell)>()
            .iter()
            .map(|(_, (position, cell))| (*position, radius(cell.mass)))
            .collect();
        let mut max_penetration = 0_f32;
        for (i, (a, ar)) in bodies.iter().enumerate() {
            for (b, br) in &bodies[i + 1..] {
                let distance = libm::hypotf(b.x - a.x, b.y - a.y);
                max_penetration = max_penetration.max(ar + br - distance);
            }
        }
        max_penetration
    };
    let initial = deepest_penetration(&sim);
    assert!(initial > 5.);
    sim.advance(3);
    assert_eq!(sim.world.len(), 9);
    let remaining = deepest_penetration(&sim);
    assert!(
        remaining < initial / 3.,
        "penetration {initial} → {remaining}"
    );
}

#[test]
fn collision_processing_preserves_direct_bond_rest_distance() {
    use super::bonds::ADHESION_GAP;

    let (mut sim, [a, b]) = collision_pair(
        radius(1.) + radius(4.) + ADHESION_GAP,
        [1., 4.],
        [96., -95.],
    );
    sim.bonds.insert((1, 2));
    for _ in 0..8 {
        sim.advance(1);
        assert_eq!(sim.links(), [1, 2]);
        let pa = sim.world.get::<&Position>(a).unwrap();
        let pb = sim.world.get::<&Position>(b).unwrap();
        let ca = sim.world.get::<&Cell>(a).unwrap();
        let cb = sim.world.get::<&Cell>(b).unwrap();
        let distance = libm::hypotf(pb.x - pa.x, pb.y - pa.y);
        let rest = radius(ca.mass) + radius(cb.mass) + ADHESION_GAP;
        assert!(
            (distance - rest).abs() < 0.05,
            "bond distance {distance}, rest {rest}"
        );
        let (va, vb) = (ca.recoil[0] / ca.mass, cb.recoil[0] / cb.mass);
        assert!(
            (va - vb).abs() < 0.001,
            "a bonded pair must not rebound apart"
        );
    }
}

#[test]
fn perpendicular_chains_keep_bonds_short_under_driven_contact() {
    use super::bonds::ADHESION_GAP;
    use crate::cell_types::{FLAGELLOCYTE, PHOTOCYTE};

    let mut sim = quiet_world();
    sim.set_circular_dish(true);
    sim.configure_environment(super::environment::Environment {
        light: 0.,
        food: 0.,
        radiation: 0.,
        viscosity: 1.5,
        aging: false,
        ..Default::default()
    })
    .unwrap();
    let rest = 2. * radius(1.) + ADHESION_GAP;
    let mut spawn_chain = |origin: [f32; 2], step: [f32; 2], count, angle, genes: &[f32]| {
        let mut chain = Vec::with_capacity(count);
        let mut previous = None;
        for i in 0..count {
            let x = origin[0] + step[0] * i as f32;
            let y = origin[1] + step[1] * i as f32;
            let id = sim.spawn(x, y, 0, genes).unwrap();
            let body = entity(&sim, id);
            sim.world.get::<&mut Position>(body).unwrap().angle = angle;
            if let Some(previous) = previous {
                sim.bonds.insert((previous, id));
            }
            previous = Some(id);
            chain.push(body);
        }
        chain
    };
    let wall = spawn_chain(
        [1536., 1426.],
        [0., rest],
        11,
        std::f32::consts::FRAC_PI_2,
        &stationary_genes(PHOTOCYTE),
    );
    let mut driven_genes = stationary_genes(FLAGELLOCYTE);
    driven_genes[5] = 60.;
    let train = spawn_chain([1500., 1503.], [-rest, 0.], 8, 0., &driven_genes);
    for &end in [&wall[0], wall.last().unwrap()] {
        sim.world.get::<&mut Cell>(end).unwrap().anchored = true;
    }

    let sample = |sim: &Simulation, chain: &[Entity]| -> Vec<_> {
        chain
            .iter()
            .map(|&body| {
                let position = *sim.world.get::<&Position>(body).unwrap();
                let mass = sim.world.get::<&Cell>(body).unwrap().mass;
                (position, radius(mass))
            })
            .collect()
    };
    let distance = |a: Position, b: Position| libm::hypotf(b.x - a.x, b.y - a.y);
    let side = |a: Position, b: Position, c: Position| {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    };
    let mut closest_clearance = f32::INFINITY;
    let mut peak_extension = 0_f32;
    for tick in 1..=180 {
        sim.advance(1);
        let barrier = sample(&sim, &wall);
        let moving = sample(&sim, &train);
        for &(a, ar) in &barrier {
            for &(b, br) in &moving {
                closest_clearance = closest_clearance.min(distance(a, b) - ar - br);
            }
        }
        for edge in barrier.windows(2).chain(moving.windows(2)) {
            let [(a, ar), (b, br)] = [edge[0], edge[1]];
            peak_extension = peak_extension.max(distance(a, b) - ar - br - ADHESION_GAP);
        }
        for edge in barrier.windows(2) {
            let (a, b) = (edge[0].0, edge[1].0);
            for edge in moving.windows(2) {
                let (c, d) = (edge[0].0, edge[1].0);
                assert!(
                    side(a, b, c) * side(a, b, d) >= 0. || side(c, d, a) * side(c, d, b) >= 0.,
                    "bond segments crossed at tick {tick}"
                );
            }
        }
    }
    assert_eq!(sim.bonds.len(), wall.len() + train.len() - 2);
    assert!(
        closest_clearance < 0.5,
        "the chains must reach contact, closest clearance {closest_clearance}"
    );
    // The 10 Hz step resolves contacts with coarser travel, so this stress
    // setup keeps enough viscosity for the discrete bond range it exercises.
    assert!(peak_extension < 4., "peak bond extension {peak_extension}");
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
    let split_axis = (noise - 0.5) * super::metabolism::RANDOM_TURN_RATE * STEP + 0.2;
    assert!((rows[9] - (split_axis + 0.3)).abs() < 0.00001);
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
    let mirrored_axis = (noise - 0.5) * super::metabolism::RANDOM_TURN_RATE * STEP - 0.2;
    assert_eq!(rows[11], 0.);
    assert_eq!(rows[23], 1.);
    assert!((rows[9] - (mirrored_axis - 0.3)).abs() < 0.00001);
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
            .all(|(_, g)| g.mode(0).kind == 4 && g.offspring().mode(0).kind == 4)
    );
    assert!(
        point
            .world
            .query::<&crate::genome::Genome>()
            .iter()
            .any(|(_, g)| g.mode(0).threshold == 4. && g.offspring().mode(0).threshold != 4.)
    );
    assert!(
        full.world
            .query::<&crate::genome::Genome>()
            .iter()
            .any(|(_, g)| g.mode(0).kind == 4 && g.offspring().mode(0).kind != 4)
    );
}

#[test]
fn radiation_mutation_is_expressed_at_birth_and_survives_a_checkpoint() {
    use crate::cell_types::{PHAGOCYTE, PHOTOCYTE};
    use crate::genome::{Genome, Mode};

    let mut sim = quiet_world();
    let genes = Genome::from_modes(vec![Mode {
        threshold: 8.,
        speed: 0.,
        ..Mode::default()
    }]);
    let id = sim.spawn_checked(300., 300., 0, &genes.values()).unwrap();
    for tile in &mut sim.tiles {
        tile.radiation = 5.;
    }
    for _ in 0..1200 {
        sim.advance(1);
        if sim.snapshot()[8] > 0. {
            break;
        }
    }
    let parent = sim.snapshot();
    assert_eq!(parent.len(), 12);
    assert_eq!(parent[0], id as f32);
    assert_eq!(parent[4], f32::from(PHOTOCYTE));
    assert_eq!(parent[8], 1.);
    assert_eq!(
        sim.chunk_snapshot(0, 0, 512.).cells[4],
        f32::from(PHOTOCYTE)
    );
    assert_eq!(sim.genome(id)[2], f32::from(PHAGOCYTE));

    let state = sim.encode_state();
    let mut restored = quiet_world();
    restored.restore_checked(&state).unwrap();
    assert_eq!(restored.encode_state(), state);
    assert_eq!(restored.snapshot(), parent);
    for world in [&mut sim, &mut restored] {
        for tile in &mut world.tiles {
            tile.radiation = 0.;
        }
        let mut parent = world.world.get::<&mut Cell>(entity(world, id)).unwrap();
        parent.mass = 10.;
        parent.nitrogen = 1.;
        parent.age = MIN_DIVISION_AGE;
        drop(parent);
        world.advance(1);
        let cells = world.snapshot();
        assert_eq!(cells.len(), 24);
        for daughter in cells.chunks_exact(12) {
            assert_eq!(daughter[4], f32::from(PHAGOCYTE));
            assert_eq!(daughter[7], 1.);
        }
    }
    assert_eq!(restored.encode_state(), sim.encode_state());
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
        (0, state[0] + 1.),
        (2, 0.),
        (4, 1.),
        (8, f64::NAN),
        (11, 0.),
        (12, 0.5),
        (14, 0.5),
        (15, 2.),
        (16, -4.),
        (17, -1.),
        (18, 3.),
        (19, 2.),
        (20, 0.),
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
    for (angle, near_flag, near_daughter) in [(0., 2, 3), (std::f32::consts::PI, 4, 4)] {
        for bits in 0..8 {
            let mut s = quiet_world();
            let mut g = modern_genes(1);
            g[1] = 1.;
            g[9] = (bits & 1 != 0) as u8 as f32;
            g[10] = (bits & 2 != 0) as u8 as f32;
            g[11] = (bits & 4 != 0) as u8 as f32;
            let parent = s.spawn(500., 500., 0, &g).unwrap();
            let neighbor = s.spawn(522., 500., 0, &modern_genes(1)).unwrap();
            s.world
                .get::<&mut Position>(entity(&s, parent))
                .unwrap()
                .angle = angle;
            s.bonds.insert((parent, neighbor));
            mature_all(&mut s);
            s.advance(1);
            let mut expected = BTreeSet::new();
            if bits & 1 != 0 {
                expected.insert((3, 4));
            }
            if bits & near_flag != 0 {
                expected.insert((neighbor, near_daughter));
            }
            assert_eq!(s.bonds, expected, "angle={angle}, flags={bits}");
            assert_eq!(s.world.len(), 3);
        }
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
    s.world.get::<&mut Position>(entity(&s, a)).unwrap().angle = 0.;
    s.world.get::<&mut Position>(entity(&s, b)).unwrap().angle = std::f32::consts::PI;
    s.bonds.insert((a, b));
    mature_all(&mut s);
    s.advance(1);
    assert_eq!(s.links(), vec![3, 5]);
    assert_eq!(s.world.len(), 4);
}

#[test]
fn inheritance_depends_on_candidate_contact_at_the_same_oblique_angle() {
    for (separation, in_contact) in [(26., true), (100., false)] {
        let mut s = quiet_world();
        let genes = Genome::from_modes(vec![Mode {
            threshold: 1.,
            angle: 0.,
            speed: 0.,
            bind_children: true,
            keep_a: true,
            keep_b: true,
            ..Mode::default()
        }])
        .values();
        let a = s.spawn(500., 500., 0, &genes).unwrap();
        let b = s.spawn(500. + separation, 500., 0, &genes).unwrap();
        for id in [a, b] {
            let entity = entity(&s, id);
            s.world.get::<&mut Cell>(entity).unwrap().mass = 2.;
            s.world.get::<&mut Position>(entity).unwrap().angle = std::f32::consts::FRAC_PI_3;
        }
        s.bonds.insert((a, b));
        mature_all(&mut s);
        s.advance(1);
        assert_eq!(s.world.len(), 4);
        let mut expected = BTreeSet::from([(3, 4), (5, 6)]);
        if in_contact {
            expected.extend([(3, 5), (4, 6)]);
        }
        assert_eq!(s.bonds, expected, "parent separation {separation}");
    }
}

#[test]
fn both_daughters_can_inherit_without_creating_diagonal_bonds() {
    for turn in [0., std::f32::consts::FRAC_PI_2] {
        let mut s = quiet_world();
        let genes = Genome::from_modes(vec![Mode {
            threshold: 1.,
            angle: 0.,
            speed: 0.,
            child_angle_a: turn,
            bind_children: true,
            keep_a: true,
            keep_b: true,
            ..Mode::default()
        }])
        .values();
        let founder = s.spawn(500., 500., 0, &genes).unwrap();
        let founder = entity(&s, founder);
        s.world.get::<&mut Cell>(founder).unwrap().mass = 4.;
        mature_all(&mut s);
        s.advance(1);
        assert_eq!(s.bonds, BTreeSet::from([(2, 3)]));

        let position = |sim: &Simulation, id| *sim.world.get::<&Position>(entity(sim, id)).unwrap();
        let (a, b) = (position(&s, 2), position(&s, 3));
        let (dx, dy) = (a.x - b.x, a.y - b.y);
        let length = libm::hypotf(dx, dy);
        let (ux, uy) = (dx / length, dy / length);
        let displacement = |from: Position, to: Position| {
            let (x, y) = (to.x - from.x, to.y - from.y);
            (x * ux + y * uy, y * ux - x * uy)
        };

        mature_all(&mut s);
        s.advance(1);
        assert_eq!(s.world.len(), 4);
        assert_eq!(s.bonds.len(), 3);
        assert!(s.bonds.contains(&(4, 5)));
        assert!(s.bonds.contains(&(6, 7)));
        assert!(s.bonds.iter().any(|&(a, b)| a <= 5 && b >= 6));

        if turn == 0. {
            assert!(s.bonds.contains(&(5, 7)));
            let chain = [6, 7, 5, 4].map(|id| position(&s, id));
            for pair in chain.windows(2) {
                let (along, across) = displacement(pair[0], pair[1]);
                assert!(
                    along > 0. && across.abs() < along * 0.001,
                    "along={along}, across={across}"
                );
            }
        } else {
            let [a, b, c, d] = [4, 5, 6, 7].map(|id| position(&s, id));
            let (along, across) = displacement(a, b);
            assert!(across.abs() > 2. * along.abs());
            let (along, across) = displacement(c, d);
            assert!(along.abs() > 2. * across.abs());
        }
    }
}

fn growing_path(angle: f32, mirror: bool) -> Simulation {
    let mut s = Simulation::new(73421);
    s.set_circular_dish(true);
    s.configure_environment(super::environment::Environment {
        radiation: 0.,
        ..Default::default()
    })
    .unwrap();
    // The same founder and conditions as the preview. Each generation grows
    // to division mass on its own, and the fourth generation stops dividing.
    let modes = (0..=4)
        .map(|mode| Mode {
            threshold: 1.,
            angle,
            child_angle_a: -angle,
            child_angle_b: -angle,
            mirror_a: mirror,
            mirror_b: mirror,
            speed: 0.,
            lifespan: 1000.,
            child_a: (mode + 1).min(4),
            child_b: (mode + 1).min(4),
            can_divide: mode < 4,
            bind_children: true,
            keep_a: true,
            keep_b: true,
            ..Mode::default()
        })
        .collect();
    s.spawn(600., 600., 0, &Genome::from_modes(modes).values())
        .unwrap();
    s
}

fn advance_to(sim: &mut Simulation, tick: u32) {
    while sim.tick < tick {
        sim.advance((tick - sim.tick).min(120));
    }
}

fn path_positions(sim: &Simulation, count: usize) -> Vec<Position> {
    assert_eq!(sim.world.len() as usize, count, "tick {}", sim.tick);
    assert_eq!(sim.bonds.len(), count - 1, "tick {}", sim.tick);
    let mut neighbors = BTreeMap::<u32, Vec<u32>>::new();
    for &(a, b) in &sim.bonds {
        neighbors.entry(a).or_default().push(b);
        neighbors.entry(b).or_default().push(a);
    }
    assert_eq!(neighbors.len(), count);
    assert!(neighbors.values().all(|ends| ends.len() <= 2));
    let (&first, _) = neighbors.iter().find(|(_, ends)| ends.len() == 1).unwrap();
    let mut order = vec![first];
    let (mut previous, mut current) = (None, first);
    while let Some(&next) = neighbors[&current].iter().find(|&&id| Some(id) != previous) {
        assert!(!order.contains(&next), "a path cannot revisit a cell");
        order.push(next);
        previous = Some(current);
        current = next;
    }
    assert_eq!(order.len(), count, "one path must reach all cells");
    order
        .into_iter()
        .map(|id| *sim.world.get::<&Position>(entity(sim, id)).unwrap())
        .collect()
}

fn path_bearings(sim: &Simulation, count: usize) -> Vec<f32> {
    let positions = path_positions(sim, count);
    let (first, last) = (positions[0], positions[count - 1]);
    let (dx, dy) = (last.x - first.x, last.y - first.y);
    positions
        .windows(2)
        .map(|pair| {
            let (x, y) = (pair[1].x - pair[0].x, pair[1].y - pair[0].y);
            let along = x * dx + y * dy;
            assert!(along > 0., "path backtracks at tick {}", sim.tick);
            libm::atan2f(y * dx - x * dy, along)
        })
        .collect()
}

fn assert_straight_path(sim: &Simulation, count: usize) {
    let bearings = path_bearings(sim, count);
    assert!(
        bearings.iter().all(|bearing| bearing.abs() < 0.001),
        "bent path: {bearings:?}"
    );
}

#[test]
fn naturally_growing_zero_angle_path_stays_straight_through_four_divisions() {
    let mut s = growing_path(0., false);
    advance_to(&mut s, 50);
    assert_straight_path(&s, 4);
    advance_to(&mut s, 100);
    assert_straight_path(&s, 8);

    let mut restored = Simulation::new(999);
    restored.restore_checked(&s.export_state()).unwrap();
    advance_to(&mut s, 150);
    advance_to(&mut restored, 150);
    assert_straight_path(&s, 16);
    assert_eq!(s.export_state(), restored.export_state());
}

#[test]
fn mirrored_daughters_turn_a_tilted_straight_path_into_a_wave() {
    let angle = 10_f32.to_radians();
    let mut straight = growing_path(angle, false);
    let mut mirrored = growing_path(angle, true);
    for (tick, count) in [(50, 4), (100, 8), (150, 16)] {
        advance_to(&mut straight, tick);
        assert_straight_path(&straight, count);
        advance_to(&mut mirrored, tick);
        let bearings = path_bearings(&mirrored, count);
        let low = bearings.iter().copied().fold(f32::INFINITY, f32::min);
        let high = bearings.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (high - low - 2. * angle).abs() < 0.001,
            "tick {tick}: {bearings:?}"
        );
        assert!(
            bearings
                .iter()
                .all(|b| (b - low).abs() < 0.001 || (b - high).abs() < 0.001)
        );
        let changes = bearings
            .windows(2)
            .filter(|pair| (pair[1] - pair[0]).abs() > angle)
            .count();
        assert!(changes >= count / 2, "tick {tick}: {bearings:?}");
    }
}

fn right_angle_growth() -> Simulation {
    let mut s = quiet_world();
    let genes = Genome::from_modes(vec![Mode {
        threshold: 1.,
        angle: std::f32::consts::FRAC_PI_2,
        speed: 0.,
        bind_children: true,
        keep_a: true,
        keep_b: true,
        ..Mode::default()
    }])
    .values();
    let founder = s.spawn(500., 500., 0, &genes).unwrap();
    s.world.get::<&mut Cell>(entity(&s, founder)).unwrap().mass = 10.;
    mature_all(&mut s);
    s.advance(1);
    assert_eq!(s.bonds, BTreeSet::from([(2, 3)]));
    s
}

fn assert_four_cell_square(s: &Simulation) {
    assert_eq!(s.world.len(), 4);
    assert_eq!(s.bonds, BTreeSet::from([(4, 5), (4, 7), (5, 6), (6, 7)]));
    let corners = [4, 7, 6, 5].map(|id| *s.world.get::<&Position>(entity(s, id)).unwrap());
    let edges: [(f32, f32); 4] = std::array::from_fn(|i| {
        let from = corners[i];
        let to = corners[(i + 1) % 4];
        (to.x - from.x, to.y - from.y)
    });
    let lengths = edges.map(|(x, y)| libm::hypotf(x, y));
    for i in 0..4 {
        let next = (i + 1) % 4;
        let (x, y) = edges[i];
        let (nx, ny) = edges[next];
        assert!(
            (lengths[i] - lengths[0]).abs() < 0.05 * lengths[0],
            "square sides have unequal lengths: {lengths:?}"
        );
        assert!(
            (x * nx + y * ny).abs() < 0.05 * lengths[i] * lengths[next],
            "adjacent square sides are not perpendicular: {edges:?}"
        );
    }
}

#[test]
fn repeated_right_angle_division_forms_a_square_then_an_eight_cell_ring() {
    let mut s = right_angle_growth();
    let center_clearance = |sim: &mut Simulation| {
        let cells = sim.snapshot();
        let count = (cells.len() / SNAPSHOT_STRIDE) as f32;
        let rows = cells.chunks_exact(SNAPSHOT_STRIDE);
        let cx = rows.clone().map(|row| row[1]).sum::<f32>() / count;
        let cy = rows.clone().map(|row| row[2]).sum::<f32>() / count;
        rows.map(|row| libm::hypotf(row[1] - cx, row[2] - cy) - row[3])
            .fold(f32::INFINITY, f32::min)
    };

    mature_all(&mut s);
    s.advance(1);
    assert_four_cell_square(&s);
    assert!(center_clearance(&mut s) > 2., "square center is occupied");

    mature_all(&mut s);
    s.advance(1);
    assert_eq!(s.world.len(), 8);
    assert_eq!(
        s.bonds,
        BTreeSet::from([
            (8, 9),
            (8, 15),
            (9, 10),
            (10, 11),
            (11, 12),
            (12, 13),
            (13, 14),
            (14, 15),
        ]),
        "the ring should have one continuous perimeter without cross-links"
    );
    assert!(center_clearance(&mut s) > 2., "ring center is occupied");
}

#[test]
fn right_angle_division_forms_the_same_square_on_separate_ticks() {
    for (first, second) in [(2, 3), (3, 2)] {
        let mut s = right_angle_growth();
        s.world.get::<&mut Cell>(entity(&s, first)).unwrap().age = MIN_DIVISION_AGE;
        s.advance(1);
        assert_eq!(s.world.len(), 3);
        assert_eq!(s.bonds, BTreeSet::from([(second, 4), (second, 5), (4, 5)]));

        s.world.get::<&mut Cell>(entity(&s, second)).unwrap().age = MIN_DIVISION_AGE;
        s.advance(1);
        assert_four_cell_square(&s);
    }
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
    g[24 + 2] = 0.;
    g[24 + 3] = 2.;
    g[24 + 11] = 1.;
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
    assert_eq!(s.links(), vec![2, 5]);
    assert_eq!(s.cell_mode(2), 1); // non-dividing flagellum survives above its threshold
    assert_eq!(s.cell_mode(4), 0); // detached photo offspring
    assert_eq!(s.cell_mode(5), 2); // nearer phagocyte inherits the bond
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
            s.advance(40);
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
    assert!((mass - 1.).abs() < 0.02, "daughter mass {mass}");
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
    assert!(c.cooldown > 0. && c.cooldown <= 0.2);
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
fn soft_population_limit_accepts_spawns_and_culls_oldest() {
    let mut s = quiet_world();
    s.settings.aging = false;
    s.population_limit = 2;
    let mut g = modern_genes(1);
    g[8] = 0.; // Non-dividing, so only population pressure removes cells.
    let old = s.spawn(100., 100., 0, &g).unwrap();
    s.world.get::<&mut Cell>(entity(&s, old)).unwrap().age = 100.;
    let young = s.spawn(200., 100., 0, &g).unwrap();
    let extra = s.spawn(300., 100., 0, &g).unwrap();
    // The soft limit does not reject placement above it.
    assert_eq!(s.world.len(), 3);
    s.advance(1);
    assert_eq!(s.world.len(), 2);
    let ids: Vec<_> = s
        .snapshot()
        .chunks_exact(SNAPSHOT_STRIDE)
        .map(|row| row[0] as u32)
        .collect();
    assert!(!ids.contains(&old), "the oldest cell dies first");
    assert!(ids.contains(&young) && ids.contains(&extra));
    assert_eq!(s.stats()[3], 1.);
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
            assert!((after.x - before.x - libm::cosf(after.angle) * speed * STEP).abs() < 0.00002);
            assert!((after.y - before.y - libm::sinf(after.angle) * speed * STEP).abs() < 0.00002);
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
    assert_eq!(first.cells.len(), 29);
    assert_eq!(first.cells[0], a as f32);
    assert_eq!(first.cells[11], 1.);
    assert_eq!(first.links, vec![a, b]);
    assert_eq!(&first.cells[12..15], &[-1., 0., INITIAL_NITROGEN]);
    assert_eq!(&first.cells[15..23], &[0.; 8]);
    assert_eq!(&first.cells[23..29], &[0., 0., -1., 0., 0., 0.]);
    assert_eq!(first.terrain.len(), 8 * 8 * 6);
    assert_eq!(first.terrain[4], s.tiles[0].nitrates);
    assert_eq!(first.terrain[5], s.tiles[0].covered_food);

    let second = s.chunk_snapshot(1, 0, 512.);
    assert_eq!(second.cells.len(), 29);
    assert_eq!(second.cells[0], b as f32);
    assert_eq!(second.cells[11], 1.);
    assert!(second.links.is_empty());
}

#[test]
fn arena_owner_scores_integrate_mass_with_type_weights() {
    let mut s = Simulation::new(7);
    s.set_track_owner_scores(true);
    let id = s.spawn_owned(300., 300., 0, &modern_genes(1), 7).unwrap();
    for (_, cell) in s.world.query_mut::<&mut Cell>() {
        if cell.id == id {
            cell.mass = 4.;
        }
    }
    s.accumulate_owner_scores();
    // Photocytes count at 0.25x over one 1/10 s fixed step.
    assert!((s.owner_score(7) - 4. * 0.25 * STEP as f64).abs() < 1e-6);
    assert_eq!(s.owner_score(8), 0.);
    s.set_track_owner_scores(false);
    assert_eq!(s.owner_score(7), 0.);
}
