//! Native load benchmark: `cargo run --release -p cellhub-simulation --example bench`.
//!
//! Fills the world with a uniform grid of non-dividing cells and measures
//! fixed-step cost plus snapshot/terrain serialization cost.

use cellhub_simulation::{SIZE, Simulation};
use std::time::Instant;

fn main() {
    let count = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000usize);
    let ticks = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(60u32);
    let mut sim = Simulation::new(73421);
    let columns = (count as f64).sqrt().ceil() as usize;
    let rows = count.div_ceil(columns);
    let genome: Vec<f32> = (0..8)
        .flat_map(|i| {
            [
                (i % 6) as f32,
                8.,
                i as f32,
                i as f32,
                0.5,
                12.,
                0.8,
                1000.,
                1.,
                0.,
                0.,
                0.,
            ]
        })
        .collect();
    let spawn_start = Instant::now();
    for i in 0..count {
        let x = 20. + (i % columns) as f32 * (SIZE - 40.) / columns as f32;
        let y = 20. + (i / columns) as f32 * (SIZE - 40.) / rows as f32;
        sim.spawn(x, y, i % 6, &genome).unwrap();
    }
    println!(
        "spawn {count} cells: {:.1} ms, population {}",
        spawn_start.elapsed().as_secs_f64() * 1e3,
        sim.stats()[1]
    );
    let step_start = Instant::now();
    for _ in 0..ticks {
        sim.advance(1);
    }
    let step_ms = step_start.elapsed().as_secs_f64() * 1e3 / ticks as f64;
    let snap_start = Instant::now();
    let mut total = 0usize;
    for _ in 0..10 {
        let cells = sim.snapshot();
        let links = sim.links();
        let terrain = sim.terrain();
        total += cells.len() + links.len() + terrain.len();
    }
    let snap_ms = snap_start.elapsed().as_secs_f64() * 1e3 / 10.;
    println!("population {}", sim.stats()[1]);
    println!("step: {step_ms:.2} ms/tick ({:.1} Hz)", 1e3 / step_ms);
    println!("snapshot bundle: {snap_ms:.2} ms ({total} floats)");
}
