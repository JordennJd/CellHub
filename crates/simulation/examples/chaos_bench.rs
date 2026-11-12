//! Native chaos-mode load benchmark.
//!
//! ```text
//! cargo run --release -p cellhub-simulation --example chaos_bench -- growth [ticks] [seed] [width]
//! cargo run --release -p cellhub-simulation --example chaos_bench -- scale [max] [step] [ticks]
//! cargo run --release -p cellhub-simulation --example chaos_bench -- steady [population] [ticks] [grid|colony|piles|wide]
//! ```
//!
//! `growth` starts with one dividing photocyte and follows the bloom all the
//! way to the 20 000-cell cap, so the tick distribution is reported for the
//! population the simulation actually reaches. `scale` and `steady` fill a
//! chaos dish with non-dividing photocytes to isolate the per-tick cost, which
//! is useful when the ecology plateaus before the hard cap.
//!
//! Rayon parallelises contact discovery and independent islands on native
//! targets. Set `RAYON_NUM_THREADS=1` for the serial baseline.

use cellhub_simulation::Simulation;
use std::hint::black_box;
use std::time::Instant;

const LIMIT: usize = 20_000;
const TICK_MS: f64 = 1000. / 30.;
const SEED: u32 = 73_421;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("growth");
    let number = |index: usize, fallback: usize| {
        args.get(index)
            .and_then(|value| value.parse().ok())
            .unwrap_or(fallback)
    };
    match command {
        "growth" => growth(
            number(1, 24_000),
            number(2, SEED as usize) as u32,
            args.get(3).and_then(|value| value.parse().ok()),
        ),
        "scale" => scale(number(1, LIMIT), number(2, 2_000), number(3, 60)),
        "steady" => steady(
            number(1, LIMIT),
            number(2, 60),
            args.get(3).map(String::as_str).unwrap_or("grid"),
        ),
        other => {
            eprintln!("unknown benchmark: {other}");
            std::process::exit(2);
        }
    }
}

/// A single dividing photocyte in the legacy 12-number row format.
fn dividing_photocyte() -> Vec<f32> {
    vec![0., 2.6, 0., 0., 0.5, 0., 0., 360., 1., 0., 1., 0.]
}

/// A v3 genome with one non-dividing photocyte.
fn passive_photocyte() -> Vec<f32> {
    vec![
        -3., 1., // header: version, mode count
        0., 2.6, 0., 0., 0.5, 0., 0., 1000., 0., 0., 0., 0., 0., 0., 0., 0., -1., -1., -1.,
    ]
}

fn chaos(seed: u32, width: Option<f32>) -> Simulation {
    let mut sim = Simulation::new(seed);
    sim.start_chaos_with_organisms(seed, &passive_photocyte(), &[], width)
        .expect("valid chaos world");
    sim.sterilize();
    sim.configure_chaos(false, 10, true)
        .expect("valid scheduler");
    sim
}

