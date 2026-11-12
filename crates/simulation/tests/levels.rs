//! Campaign regression: every offered solution must obey player rules and win.
//!
//! The files in `/levels` are the product, not test fixtures. This test is the
//! contract that keeps the data honest: it loads each manifest through the same
//! JSON path the browser uses, plays the demo track at 30 Hz and fails on a
//! timeout or loss. Scripted tutorials are skipped because their original
//! lessons have no ordinary win condition.
//!
//! Runtime policy: levels are independent, so they run in parallel, and every
//! level has a hard 30-second wall-clock budget. A broken demo therefore costs
//! at most 30 real seconds instead of ten minutes of debug simulation.
//!
//! Environment:
//! - `LEVEL_FILTER=<substring>` — only ids containing the substring.
//! - `LEVEL_JOBS=<n>` — worker threads (default: available parallelism).
//! - `LEVEL_WALL_SECONDS=<n>` — per-level wall deadline (default: 30).
//! - `LEVEL_FAIL_FAST=1` — stop scheduling new levels after the first failure.
//! - `LEVEL_VERBOSE=1` — print per-level progress.

use cellhub_simulation::Simulation;
use serde_json::Value;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn levels_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../levels")
}

fn env_number(name: &str) -> Option<f64> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
}

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| value != "0")
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

fn load(path: &Path) -> Value {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Numeric-aware order so `3.2` sorts after `3` and before `10`.
fn sort_key(path: &Path) -> Vec<u64> {
    path.file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_default()
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(u64::MAX))
        .collect()
}

fn status(simulation: &Simulation) -> Value {
    serde_json::from_str(&simulation.level_status_json()).unwrap_or(Value::Null)
}

