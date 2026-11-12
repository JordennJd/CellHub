//! Replay legal player interventions; no privileged mass, tools or budgets.
use super::schema::DemoAction;
use super::{LevelEvent, LevelState};
use crate::components::SIZE;
use crate::simulation::Simulation;

impl Simulation {
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
        for action in due {
            match self.apply_demo_action(&action) {
                Ok(()) => self.level.as_mut().unwrap().demo_cursor += 1,
                Err(error) => {
                    self.emit_level_event(
                        LevelEvent::new(self.tick, "demo-error").with_detail(error.clone()),
                    );
                    self.level.as_mut().unwrap().demo_error = Some(error);
                    self.level.as_mut().unwrap().demo = false;
                    break;
                }
            }
        }
    }

    fn apply_demo_action(&mut self, action: &DemoAction) -> Result<(), String> {
        match action.op.as_str() {
            "spawn" => {
                let runtime = self.level.as_ref().ok_or("Уровень не загружен")?;
                let index = action.genome.ok_or("Не указан геном")?;
                let genome = runtime
                    .genomes
                    .get(index)
                    .ok_or("Отсутствующий геном")?
                    .clone();
                let id = self.spawn_genome_checked(
                    action.x * SIZE,
                    action.y * SIZE,
                    action.mode,
                    genome,
                )?;
                self.emit_level_event(LevelEvent::new(self.tick, "demo").with_detail(format!(
                    "spawn genome={index} mode={} mass=1 id={id}",
                    action.mode
                )));
                Ok(())
            }
            "remove" => self.remove_cell(action.id.ok_or("Не указана клетка")?),
            "boost" => self.boost_cell(action.id.ok_or("Не указана клетка")?),
            "move" => self.reposition_cell(
                action.id.ok_or("Не указана клетка")?,
                action.x * SIZE,
                action.y * SIZE,
            ),
            _ => Err("Неизвестное действие решения".into()),
        }
    }
}
