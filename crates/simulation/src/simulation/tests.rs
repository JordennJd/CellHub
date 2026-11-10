use super::Simulation;

use crate::components::{Cell, LIMIT, Position, SIZE};
use crate::genome::MODE_STRIDE;
use crate::terrain::tile_index;
use hecs::Entity;
use std::collections::BTreeSet;

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
    s.advance(1);
    assert_eq!(s.links(), vec![2, 3]);
    let flagellum = entity(&s, 2);
    let phagocyte = entity(&s, 3);
    s.world.get::<&mut Cell>(flagellum).unwrap().mass = 10.;
    s.world.get::<&mut Cell>(phagocyte).unwrap().mass = 3.;
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
    let distance = (rows[1] - rows[11]).hypot(rows[2] - rows[12]);
    assert!((distance - rows[3] - rows[13] - 2.).abs() < 0.01);
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
    assert_eq!(s.genome(recipient).len(), 40 * MODE_STRIDE);
    let mut donor = modern_genes(1);
    donor[0] = 3.;
    s.spawn(505., 500., 0, &donor).unwrap();
    s.advance(2);
    assert_eq!(s.genome(recipient).len(), MODE_STRIDE);
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
        for row in snapshot.as_chunks::<10>().0 {
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
    s.advance(1);
    let rows = s.snapshot();
    assert_eq!(rows.len(), 20);
    let mass = rows[5] + rows[15];
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
    for row in s.snapshot().as_chunks::<10>().0 {
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
    s.advance(1);
    assert_eq!(s.world.len() as usize, LIMIT);
}
#[test]
fn chunk_snapshot_exports_cells_links_and_terrain() {
    let mut s = Simulation::new(5);
    let g = modern_genes(1);
    let a = s.spawn(100., 100., 0, &g).unwrap();
    let b = s.spawn(600., 100., 0, &g).unwrap();
    s.bonds.insert((a, b));

    let first = s.chunk_snapshot(0, 0, 512.);
    assert_eq!(first.cells.len(), 12);
    assert_eq!(first.cells[0], a as f32);
    assert_eq!(first.cells[11], 1.);
    assert_eq!(first.links, vec![a, b]);
    assert_eq!(first.terrain.len(), 8 * 8 * 4);

    let second = s.chunk_snapshot(1, 0, 512.);
    assert_eq!(second.cells.len(), 12);
    assert_eq!(second.cells[0], b as f32);
    assert!(second.links.is_empty());
}