fn conditions_report(status: &Value) -> String {
    status["conditions"]
        .as_array()
        .map(|conditions| {
            conditions
                .iter()
                .map(|condition| {
                    format!(
                        "  {:<28} {:>8} / {:<8} {}",
                        condition["key"].as_str().unwrap_or(""),
                        condition["current"],
                        condition["target"],
                        if condition["met"].as_bool().unwrap_or(false) {
                            "ok"
                        } else {
                            "FAIL"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn census_report(simulation: &Simulation) -> String {
    let census: Value =
        serde_json::from_str(&simulation.level_census_json()).expect("invalid census");
    let status = &census["status"];
    let counts = &status["counts"];
    let substrate = &census["substrate"];
    let mut lines = vec![
        format!(
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
        ),
        format!(
            "  food={:.1}/{:.1} nitrates={:.1}/{:.1} covered={:.1} light={:.3} radiation={:.4}",
            substrate["food"].as_f64().unwrap_or(0.),
            substrate["capacity"].as_f64().unwrap_or(0.),
            substrate["nitrates"].as_f64().unwrap_or(0.),
            substrate["nitrateCapacity"].as_f64().unwrap_or(0.),
            substrate["coveredFood"].as_f64().unwrap_or(0.),
            substrate["light"].as_f64().unwrap_or(0.),
            substrate["radiation"].as_f64().unwrap_or(0.),
        ),
    ];
    if let Some(modes) = census["modes"].as_array() {
        for mode in modes.iter().take(12) {
            lines.push(format!(
                "  mode class={} mode={} count={} rank={} color=({:.2},{:.2},{:.2}) mass={:.2}..{:.2} centroid=({:.3},{:.3})",
                mode["class"],
                mode["mode"],
                mode["count"],
                mode["rank"].as_str().unwrap_or(""),
                mode["color"][0].as_f64().unwrap_or(0.),
                mode["color"][1].as_f64().unwrap_or(0.),
                mode["color"][2].as_f64().unwrap_or(0.),
                mode["massMin"].as_f64().unwrap_or(0.),
                mode["massMax"].as_f64().unwrap_or(0.),
                mode["centroid"][0].as_f64().unwrap_or(0.),
                mode["centroid"][1].as_f64().unwrap_or(0.),
            ));
        }
        if modes.len() > 12 {
            lines.push(format!("  … {} more mode groups", modes.len() - 12));
        }
    }
    let events: Vec<Value> =
        serde_json::from_str(&simulation.level_events_json()).unwrap_or_default();
    if !events.is_empty() {
        lines.push(format!("  last events ({} total):", events.len()));
        for event in events.iter().rev().take(15).rev() {
            lines.push(format!(
                "    t={:.2}s {:<10} cell={} class={} mode={} {}",
                event["time"].as_f64().unwrap_or(0.),
                event["kind"].as_str().unwrap_or("?"),
                event["cell"],
                event["class"],
                event["mode"],
                event["detail"].as_str().unwrap_or(""),
            ));
        }
    }
    lines.join("\n")
}

/// Play one demo. `Err` is a readable failure report, never a panic.
fn check_level(id: &str, path: &Path, wall: Duration) -> Result<String, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{id}: {error}"))?;
    let level: Value = serde_json::from_str(&text).map_err(|error| format!("{id}: {error}"))?;
    let limit = time_limit(&level);
    let mut simulation = Simulation::new(0xCE11AB);
    simulation
        .load_level_text(&text)
        .map_err(|error| format!("{id}: {error}"))?;
    simulation
        .validate_demo_track()
        .map_err(|error| format!("{id}: {error}"))?;
    simulation
        .start_demo_track()
        .map_err(|error| format!("{id}: {error}"))?;
    let started = Instant::now();
    let mut elapsed = 0u32;
    loop {
        simulation.advance(30);
        elapsed += 1;
        let status = status(&simulation);
        if status["demoError"].is_string() {
            return Err(format!("{id}: {}", status["demoError"]));
        }
        if status["inserted"].as_u64().unwrap_or(0) > level["budget"].as_u64().unwrap_or(0) {
            return Err(format!("{id}: placement budget exceeded"));
        }
        if let Some(max) = level["removeMax"].as_u64() {
            if status["removed"].as_u64().unwrap_or(0) > max {
                return Err(format!("{id}: removal budget exceeded"));
            }
        }
        match status["state"].as_str().unwrap_or("") {
            "won" => {
                return Ok(format!(
                    "{id}: PASS sim={elapsed}s wall={:.1}s",
                    started.elapsed().as_secs_f64()
                ));
            }
            "lost" => {
                return Err(format!(
                    "{id}: LOST at {elapsed}s\n{}\n{}",
                    conditions_report(&status),
                    census_report(&simulation)
                ));
            }
            _ => {}
        }
        if f64::from(elapsed) >= limit {
            return Err(format!(
                "{id}: no win after {limit:.0}s simulated\n{}\n{}",
                conditions_report(&status),
                census_report(&simulation)
            ));
        }
        if started.elapsed() > wall {
            return Err(format!(
                "{id}: wall-clock deadline {:.0}s hit at {elapsed}s simulated\n{}\n{}",
                wall.as_secs_f64(),
                conditions_report(&status),
                census_report(&simulation)
            ));
        }
    }
}

#[test]
fn every_available_solution_obeys_player_rules_and_completes_its_level() {
    let filter = std::env::var("LEVEL_FILTER").ok();
    let wall = Duration::from_secs_f64(env_number("LEVEL_WALL_SECONDS").unwrap_or(30.).max(1.));
    let fail_fast = env_flag("LEVEL_FAIL_FAST");
    let verbose = env_flag("LEVEL_VERBOSE");
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
    paths.sort_by_key(|path| sort_key(path));
    assert!(!paths.is_empty(), "no levels matched the filter");

    let tasks = paths
        .into_iter()
        .filter_map(|path| {
            let level = load(&path);
            if level["tutorial"].as_bool().unwrap_or(false) {
                return None;
            }
            if level["solutionStatus"] == "unavailable" {
                assert!(
                    level["demo"].as_array().is_some_and(Vec::is_empty),
                    "unavailable solution must not carry a cheating track"
                );
                assert!(
                    level["solutionIssue"]
                        .as_str()
                        .is_some_and(|s| !s.trim().is_empty())
                );
                return None;
            }
            assert_eq!(
                level["solutionStatus"], "verified",
                "every campaign solution must declare its status"
            );
            Some((level["id"].as_str().unwrap_or("?").to_owned(), path))
        })
        .collect::<Vec<_>>();
    assert!(
        !tasks.is_empty(),
        "no non-tutorial levels matched the filter"
    );

    let queue = Mutex::new(VecDeque::from(tasks));
    let results: Mutex<Vec<(String, Result<String, String>)>> = Mutex::new(Vec::new());
    let stop = AtomicBool::new(false);
    let jobs = std::env::var("LEVEL_JOBS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get)
        })
        .clamp(1, 32);

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| {
                loop {
                    if fail_fast && stop.load(Ordering::Relaxed) {
                        return;
                    }
                    let task = queue.lock().unwrap().pop_front();
                    let Some((id, path)) = task else {
                        return;
                    };
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        check_level(&id, &path, wall)
                    }))
                    .unwrap_or_else(|payload| {
                        let message = payload
                            .downcast_ref::<&str>()
                            .map(|text| (*text).to_owned())
                            .or_else(|| payload.downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "panic".to_owned());
                        Err(format!("{id}: PANIC: {message}"))
                    });
                    let failed = outcome.is_err();
                    if verbose || failed {
                        eprintln!("{}", outcome.as_deref().unwrap_or_else(|error| error));
                    }
                    results.lock().unwrap().push((id, outcome));
                    if failed && fail_fast {
                        stop.store(true, Ordering::Relaxed);
                    }
                }
            });
        }
    });

    let mut results = results.into_inner().unwrap();
    results.sort_by(|a, b| a.0.cmp(&b.0));
    let checked = results.len();
    let failures = results
        .into_iter()
        .filter_map(|(_, outcome)| outcome.err())
        .collect::<Vec<_>>();
    assert!(checked > 0, "no levels checked");
    assert!(
        failures.is_empty(),
        "{} of {checked} level demos failed (per-level wall deadline {:.0}s):\n\n{}",
        failures.len(),
        wall.as_secs_f64(),
        failures.join("\n\n")
    );
    if verbose {
        eprintln!("all {checked} level demos passed");
    }
}

#[test]
fn all_level_tracks_obey_the_original_intervention_rules() {
    for entry in std::fs::read_dir(levels_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "json")
            || path.file_name().unwrap() == "index.json"
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let mut sim = Simulation::new(1);
        sim.load_level_text(&text).unwrap();
        sim.validate_demo_track()
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

#[test]
fn confirmed_solutions_cannot_silently_disappear() {
    for id in [
        "1", "2", "4", "3", "3.2", "3.3", "3.4", "3.5", "5", "5.5", "6", "7", "8", "9.7", "13",
        "14", "14.5", "16", "17.5", "18.5", "22.5", "25", "26.5", "26.6", "26.8", "26.9", "30",
        "31", "31.5", "32.6", "33.3",
    ] {
        let level = load(&levels_dir().join(format!("{id}.json")));
        assert_eq!(
            level["solutionStatus"], "verified",
            "{id}: a verified solution was removed"
        );
    }
}
