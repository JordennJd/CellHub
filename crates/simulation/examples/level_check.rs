//! Check one campaign demo against the real simulation.
//!
//! Usage: `cargo run --release -p cellhub-simulation --example level_check -- 26.6`
//!
//! Diagnostic environment variables:
//! - `LEVEL_VERBOSE=1` — print objective progress once per `LEVEL_PERIOD` seconds.
//! - `LEVEL_TRACE=1` — print every simulated event (spawns, deaths, births,
//!   infections, objective transitions).
//! - `LEVEL_CENSUS=<seconds>` — print an aggregated substrate census at that
//!   interval; `LEVEL_CENSUS=1` also dumps it once on failure.
//! - `LEVEL_CELLS=1` — on failure, print every live cell (can be large).
//! - `LEVEL_LIMIT=<seconds>` — override the simulated time budget.

use cellhub_simulation::Simulation;
use serde_json::Value;
use std::path::PathBuf;

fn levels_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../levels")
}

fn env_number(name: &str) -> Option<f64> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
}

fn main() {
    let id = std::env::args().nth(1).unwrap_or_else(|| "1".to_owned());
    let path = std::env::var_os("LEVEL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| levels_dir().join(format!("{id}.json")));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("не удалось прочитать {}: {e}", path.display());
        std::process::exit(2);
    });
    let level: Value = serde_json::from_str(&text).expect("invalid level json");
    let limit = env_number("LEVEL_LIMIT")
        .unwrap_or_else(|| {
            level["test"]["maxSeconds"]
                .as_f64()
                .or_else(|| level["conditions"]["max-time"].as_f64().map(|v| v + 30.))
                .unwrap_or(600.)
        })
        .min(3600.);
    let verbose = std::env::var_os("LEVEL_VERBOSE").is_some();
    let trace = std::env::var_os("LEVEL_TRACE").is_some();
    let census_interval = env_number("LEVEL_CENSUS").unwrap_or(0.);
    let cells_on_failure = std::env::var_os("LEVEL_CELLS").is_some();
    let period = env_number("LEVEL_PERIOD").unwrap_or(10.);

    let mut simulation = Simulation::new(0xCE11AB);
    simulation.load_level_text(&text).expect("level rejected");
    simulation.start_demo_track().expect("demo unavailable");
    let mut elapsed = 0.;
    let mut last_period = -period;
    let mut last_census = 0.;
    let mut seen_events = 0usize;
    loop {
        simulation.advance(30);
        elapsed += 1.;
        if trace {
            seen_events = print_new_events(&simulation, seen_events);
        }
        let status: Value =
            serde_json::from_str(&simulation.level_status_json()).expect("invalid status");
        if let Some(error) = status["demoError"].as_str() {
            eprintln!("{id}: DEMO ERROR: {error}");
            std::process::exit(1);
        }
        match status["state"].as_str().unwrap_or("") {
            "won" => {
                println!(
                    "{id}: WON at {elapsed:.0}s actions={} inserted={} removed={}",
                    status["demoCursor"], status["inserted"], status["removed"]
                );
                return;
            }
            "lost" => {
                eprintln!("{id}: LOST at {elapsed:.0}s");
                fail(&simulation, id.as_str(), &status, cells_on_failure);
                std::process::exit(1);
            }
            _ => {}
        }
        if verbose && elapsed - last_period >= period {
            last_period = elapsed;
            report(&status);
        }
        if census_interval > 0. && elapsed - last_census >= census_interval {
            last_census = elapsed;
            print_census(&simulation);
        }
        if elapsed >= limit {
            eprintln!("{id}: still running after {limit:.0}s");
            fail(&simulation, id.as_str(), &status, cells_on_failure);
            std::process::exit(1);
        }
    }
}

fn fail(simulation: &Simulation, id: &str, status: &Value, cells: bool) {
    report(status);
    println!("substrate census:");
    print_census(simulation);
    println!("last events:");
    let events = events(simulation);
    for event in events.iter().rev().take(40).rev() {
        print_event(event);
    }
    if cells {
        println!("cells:");
        println!("{}", simulation.level_cells_json());
    }
    let _ = id;
}

fn events(simulation: &Simulation) -> Vec<Value> {
    serde_json::from_str(&simulation.level_events_json()).unwrap_or_default()
}

fn print_new_events(simulation: &Simulation, seen: usize) -> usize {
    let events = events(simulation);
    let start = if events.len() < seen { 0 } else { seen };
    for event in &events[start..] {
        print_event(event);
    }
    events.len()
}

