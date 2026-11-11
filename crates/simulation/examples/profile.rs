//! Reproducible native benchmark and native/WASM parity fixture.
#[cfg(not(target_arch = "wasm32"))]
use cellhub_simulation::Simulation;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let population: usize = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(10_000);
    let ticks: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(120);
    let layout = args.get(3).map(String::as_str).unwrap_or("grid");
    assert!((1..=20_000).contains(&population) && ticks > 0);
    let mut simulation = Simulation::new(73421);
    // Legacy ABI, eight photocyte modes, with motion, radiation and division.
    let genes = [0., 2.4, 0., 0., 0.5, 12., 0.8, 240.].repeat(8);
    for i in 0..population {
        let (x, y) = match layout {
            "grid" => (30. + (i % 140) as f32 * 21.5, 30. + (i / 140) as f32 * 21.),
            "colony" => (650. + (i % 140) as f32 * 12., 650. + (i / 140) as f32 * 12.),
            "piles" => {
                let group = i / 200;
                (
                    160. + (group % 10) as f32 * 280.,
                    160. + (group / 10) as f32 * 280.,
                )
            }
            _ => panic!("Unknown layout: {layout}"),
        };
        simulation.spawn_checked(x, y, 0, &genes).unwrap();
    }
    let mut samples = Vec::with_capacity(ticks);
    for _ in 0..ticks {
        let started = Instant::now();
        simulation.advance(1);
        samples.push(started.elapsed().as_secs_f64() * 1000.);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "layout={layout} population={} ticks={} threads={} p50_ms={:.3} p95_ms={:.3} max_ms={:.3} checksum={}",
        simulation.stats()[1],
        ticks,
        rayon::current_num_threads(),
        samples[ticks / 2],
        samples[(ticks * 95 / 100).min(ticks - 1)],
        samples[ticks - 1],
        simulation.checksum()
    );
}

#[cfg(target_arch = "wasm32")]
fn main() {}
