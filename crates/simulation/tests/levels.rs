//! Campaign regression: every authored demo must really complete its level.
//!
//! The files in `/levels` are the product, not test fixtures. This test is the
//! contract that keeps the data honest: it loads each manifest through the same
//! JSON path the browser uses, plays the demo track at 30 Hz and fails on a
//! timeout or loss. Scripted tutorials are skipped because their original
//! lessons have no ordinary win condition.

use cellhub_simulation::Simulation;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn levels_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../levels")
}

fn load(path: &Path) -> Value {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn time_limit(level: &Value) -> f64 {
    if let Some(value) = level["test"]["maxSeconds"].as_f64() {
        return value.min(3600.);
    }
    let conditions = &level["conditions"];
    let max = conditions["max-time"].as_f64().unwrap_or(0.);
    let win = conditions["win-time"].as_f64().unwrap_or(0.);
    (max.max(win) + 120.).min(3600.)
}

fn report(id: &str, status: &Value) -> String {
    let mut lines = vec![format!(
        "{id}: state={} elapsed={} hold={} counts={}",
        status["state"], status["elapsed"], status["winHold"], status["counts"]
    )];
    if let Some(conditions) = status["conditions"].as_array() {
        for condition in conditions {
            lines.push(format!(
                "  {:<28} {:>8} / {:<8} {}",
                condition["key"].as_str().unwrap_or(""),
                condition["current"],
                condition["target"],
                if condition["met"].as_bool().unwrap_or(false) {
                    "ok"
                } else {
                    "FAIL"
                }
            ));
        }
    }
    lines.join("\n")
}

#[test]
fn every_demo_completes_its_level() {
    let filter = std::env::var("LEVEL_FILTER").ok();
    let mut paths = std::fs::read_dir(levels_dir())
        .expect("levels directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter(|path| {
            path.file_name().is_some_and(|name| name != "index.json")
                && filter.as_ref().is_none_or(|needle| {
                    path.file_stem()
                        .is_some_and(|stem| stem.to_string_lossy().contains(needle))
                })
        })
        .collect::<Vec<_>>();
    paths.sort();
    assert!(!paths.is_empty(), "no levels matched the filter");
    let mut failures = Vec::new();
    let mut checked = 0;
    for path in paths {
        let level = load(&path);
        let id = level["id"].as_str().unwrap_or("?").to_owned();
        if level["tutorial"].as_bool().unwrap_or(false) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let limit = time_limit(&level);
        let mut simulation = Simulation::new(0xCE11AB);
        simulation
            .load_level_text(&text)
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        simulation
            .start_demo_track()
            .unwrap_or_else(|error| panic!("{id}: {error}"));
        let mut elapsed = 0.;
        loop {
            simulation.advance(30);
            elapsed += 1.;
            let status: Value =
                serde_json::from_str(&simulation.level_status_json()).expect("status json");
            match status["state"].as_str().unwrap_or("") {
                "won" => break,
                "lost" => {
                    failures.push(report(&id, &status));
                    break;
                }
                _ => {}
            }
            if elapsed >= limit {
                failures.push(report(&id, &status));
                break;
            }
        }
        checked += 1;
    }
    assert!(checked > 0, "no non-tutorial levels checked");
    assert!(
        failures.is_empty(),
        "{} of {checked} level demos failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
