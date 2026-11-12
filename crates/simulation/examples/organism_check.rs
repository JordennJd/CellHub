//! Audit the real frontend catalog; input JSON is exported by scripts/check-organisms.mjs.
//! Usage: organism_check <catalog.json> [seconds] [seed,seed,...]
use cellhub_simulation::Simulation;
use serde::Deserialize;
use serde_json::json;
#[derive(Deserialize)]
struct Catalog {
    pool: Vec<f32>,
    entries: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    id: String,
    span: usize,
    initial: usize,
    genome: Vec<f32>,
}
fn owned(sim: &Simulation) -> Vec<Vec<f32>> {
    let mut rows = Vec::new();
    for y in 0..6 {
        for x in 0..6 {
            rows.extend(
                sim.chunk_snapshot(x, y, 512.)
                    .cells
                    .chunks_exact(29)
                    .filter(|r| r[28] == 1.)
                    .map(|r| r.to_vec()),
            );
        }
    }
    rows
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let catalog: Catalog =
        serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    let seconds: usize = args.get(2).map_or(120, |s| s.parse().unwrap());
    let seeds: Vec<u32> = args.get(3).map_or_else(
        || vec![42, 73421, 91],
        |s| s.split(',').map(|v| v.parse().unwrap()).collect(),
    );
    let mut results = Vec::new();
    for entry in &catalog.entries {
        for scenario in ["nursery", "chaos"] {
            for &seed in &seeds {
                let mut sim = Simulation::new(seed);
                if scenario == "chaos" {
                    sim.start_chaos_native(seed, &catalog.pool, &[]).unwrap();
                    sim.configure_chaos(false, 10, true).unwrap();
                    sim.sterilize();
                } else {
                    sim.configure(
                        1.,
                        1.,
                        0.,
                        1.,
                        false,
                        false,
                        Some(true),
                        Some(0.),
                        Some(0.),
                        Some(1.),
                    )
                    .unwrap();
                }
                let tiles = sim.terrain();
                let kind = entry.genome[2 + entry.initial * 102] as usize;
                let tile = (0..48 * 48)
                    .filter(|&i| {
                        let x = i % 48;
                        let y = i / 48;
                        x > 3 && x < 44 && y > 3 && y < 44 && tiles[i * 4 + 1] < 0.1
                    })
                    .max_by(|&a, &b| {
                        let score = |i: usize| {
                            if kind == 1 {
                                tiles[i * 4 + 2]
                            } else {
                                tiles[i * 4]
                            }
                        };
                        score(a).total_cmp(&score(b))
                    })
                    .unwrap();
                let (x, y) = (
                    (tile % 48) as f32 * 64. + 32.,
                    (tile / 48) as f32 * 64. + 32.,
                );
                sim.spawn_owned(x, y, entry.initial, &entry.genome, 1)
                    .unwrap();
                // Predators need actual prey; the same finite prey patch is used in all runs.
                if kind == 5 {
                    for i in 0..48 {
                        let theta = i as f32 * 0.55;
                        let r = 35. + i as f32 * 2.;
                        sim.spawn_owned(
                            x + theta.cos() * r,
                            y + theta.sin() * r,
                            0,
                            &catalog.pool,
                            2,
                        )
                        .unwrap();
                    }
                }
                let mut checkpoints = Vec::new();
                let mut peak = 1;
                let mut generation = 0;
                let mut roles = std::collections::BTreeSet::new();
                for second in 1..=seconds {
                    sim.advance(10);
                    if second % 10 == 0 || second == seconds {
                        let rows = owned(&sim);
                        peak = peak.max(rows.len());
                        for r in &rows {
                            generation = generation.max(r[7] as u32);
                            roles.insert(r[4] as u32);
                        }
                        checkpoints.push(json!({"seconds":second,"alive":rows.len(),"bonds":sim.links().len()/2}));
                        if rows.is_empty() {
                            break;
                        }
                    }
                }
                let rows = owned(&sim);
                let result = json!({"id":entry.id,"span":entry.span,"scenario":scenario,"seed":seed,"alive":rows.len(),"peak":peak,"generation":generation,"roles":roles,"checkpoints":checkpoints});
                eprintln!(
                    "{} {} {} alive={} gen={}",
                    entry.id,
                    scenario,
                    seed,
                    rows.len(),
                    generation
                );
                results.push(result);
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&results).unwrap());
}