fn print_event(event: &Value) {
    let mut line = format!(
        "  t={:>6.2}s tick={:<8} {:<10}",
        event["time"].as_f64().unwrap_or(0.),
        event["tick"].as_u64().unwrap_or(0),
        event["kind"].as_str().unwrap_or("?"),
    );
    for field in ["cell", "parent", "class", "mode", "mass", "nitrogen"] {
        if let Some(value) = event.get(field).and_then(|value| value.as_f64()) {
            line.push_str(&format!(" {field}={value:.3}"));
        }
    }
    if let (Some(x), Some(y)) = (event["x"].as_f64(), event["y"].as_f64()) {
        line.push_str(&format!(" at=({x:.3},{y:.3})"));
    }
    if let Some(color) = event["color"].as_array() {
        let color = color
            .iter()
            .filter_map(|value| value.as_f64())
            .map(|value| format!("{value:.2}"))
            .collect::<Vec<_>>()
            .join(",");
        line.push_str(&format!(" color=({color})"));
    }
    if let Some(detail) = event["detail"].as_str() {
        line.push_str(&format!("  {detail}"));
    }
    println!("{line}");
}

fn print_census(simulation: &Simulation) {
    let census: Value =
        serde_json::from_str(&simulation.level_census_json()).expect("invalid census");
    let status = &census["status"];
    let counts = &status["counts"];
    let substrate = &census["substrate"];
    println!(
        "  t={:.1}s state={} cells={} user={} level={} infected={} hybrid={} black={} red={} yellow={} bonds={}",
        status["elapsed"].as_f64().unwrap_or(0.),
        status["state"].as_str().unwrap_or("?"),
        counts["total"],
        counts["user"],
        counts["level"],
        counts["infected"],
        counts["hybrid"],
        counts["black"],
        counts["red"],
        counts["yellow"],
        census["bonds"],
    );
    println!(
        "  food={:.1}/{:.1} nitrates={:.1}/{:.1} covered={:.1} light={:.3} radiation={:.4} biomes={}",
        substrate["food"].as_f64().unwrap_or(0.),
        substrate["capacity"].as_f64().unwrap_or(0.),
        substrate["nitrates"].as_f64().unwrap_or(0.),
        substrate["nitrateCapacity"].as_f64().unwrap_or(0.),
        substrate["coveredFood"].as_f64().unwrap_or(0.),
        substrate["light"].as_f64().unwrap_or(0.),
        substrate["radiation"].as_f64().unwrap_or(0.),
        substrate["biomes"],
    );
    if let Some(modes) = census["modes"].as_array() {
        for mode in modes {
            println!(
                "  mode class={} mode={} count={} rank={} color=({:.2},{:.2},{:.2}) mass={:.2}..{:.2} avg={:.2} centroid=({:.3},{:.3}) gen={} muts={}",
                mode["class"],
                mode["mode"],
                mode["count"],
                mode["rank"].as_str().unwrap_or(""),
                mode["color"][0].as_f64().unwrap_or(0.),
                mode["color"][1].as_f64().unwrap_or(0.),
                mode["color"][2].as_f64().unwrap_or(0.),
                mode["massMin"].as_f64().unwrap_or(0.),
                mode["massMax"].as_f64().unwrap_or(0.),
                mode["massAvg"].as_f64().unwrap_or(0.),
                mode["centroid"][0].as_f64().unwrap_or(0.),
                mode["centroid"][1].as_f64().unwrap_or(0.),
                mode["generationMax"],
                mode["mutationsMax"],
            );
        }
    }
    if let Some(conditions) = status["conditions"].as_array() {
        for condition in conditions {
            println!(
                "  {:<28} {:>8} / {:<8} {}",
                condition["key"].as_str().unwrap_or(""),
                condition["current"],
                condition["target"],
                if condition["met"].as_bool().unwrap_or(false) {
                    "ok"
                } else {
                    "FAIL"
                }
            );
        }
    }
}

fn report(status: &Value) {
    if let Some(conditions) = status["conditions"].as_array() {
        for condition in conditions {
            println!(
                "  {:<28} {:>8} / {:<8} {}",
                condition["key"].as_str().unwrap_or(""),
                condition["current"],
                condition["target"],
                if condition["met"].as_bool().unwrap_or(false) {
                    "ok"
                } else {
                    "FAIL"
                }
            );
        }
    }
}
