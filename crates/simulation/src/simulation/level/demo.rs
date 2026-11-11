//! Scripted demo playback. A demo is a tiny ordered command track: spawn a
//! genome mode, remove a scripted cell, or boost one. Demo cells are player
//! cells (class 1), exactly like a manual insertion, and bypass the insertion
//! budget only because playback is an explicit "show me" mode.

use super::schema::DemoAction;
use super::{LevelEvent, LevelState, spawn_player_cell};
use crate::simulation::Simulation;

impl Simulation {
    /// Run every action whose timestamp has been reached. Called once per tick
    /// before the world advances, so a spawn at t=0 is visible for a full tick.
    pub(crate) fn dispatch_level_demo(&mut self) {
        let Some(runtime) = &self.level else {
            return;
        };
        if !runtime.demo || runtime.state != LevelState::Running {
            return;
        }
        let elapsed = runtime.elapsed(self.tick);
        let due = runtime.level.demo[runtime.demo_cursor..]
            .iter()
            .take_while(|action| action.at <= elapsed + 1e-6)
            .cloned()
            .collect::<Vec<_>>();
        if due.is_empty() {
            return;
        }
        if let Some(runtime) = &mut self.level {
            runtime.demo_cursor += due.len();
        }
        for action in due {
            self.apply_demo_action(action);
        }
    }

    fn apply_demo_action(&mut self, action: DemoAction) {
        let tick = self.tick;
        match action.op.as_str() {
            "spawn" => {
                let Some(genome_index) = action.genome else {
                    self.emit_level_event(
                        LevelEvent::new(tick, "demo").with_detail("spawn without genome"),
                    );
                    return;
                };
                let Some(runtime) = &self.level else {
                    return;
                };
                let Some(genome) = runtime.genomes.get(genome_index) else {
                    self.emit_level_event(
                        LevelEvent::new(tick, "demo")
                            .with_detail(format!("spawn missing genome={genome_index}")),
                    );
                    return;
                };
                if action.mode >= genome.len() {
                    self.emit_level_event(
                        LevelEvent::new(tick, "demo")
                            .with_detail(format!("spawn missing mode={}", action.mode)),
                    );
                    return;
                }
                let genome = genome.clone();
                let mass = action.mass.unwrap_or(1.);
                let id =
                    spawn_player_cell(self, action.x, action.y, action.mode, genome, mass, None);
                let detail = format!(
                    "spawn genome={genome_index} mode={} mass={mass:.2} id={id}",
                    action.mode
                );
                self.emit_level_event(LevelEvent {
                    cell: (id != 0).then_some(id),
                    class: Some(crate::components::CLASS_USER),
                    mode: Some(action.mode),
                    mass: Some(mass),
                    x: Some(action.x.clamp(0., 1.)),
                    y: Some(action.y.clamp(0., 1.)),
                    ..LevelEvent::new(tick, "demo").with_detail(detail)
                });
            }
            "remove" => {
                if let Some(id) = action.id {
                    let _ = self.remove_cell(id);
                }
            }
            "boost" => {
                if let Some(id) = action.id {
                    let _ = self.boost_cell(id);
                }
            }
            other => {
                self.emit_level_event(
                    LevelEvent::new(tick, "demo").with_detail(format!("unknown op={other}")),
                );
            }
        }
    }
}