/// A neutral light level keeps passive cells near their spawn mass.
fn neutral(sim: &mut Simulation) {
    sim.configure(
        0.18,
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
    .expect("valid environment");
}

fn growth(ticks: usize, seed: u32, width: Option<f32>) {
    let mut sim = chaos(seed, width);
    let width = sim.world_width();
    let height = sim.world_size();
    sim.configure(
        1.5,
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
    .expect("valid environment");
    sim.spawn_checked(width / 2., height / 2., 0, &dividing_photocyte())
        .expect("spawn photocyte");
    let mut samples: Vec<(usize, f64)> = Vec::with_capacity(ticks);
    let mut milestones = [None; 5];
    let thresholds = [1_000, 5_000, 10_000, 15_000, 20_000];
    let mut first_slow = [None; 3];
    let slow_thresholds = [TICK_MS, TICK_MS / 2., TICK_MS / 4.];
    let mut capped = false;
    let mut cap_samples: Vec<f64> = Vec::new();
    let mut peak = 0;
    let started = Instant::now();
    for tick in 0..ticks {
        let population = sim.stats()[1] as usize;
        peak = peak.max(population);
        if population >= LIMIT && !capped {
            // Stop growth at the cap so the measurement reflects a stable
            // colony instead of unbounded photocyte mass.
            neutral(&mut sim);
            capped = true;
            println!(
                "cap reached at tick {tick}, {:.1} s wall",
                started.elapsed().as_secs_f64()
            );
        }
        for (index, threshold) in thresholds.iter().enumerate() {
            if milestones[index].is_none() && population >= *threshold {
                milestones[index] = Some((tick, started.elapsed().as_secs_f64()));
            }
        }
        let sample = Instant::now();
        sim.advance(1);
        let ms = sample.elapsed().as_secs_f64() * 1e3;
        let population = sim.stats()[1] as usize;
        let at_cap = capped && cap_samples.len() < 120;
        if at_cap {
            cap_samples.push(ms);
        }
        samples.push((population, ms));
        for (index, limit) in slow_thresholds.iter().enumerate() {
            if first_slow[index].is_none() && ms > *limit && tick >= 120 {
                first_slow[index] = Some((tick, population, ms));
            }
        }
        if tick % 600 == 0 || tick + 1 == ticks {
            println!(
                "tick {tick:>6} pop {population:>6} {:>7.2} ms threads {}",
                median(&window(&samples, 120)),
                rayon::current_num_threads(),
            );
        }
    }
    println!("threads {}", rayon::current_num_threads());
    for (index, threshold) in thresholds.iter().enumerate() {
        match milestones[index] {
            Some((tick, seconds)) => {
                let bin = bin_samples(&samples, *threshold, threshold + 2_000);
                println!(
                    "reached {threshold:>6} at tick {tick:>6} ({seconds:.1} s) p50 {:.2} ms p95 {:.2} ms",
                    percentile(&bin, 0.5),
                    percentile(&bin, 0.95),
                );
            }
            None => println!("never reached {threshold}"),
        }
    }
    for (index, limit) in slow_thresholds.iter().enumerate() {
        match first_slow[index] {
            Some((tick, population, ms)) => {
                println!("first tick over {limit:.1} ms: tick {tick}, pop {population}, {ms:.2} ms")
            }
            None => println!("no tick over {limit:.1} ms"),
        }
    }
    for start in (0..LIMIT).step_by(2_000) {
        let bin = bin_samples(&samples, start, start + 2_000);
        if !bin.is_empty() {
            println!(
                "pop {start:>5}-{:>5}: samples {:>4} p50 {:>7.2} ms p95 {:>7.2} ms p99 {:>7.2} ms",
                start + 2_000,
                bin.len(),
                percentile(&bin, 0.5),
                percentile(&bin, 0.95),
                percentile(&bin, 0.99),
            );
        }
    }
    let mut cap_samples = cap_samples;
    cap_samples.sort_by(f64::total_cmp);
    println!(
        "peak population {peak}; first 120 ticks at cap: p50 {:.2} ms p95 {:.2} ms max {:.2} ms",
        percentile(&cap_samples, 0.5),
        percentile(&cap_samples, 0.95),
        cap_samples.last().copied().unwrap_or(0.),
    );
    let tail = bin_samples(&samples, LIMIT.saturating_sub(2_000), LIMIT);
    println!(
        "tail 18000-20000: samples {} p50 {:.2} ms p95 {:.2} ms max {:.2} ms",
        tail.len(),
        percentile(&tail, 0.5),
        percentile(&tail, 0.95),
        tail.last().copied().unwrap_or(0.),
    );
    let frame = frame_bundle(&mut sim);
    println!(
        "final population {} frame bundle {frame:.2} ms",
        sim.stats()[1]
    );
}

fn scale(max: usize, step: usize, ticks: usize) {
    let step = step.max(1);
    for population in (step..=max).step_by(step) {
        let (p50, p95, frame, chunk) = steady_measure(population, ticks, "grid");
        println!(
            "pop {population:>6} threads {:>2} p50 {p50:>7.2} ms p95 {p95:>7.2} ms frame {frame:>6.2} ms chunk {chunk:>5.2} ms",
            rayon::current_num_threads(),
        );
    }
}

fn steady(population: usize, ticks: usize, layout: &str) {
    let (p50, p95, frame, chunk) = steady_measure(population, ticks, layout);
    println!(
        "layout {layout} pop {population} threads {} p50 {p50:.2} ms p95 {p95:.2} ms frame {frame:.2} ms chunk {chunk:.2} ms",
        rayon::current_num_threads(),
    );
}

fn steady_measure(population: usize, ticks: usize, layout: &str) -> (f64, f64, f64, f64) {
    // `wide` keeps the old 6144 × 6144 arena area (12288 × 3072) for a
    // density comparison with the halved dish.
    let width_option = (layout == "wide").then_some(12_288.);
    let mut sim = chaos(SEED, width_option);
    neutral(&mut sim);
    let width = sim.world_width();
    let height = sim.world_size();
    let genes = passive_photocyte();
    let margin = 48.;
    if layout == "piles" {
        // Many independent dense islands: the shape where the parallel island
        // solver is expected to scale.
        let groups = 100usize;
        let per_group = population.div_ceil(groups);
        let spacing = 7.;
        let mut index = 0;
        for group in 0..groups {
            if index == population {
                break;
            }
            let gx = width * (group % 10) as f32 / 10. + width / 20.;
            let gy = height * (group / 10) as f32 / 10. + height / 20.;
            let side = (per_group as f32).sqrt().ceil() as usize;
            for slot in 0..per_group {
                if index == population {
                    break;
                }
                let x = gx + (slot % side) as f32 * spacing - side as f32 * spacing / 2.;
                let y = gy + (slot / side) as f32 * spacing - side as f32 * spacing / 2.;
                sim.spawn_checked(x, y, 0, &genes).expect("spawn");
                index += 1;
            }
        }
    } else if layout == "colony" {
        // Hexagonal packing at roughly one cell diameter.
        let spacing = 18.;
        let columns = (width / spacing) as usize;
        let mut index = 0;
        let mut row = 0;
        'outer: while index < population {
            let y = height / 2. + (row as f32 - columns as f32 / 4.) * spacing;
            let offset = if row % 2 == 0 { 0. } else { spacing / 2. };
            for column in 0..columns {
                if index == population {
                    break 'outer;
                }
                let x = width / 2. + (column as f32 - columns as f32 / 2.) * spacing + offset;
                if x < margin || x > width - margin {
                    continue;
                }
                sim.spawn_checked(x, y, 0, &genes).expect("spawn");
                index += 1;
            }
            row += 1;
        }
    } else {
        let columns = ((population as f64 * width as f64 / height as f64)
            .sqrt()
            .ceil() as usize)
            .max(1);
        let rows = population.div_ceil(columns);
        for i in 0..population {
            let x = margin + (i % columns) as f32 * (width - 2. * margin) / columns as f32;
            let y = margin + (i / columns) as f32 * (height - 2. * margin) / rows as f32;
            sim.spawn_checked(x, y, 0, &genes).expect("spawn");
        }
    }
    // Warm up the spatial grid and allocator.
    sim.advance(10);
    let mut samples = Vec::with_capacity(ticks);
    for _ in 0..ticks {
        let started = Instant::now();
        sim.advance(1);
        samples.push(started.elapsed().as_secs_f64() * 1e3);
    }
    samples.sort_by(f64::total_cmp);
    let p50 = percentile(&samples, 0.5);
    let p95 = percentile(&samples, 0.95);
    let frame = frame_bundle(&mut sim);
    let chunk = chunk_snapshot(&sim);
    (p50, p95, frame, chunk)
}

/// One authoritative chunk, as the server sends it every tick.
fn chunk_snapshot(sim: &Simulation) -> f64 {
    let started = Instant::now();
    let snapshot = sim.chunk_snapshot(3, 3, 512.);
    black_box((
        snapshot.cells.len(),
        snapshot.links.len(),
        snapshot.terrain.len(),
    ));
    started.elapsed().as_secs_f64() * 1e3
}

/// Everything the local worker serialises for one rendered frame.
fn frame_bundle(sim: &mut Simulation) -> f64 {
    let started = Instant::now();
    let cells = sim.snapshot();
    let links = sim.links();
    let terrain = sim.terrain();
    let details = sim.cell_details();
    let nitrogen = sim.nitrogen_reserves();
    let nitrates = sim.nitrates();
    let covered = sim.covered_food();
    let info = sim.chaos_info();
    let history = sim.chaos_history();
    let events = sim.chaos_events();
    black_box((
        cells.len(),
        links.len(),
        terrain.len(),
        details.len(),
        nitrogen.len(),
        nitrates.len(),
        covered.len(),
        info.len(),
        history.len(),
        events.len(),
    ));
    started.elapsed().as_secs_f64() * 1e3
}

fn bin_samples(samples: &[(usize, f64)], from: usize, to: usize) -> Vec<f64> {
    let mut bin: Vec<f64> = samples
        .iter()
        .filter(|(population, _)| *population >= from && *population < to)
        .map(|(_, ms)| *ms)
        .collect();
    bin.sort_by(f64::total_cmp);
    bin
}

fn window(samples: &[(usize, f64)], count: usize) -> Vec<f64> {
    let mut values: Vec<f64> = samples
        .iter()
        .rev()
        .take(count)
        .map(|(_, ms)| *ms)
        .collect();
    values.sort_by(f64::total_cmp);
    values
}

fn median(values: &[f64]) -> f64 {
    percentile(values, 0.5)
}

fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.;
    }
    values[((values.len() - 1) as f64 * p) as usize]
}
