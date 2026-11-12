//! The player and demonstration use the same intervention rules.
use super::{Level, LevelRuntime};
use crate::genome::Genome;
use crate::simulation::Simulation;

const KINDS: [&str; 18] = [
    "photocyte",
    "phagocyte",
    "flagellocyte",
    "virocyte",
    "keratinocyte",
    "devorocyte",
    "nitrocyte",
    "lipocyte",
    "buoyocyte",
    "glueocyte",
    "stereocyte",
    "senseocyte",
    "myocyte",
    "neurocyte",
    "secrocyte",
    "stemocyte",
    "gamete",
    "ciliocyte",
];

impl Level {
    pub(super) fn check_tool(&self, tool: &str) -> Result<(), String> {
        if self.tools.iter().any(|name| name == tool) {
            Ok(())
        } else {
            Err(format!("Инструмент «{tool}» недоступен в этом испытании"))
        }
    }

    pub(super) fn check_genome(&self, genome: &Genome) -> Result<(), String> {
        let mut counts = [0u32; 18];
        for i in 0..genome.len() {
            let kind = genome.mode(i).kind;
            if !self.allowed_kinds.is_empty() && !self.allowed_kinds.contains(&kind) {
                return Err(format!(
                    "Тип клетки {} недоступен в испытании",
                    KINDS[kind as usize]
                ));
            }
            counts[kind as usize] += 1;
        }
        for (name, &max) in &self.max_cells_of_type {
            let key = name.to_lowercase().replace('-', "");
            if let Some(kind) = KINDS.iter().position(|&name| name == key) {
                if counts[kind] > max {
                    return Err(format!(
                        "В геноме допускается не более {max} режимов {name}"
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_demo(&self, genomes: &[Genome]) -> Result<(), String> {
        let mut placed = 0;
        let mut removed = 0;
        let mut previous = 0.;
        for (index, action) in self.demo.iter().enumerate() {
            let check = || -> Result<(), String> {
                if !action.at.is_finite() || action.at < previous {
                    return Err("Действия должны идти по времени".into());
                }
                match action.op.as_str() {
                    "spawn" => {
                        self.check_tool("place")?;
                        if placed >= self.budget {
                            return Err("Исчерпан лимит постановки клеток".into());
                        }
                        if action.mass.is_some_and(|mass| mass != 1.) {
                            return Err("Начальная масса клетки должна быть 1".into());
                        }
                        let genome = action
                            .genome
                            .and_then(|i| genomes.get(i))
                            .ok_or("Отсутствующий геном")?;
                        self.check_genome(genome)?;
                        if action.mode >= genome.len() {
                            return Err("Отсутствующий режим".into());
                        }
                        if !action.x.is_finite()
                            || !action.y.is_finite()
                            || !(0. ..=1.).contains(&action.x)
                            || !(0. ..=1.).contains(&action.y)
                        {
                            return Err("Некорректные координаты".into());
                        }
                    }
                    "remove" => {
                        self.check_tool("remove")?;
                        if self.remove_max.is_some_and(|max| removed >= max) {
                            return Err("Исчерпан лимит удалений".into());
                        }
                        if action.id.is_none() {
                            return Err("Не указана клетка".into());
                        }
                    }
                    "boost" => {
                        self.check_tool("add")?;
                        if action.id.is_none() {
                            return Err("Не указана клетка".into());
                        }
                    }
                    "move" => {
                        self.check_tool("move")?;
                        if action.id.is_none()
                            || !action.x.is_finite()
                            || !action.y.is_finite()
                            || !(0. ..=1.).contains(&action.x)
                            || !(0. ..=1.).contains(&action.y)
                        {
                            return Err("Некорректное перемещение".into());
                        }
                    }
                    _ => return Err("Неизвестное действие".into()),
                }
                Ok(())
            };
            check().map_err(|error| format!("Действие решения {}: {error}", index + 1))?;
            placed += u32::from(action.op == "spawn");
            removed += u32::from(action.op == "remove");
            previous = action.at;
        }
        Ok(())
    }
}

impl LevelRuntime {
    pub(crate) fn check_spawn(&self, genome: &Genome) -> Result<(), String> {
        self.level.check_tool("place")?;
        if self.inserted >= self.level.budget {
            return Err("Лимит вставок исчерпан. Перезапустите попытку.".into());
        }
        self.level.check_genome(genome)
    }
}

impl Simulation {
    pub(crate) fn check_level_tool(&self, tool: &str) -> Result<(), String> {
        if let Some(runtime) = &self.level {
            runtime.level.check_tool(tool)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Mode;
    use crate::simulation::level::schema::{DemoAction, LevelGenome, LevelMode};

    fn level() -> Level {
        Level {
            id: "rules".into(),
            budget: 1,
            allowed_kinds: vec![0],
            solution_status: Some("verified".into()),
            genomes: vec![LevelGenome {
                modes: vec![LevelMode::default()],
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    fn genes() -> Vec<f32> {
        Genome::from_modes(vec![Mode::default()]).values()
    }

    #[test]
    fn placement_budget_cannot_be_bypassed_or_refunded_by_death() {
        let mut l = level();
        l.tools.push("remove".into());
        let mut sim = Simulation::new(5);
        sim.install_level(l).unwrap();
        let id = sim.spawn_checked(1500., 1500., 0, &genes()).unwrap();
        assert!(sim.spawn_checked(1500., 1500., 0, &genes()).is_err());
        sim.remove_cell(id).unwrap();
        assert!(sim.spawn_checked(1500., 1500., 0, &genes()).is_err());
        assert_eq!(sim.level.as_ref().unwrap().inserted, 1);
        sim.reset();
        assert!(sim.spawn_checked(1500., 1500., 0, &genes()).is_ok());
    }

    #[test]
    fn tools_and_successful_removal_counts_are_enforced() {
        let mut l = level();
        l.tools.push("remove".into());
        l.remove_max = Some(1);
        l.budget = 2;
        let mut sim = Simulation::new(5);
        sim.install_level(l).unwrap();
        let a = sim.spawn_checked(1500., 1500., 0, &genes()).unwrap();
        let b = sim.spawn_checked(1520., 1500., 0, &genes()).unwrap();
        assert!(sim.boost_cell(a).is_err());
        assert!(sim.reposition_cell(a, 1500., 1500.).is_err());
        assert!(sim.edit_checked(a, 0, &genes()).is_err());
        assert!(sim.remove_cell(999).is_err());
        assert_eq!(sim.level.as_ref().unwrap().removed, 0);
        sim.remove_cell(a).unwrap();
        assert!(sim.remove_cell(b).is_err());
    }

    #[test]
    fn every_genome_mode_obeys_allowed_types_and_mode_caps() {
        let mut l = level();
        l.allowed_kinds.push(8);
        l.max_cells_of_type.insert("buoyocyte".into(), 1);
        let buoy = Mode {
            kind: 8,
            ..Default::default()
        };
        assert!(
            l.check_genome(&Genome::from_modes(vec![Mode::default(), buoy]))
                .is_ok()
        );
        assert!(
            l.check_genome(&Genome::from_modes(vec![Mode::default(), buoy, buoy]))
                .is_err()
        );
        assert!(
            l.check_genome(&Genome::from_modes(vec![
                Mode::default(),
                Mode {
                    kind: 3,
                    ..Default::default()
                }
            ]))
            .is_err()
        );
    }

    #[test]
    fn demo_rejects_extra_placements_tools_mass_and_forbidden_types_before_reset() {
        let spawn = DemoAction {
            op: "spawn".into(),
            genome: Some(0),
            ..Default::default()
        };
        for actions in [
            vec![spawn.clone(), spawn.clone()],
            vec![DemoAction {
                mass: Some(20.),
                ..spawn.clone()
            }],
            vec![DemoAction {
                op: "boost".into(),
                id: Some(1),
                ..Default::default()
            }],
        ] {
            let mut l = level();
            l.demo = actions;
            let mut sim = Simulation::new(5);
            // Invalid tracks may be rejected when the level is loaded or when
            // the demo starts; either way they must never mutate the world.
            if sim.install_level(l).is_ok() {
                assert!(sim.start_demo().is_err());
            }
            assert!(sim.level.as_ref().is_none_or(|runtime| !runtime.demo));
            assert_eq!(sim.world.len(), 0);
        }
        let mut l = level();
        l.genomes[0].modes[0].kind = 3;
        l.demo = vec![spawn];
        let mut sim = Simulation::new(5);
        if sim.install_level(l).is_ok() {
            assert!(sim.start_demo().is_err());
        }
        assert!(sim.level.as_ref().is_none_or(|runtime| !runtime.demo));
        assert_eq!(sim.world.len(), 0);
    }

    #[test]
    fn legal_demo_and_manual_placement_produce_identical_worlds() {
        let mut l = level();
        l.demo = vec![DemoAction {
            op: "spawn".into(),
            x: 0.5,
            y: 0.5,
            genome: Some(0),
            ..Default::default()
        }];
        let mut manual = Simulation::new(5);
        manual.install_level(l.clone()).unwrap();
        let values = manual.level.as_ref().unwrap().genomes[0].values();
        manual.spawn_checked(1536., 1536., 0, &values).unwrap();
        manual.advance(1);
        let mut demo = Simulation::new(5);
        demo.install_level(l).unwrap();
        demo.start_demo().unwrap();
        demo.advance(1);
        assert_eq!(manual.encode_state(), demo.encode_state());
        assert_eq!(demo.level.as_ref().unwrap().inserted, 1);
    }
}
