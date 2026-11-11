//! Check one campaign demo against the real simulation.
//!
//! Usage: `cargo run -p cellhub-simulation --example level_check -- 26.6`
//! Set `LEVEL_VERBOSE=1` to print progress once per second.

use cellhub_simulation::Simulation;
use serde_json::Value;
use std::path::PathBuf;

fn levels_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../levels")
}

fn main() {
    let id = std::env::args().nth(1).unwrap_or_else(|| "1".to_owned());
    let path = levels_dir().join(format!("{id}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("не удалось прочитать {}: {e}", path.display());
        std::process::exit(2);
    });
    let level: Value = serde_json::from_str(&text).expect("invalid level json");
    let limit = std::env::var("LEVEL_LIMIT")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or_else(|| {
            level["test"]["maxSeconds"]
                .as_f64()
                .or_else(|| level["conditions"]["max-time"].as_f64().map(|v| v + 30.))
                .unwrap_or(600.)
        })
        .min(3600.);
    let verbose = std::env::var_os("LEVEL_VERBOSE").is_some();
    let mut simulation = Simulation::new(0xCE11AB);
    simulation.load_level_text(&text).expect("level rejected");
    simulation.start_demo_track().expect("demo unavailable");
    let mut elapsed = 0.;
    let mut last_report = -10.;
    while elapsed < limit {
        simulation.advance(30);
        elapsed += 1.;
        let status: Value =
            serde_json::from_str(&simulation.level_status_json()).expect("invalid status");
        match status["state"].as_str().unwrap_or("") {
            "won" => {
                println!("{id}: WON at {elapsed:.0}s");
                return;
            }
            "lost" => {
                eprintln!("{id}: LOST at {elapsed:.0}s");
                report(&status);
                std::process::exit(1);
            }
            _ => {}
        }
        if verbose && elapsed - last_report >= 10. {
            last_report = elapsed;
            let counts = &status["counts"];
            println!(
                "{id}: t={elapsed:.0}s state={} user={} level={} infected={} total={} hold={:.0} counts={}",
                status["state"],
                counts["user"],
                counts["level"],
                counts["infected"],
                counts["total"],
                status["winHold"].as_f64().unwrap_or(0.),
                counts
            );
            report(&status);
        }
    }
    let status: Value = serde_json::from_str(&simulation.level_status_json()).unwrap();
    eprintln!("{id}: still running after {limit:.0}s");
    report(&status);
    std::process::exit(1);
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
