//! Data-driven challenge manifests. Levels are plain JSON so campaign content
//! can be edited, reviewed and reproduced without recompiling the engine.
//!
//! Genomes use the same `{ version: 5, modes: [...] }` shape as the browser
//! genome bank. Omitted mode fields receive the same defaults as `newMode` in
//! `frontend/model.ts`; child references default to the mode's own index.

use crate::components::LIMIT;
use crate::genome::{Genome, Mode, Program};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const LEVEL_SCHEMA: u32 = 1;

fn default_light() -> f32 {
    1.
}

fn default_threshold() -> f32 {
    2.4
}

fn default_ratio() -> f32 {
    0.5
}

fn default_speed() -> f32 {
    12.
}

fn default_angle() -> f32 {
    0.8
}

fn default_lifespan() -> f32 {
    240.
}

fn default_priority() -> f32 {
    1.
}

fn default_sensor_gain() -> f32 {
    1.
}

fn default_sense_tolerance() -> f32 {
    0.2
}

fn default_secretion_rate() -> f32 {
    0.2
}

fn default_transition_threshold_a() -> f32 {
    0.5
}

fn default_transition_threshold_b() -> f32 {
    1.
}

fn default_telomeres() -> i32 {
    -1
}

fn default_cilia() -> f32 {
    6.
}

fn default_contraction() -> f32 {
    0.3
}

fn default_minus_one() -> f32 {
    -1.
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Level {
    pub schema: u32,
    pub id: String,
    pub order: u32,
    pub title: String,
    pub description: String,
    pub difficulty: u8,
    pub hints: Vec<String>,
    pub solution: String,
    pub tools: Vec<String>,
    pub budget: u32,
    pub remove_max: Option<u32>,
    pub max_cells_of_type: BTreeMap<String, u32>,
    pub unlocks: Vec<String>,
    pub allowed_kinds: Vec<u8>,
    /// Scripted original tutorials have no ordinary win condition. They still
    /// load and can be opened, but the campaign demo test does not require a
    /// fabricated completion target.
    pub tutorial: bool,
    pub environment: LevelEnvironment,
    pub terrain: LevelTerrain,
    pub conditions: Conditions,
    pub genomes: Vec<LevelGenome>,
    pub initial: InitialState,
    pub demo: Vec<DemoAction>,
    pub test: TestOptions,
}

impl Default for Level {
    fn default() -> Self {
        Self {
            schema: LEVEL_SCHEMA,
            id: String::new(),
            order: 0,
            title: String::new(),
            description: String::new(),
            difficulty: 0,
            hints: Vec::new(),
            solution: String::new(),
            tools: vec!["place".into(), "select".into()],
            budget: 1,
            remove_max: None,
            max_cells_of_type: BTreeMap::new(),
            unlocks: Vec::new(),
            allowed_kinds: Vec::new(),
            tutorial: false,
            environment: LevelEnvironment::default(),
            terrain: LevelTerrain::default(),
            conditions: Conditions::default(),
            genomes: Vec::new(),
            initial: InitialState::default(),
            demo: Vec::new(),
            test: TestOptions::default(),
        }
    }
}

impl Level {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let level: Self =
            serde_json::from_str(text).map_err(|error| format!("Некорректный уровень: {error}"))?;
        level.validate()?;
        Ok(level)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != LEVEL_SCHEMA {
            return Err(format!(
                "Неподдерживаемая схема уровня {} (ожидается {LEVEL_SCHEMA})",
                self.schema
            ));
        }
        if self.id.trim().is_empty() || self.id.len() > 80 {
            return Err("Уровень должен иметь короткий id".into());
        }
        if self.genomes.len() > 1000 {
            return Err("Слишком много геномов в уровне".into());
        }
        for (index, genome) in self.genomes.iter().enumerate() {
            genome
                .to_mode_vec()
                .map_err(|error| format!("Геном {index}: {error}"))?;
        }
        for (index, cell) in self.initial.cells.iter().enumerate() {
            let genome = self
                .genomes
                .get(cell.genome)
                .ok_or_else(|| format!("Клетка {index} ссылается на отсутствующий геном"))?;
            if cell.mode >= genome.modes.len() {
                return Err(format!("Клетка {index} ссылается на отсутствующий режим"));
            }
            if !cell.x.is_finite()
                || !cell.y.is_finite()
                || !(0. ..=1.).contains(&cell.x)
                || !(0. ..=1.).contains(&cell.y)
                || !cell.mass.is_finite()
                || cell.mass <= 0.
            {
                return Err(format!("Некорректная начальная клетка {index}"));
            }
        }
        for &[a, b] in &self.initial.links {
            if a >= self.initial.cells.len() || b >= self.initial.cells.len() || a == b {
                return Err("Некорректная начальная связь".into());
            }
        }
        for action in &self.demo {
            if !action.at.is_finite() || action.at < 0. {
                return Err("Некорректное время демонстрационного действия".into());
            }
            if action.op == "spawn" {
                if action
                    .genome
                    .and_then(|index| self.genomes.get(index))
                    .is_none()
                {
                    return Err("Демонстрация ссылается на отсутствующий геном".into());
                }
                if action.mode >= self.genomes[action.genome.unwrap()].modes.len() {
                    return Err("Демонстрация ссылается на отсутствующий режим".into());
                }
                if !action.x.is_finite()
                    || !action.y.is_finite()
                    || !(0. ..=1.).contains(&action.x)
                    || !(0. ..=1.).contains(&action.y)
                {
                    return Err("Демонстрационная клетка вне чашки".into());
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LevelEnvironment {
    pub light: f32,
    pub food: f32,
    pub radiation: f32,
    pub viscosity: f32,
    pub aging: bool,
    pub kill_at_edge: bool,
    pub only_point_mutations: bool,
    pub gravity: f32,
    pub density: f32,
    pub nitrogen: f32,
    /// Original `max_cells`; 0 means the engine's hard limit.
    pub max_cells: usize,
}

impl Default for LevelEnvironment {
    fn default() -> Self {
        Self {
            light: 1.,
            food: 1.,
            radiation: 0.,
            viscosity: 1.,
            aging: true,
            kill_at_edge: false,
            only_point_mutations: true,
            gravity: 0.,
            density: 0.,
            nitrogen: 1.,
            max_cells: LIMIT,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LevelTerrain {
    /// Absolute light at the top of the dish.
    #[serde(default = "default_light")]
    pub light: f32,
    /// Fraction of the dish height reached by the light. 0 lights the dish
    /// uniformly; the original bright bands use small values.
    pub light_range: f32,
    pub radiation: f32,
    /// Uniform food concentration per tile when no food particles are given.
    pub food: f32,
    pub nitrates: f32,
    pub biome: u8,
    pub capacity: f32,
}

impl Default for LevelTerrain {
    fn default() -> Self {
        Self {
            light: 1.,
            light_range: 0.,
            radiation: 0.,
            food: 0.,
            nitrates: 1.,
            biome: 1,
            capacity: 1.,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct Conditions {
    pub user_cells: Option<u32>,
    pub user_cells_max: Option<u32>,
    pub level_cells: Option<u32>,
    pub level_cells_min: Option<u32>,
    pub total_cells_min: Option<u32>,
    pub infected_level_cells: Option<u32>,
    pub infected_level_cells_max: Option<u32>,
    pub hybrid_cells_min: Option<u32>,
    pub black_level_cells: Option<u32>,
    pub red_level_cells: Option<u32>,
    pub red_level_cells_min: Option<u32>,
    pub yellow_level_cells: Option<u32>,
    pub yellow_level_cells_min: Option<u32>,
    pub win_time: Option<f32>,
    pub max_time: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionKind {
    UserCells,
    UserCellsMax,
    LevelCells,
    LevelCellsMin,
    TotalCellsMin,
    InfectedLevelCells,
    InfectedLevelCellsMax,
    HybridCellsMin,
    BlackLevelCells,
    RedLevelCells,
    RedLevelCellsMin,
    YellowLevelCells,
    YellowLevelCellsMin,
    WinTime,
    MaxTime,
}

/// Canonical order shared with the browser objective panel and WASM status.
pub const CONDITION_ORDER: [ConditionKind; 15] = [
    ConditionKind::UserCells,
    ConditionKind::UserCellsMax,
    ConditionKind::LevelCells,
    ConditionKind::LevelCellsMin,
    ConditionKind::TotalCellsMin,
    ConditionKind::InfectedLevelCells,
    ConditionKind::InfectedLevelCellsMax,
    ConditionKind::HybridCellsMin,
    ConditionKind::BlackLevelCells,
    ConditionKind::RedLevelCells,
    ConditionKind::RedLevelCellsMin,
    ConditionKind::YellowLevelCells,
    ConditionKind::YellowLevelCellsMin,
    ConditionKind::WinTime,
    ConditionKind::MaxTime,
];

impl Conditions {
    pub fn values(&self) -> Vec<(ConditionKind, f32)> {
        let mut values = Vec::new();
        for key in CONDITION_ORDER {
            let value = match key {
                ConditionKind::UserCells => self.user_cells.map(|v| v as f32),
                ConditionKind::UserCellsMax => self.user_cells_max.map(|v| v as f32),
                ConditionKind::LevelCells => self.level_cells.map(|v| v as f32),
                ConditionKind::LevelCellsMin => self.level_cells_min.map(|v| v as f32),
                ConditionKind::TotalCellsMin => self.total_cells_min.map(|v| v as f32),
                ConditionKind::InfectedLevelCells => self.infected_level_cells.map(|v| v as f32),
                ConditionKind::InfectedLevelCellsMax => {
                    self.infected_level_cells_max.map(|v| v as f32)
                }
                ConditionKind::HybridCellsMin => self.hybrid_cells_min.map(|v| v as f32),
                ConditionKind::BlackLevelCells => self.black_level_cells.map(|v| v as f32),
                ConditionKind::RedLevelCells => self.red_level_cells.map(|v| v as f32),
                ConditionKind::RedLevelCellsMin => self.red_level_cells_min.map(|v| v as f32),
                ConditionKind::YellowLevelCells => self.yellow_level_cells.map(|v| v as f32),
                ConditionKind::YellowLevelCellsMin => self.yellow_level_cells_min.map(|v| v as f32),
                ConditionKind::WinTime => self.win_time,
                ConditionKind::MaxTime => self.max_time,
            };
            if let Some(value) = value {
                values.push((key, value));
            }
        }
        values
    }

    pub fn win_time(&self) -> f32 {
        self.win_time.unwrap_or(0.)
    }

    pub fn max_time(&self) -> f32 {
        self.max_time.unwrap_or(f32::INFINITY)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LevelGenome {
    pub version: u32,
    pub modes: Vec<LevelMode>,
}

impl Default for LevelGenome {
    fn default() -> Self {
        Self {
            version: 5,
            modes: Vec::new(),
        }
    }
}

impl LevelGenome {
    pub fn to_mode_vec(&self) -> Result<Vec<Mode>, String> {
        if self.version != 5 {
            return Err(format!(
                "Геном уровня должен быть версии 5, получено {}",
                self.version
            ));
        }
        if self.modes.is_empty() || self.modes.len() > 1000 {
            return Err("Геном уровня должен содержать от 1 до 1000 режимов".into());
        }
        let count = self.modes.len();
        self.modes
            .iter()
            .enumerate()
            .map(|(index, mode)| mode.to_mode(index, count))
            .collect()
    }

    pub fn to_genome(&self) -> Result<Genome, String> {
        Ok(Genome::from_modes(self.to_mode_vec()?))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LevelMode {
    pub name: Option<String>,
    pub kind: u8,
    #[serde(default = "default_threshold")]
    pub threshold: f32,
    pub child_a: Option<usize>,
    pub child_b: Option<usize>,
    #[serde(default = "default_ratio")]
    pub ratio: f32,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default = "default_angle")]
    pub angle: f32,
    #[serde(default = "default_lifespan")]
    pub lifespan: f32,
    pub can_divide: bool,
    pub bind_children: bool,
    pub keep_adhesion_a: bool,
    pub keep_adhesion_b: bool,
    pub child_angle_a: f32,
    pub child_angle_b: f32,
    pub mirror_a: bool,
    pub mirror_b: bool,
    #[serde(default = "default_minus_one")]
    pub red: f32,
    #[serde(default = "default_minus_one")]
    pub green: f32,
    #[serde(default = "default_minus_one")]
    pub blue: f32,
    pub max_connections: i32,
    #[serde(default = "default_priority")]
    pub nutrient_priority: f32,
    pub prioritize_when_starving: bool,
    pub density: f32,
    pub signal_channel: u8,
    pub sensor: u8,
    #[serde(default = "default_sensor_gain")]
    pub sensor_gain: f32,
    #[serde(default = "default_minus_one")]
    pub target_red: f32,
    #[serde(default = "default_minus_one")]
    pub target_green: f32,
    #[serde(default = "default_minus_one")]
    pub target_blue: f32,
    #[serde(default = "default_sense_tolerance")]
    pub sense_tolerance: f32,
    pub secretion: u8,
    #[serde(default = "default_secretion_rate")]
    pub secretion_rate: f32,
    pub transition_a: Option<usize>,
    pub transition_b: Option<usize>,
    #[serde(default = "default_transition_threshold_a")]
    pub transition_threshold_a: f32,
    #[serde(default = "default_transition_threshold_b")]
    pub transition_threshold_b: f32,
    pub offspring_mode: Option<usize>,
    /// Nonnegative mode index, or a negative legacy gamete group.
    pub compatible_mode: Option<i32>,
    #[serde(default = "default_telomeres")]
    pub telomeres: i32,
    #[serde(default = "default_cilia")]
    pub cilia_left: f32,
    #[serde(default = "default_cilia")]
    pub cilia_right: f32,
    #[serde(default = "default_contraction")]
    pub contraction: f32,
    pub bending: f32,
    pub lift: f32,
    pub virus_copy_from: i32,
    pub programs: Vec<LevelProgram>,
}

impl Default for LevelMode {
    fn default() -> Self {
        Self {
            name: None,
            kind: 0,
            threshold: default_threshold(),
            child_a: None,
            child_b: None,
            ratio: default_ratio(),
            speed: default_speed(),
            angle: default_angle(),
            lifespan: default_lifespan(),
            can_divide: true,
            bind_children: false,
            keep_adhesion_a: false,
            keep_adhesion_b: false,
            child_angle_a: 0.,
            child_angle_b: 0.,
            mirror_a: false,
            mirror_b: false,
            red: -1.,
            green: -1.,
            blue: -1.,
            max_connections: -1,
            nutrient_priority: 1.,
            prioritize_when_starving: false,
            density: 0.,
            signal_channel: 0,
            sensor: 0,
            sensor_gain: 1.,
            target_red: -1.,
            target_green: -1.,
            target_blue: -1.,
            sense_tolerance: 0.2,
            secretion: 0,
            secretion_rate: 0.2,
            transition_a: None,
            transition_b: None,
            transition_threshold_a: 0.5,
            transition_threshold_b: 1.,
            offspring_mode: None,
            compatible_mode: None,
            telomeres: -1,
            cilia_left: 6.,
            cilia_right: 6.,
            contraction: 0.3,
            bending: 0.,
            lift: 0.,
            virus_copy_from: -1,
            programs: Vec::new(),
        }
    }
}

impl LevelMode {
    pub fn to_mode(&self, index: usize, count: usize) -> Result<Mode, String> {
        if self.kind >= crate::cell_types::COUNT {
            return Err(format!("Неизвестный тип клетки {}", self.kind));
        }
        let reference = |value: Option<usize>| value.unwrap_or(index).min(count - 1);
        let compatible = match self.compatible_mode {
            Some(value) if value < 0 => value.clamp(-16, -1),
            Some(value) => value.min(count as i32 - 1),
            None => index.min(count.saturating_sub(1)) as i32,
        };
        let color = if self.red == -1. && self.green == -1. && self.blue == -1. {
            [-1.; 3]
        } else {
            [
                self.red.clamp(0., 1.),
                self.green.clamp(0., 1.),
                self.blue.clamp(0., 1.),
            ]
        };
        let target_color =
            if self.target_red == -1. && self.target_green == -1. && self.target_blue == -1. {
                [-1.; 3]
            } else {
                [
                    self.target_red.clamp(0., 1.),
                    self.target_green.clamp(0., 1.),
                    self.target_blue.clamp(0., 1.),
                ]
            };
        let mut programs = [Program::default(); 8];
        let count_programs = self.programs.len().min(8);
        for (slot, source) in programs.iter_mut().zip(self.programs.iter()) {
            *slot = Program {
                target: source.target.min(17),
                input: source.input.min(13),
                kind: u8::from(source.kind != 0),
                scale: clamp_finite(source.scale, -1000., 1000.),
                offset: clamp_finite(source.offset, -1000., 1000.),
                threshold: clamp_finite(source.threshold, -1000., 1000.),
                below: clamp_finite(source.below, -1000., 1000.),
            };
        }
        Ok(Mode {
            kind: self.kind,
            threshold: self.threshold.clamp(1., 8.),
            child_a: reference(self.child_a),
            child_b: reference(self.child_b),
            ratio: self.ratio.clamp(0.2, 0.8),
            speed: self.speed.clamp(0., 60.),
            angle: self
                .angle
                .clamp(-std::f32::consts::PI, std::f32::consts::PI),
            lifespan: self.lifespan.clamp(20., 1000.),
            can_divide: self.can_divide,
            bind_children: self.bind_children,
            keep_a: self.keep_adhesion_a,
            keep_b: self.keep_adhesion_b,
            child_angle_a: self
                .child_angle_a
                .clamp(-std::f32::consts::PI, std::f32::consts::PI),
            child_angle_b: self
                .child_angle_b
                .clamp(-std::f32::consts::PI, std::f32::consts::PI),
            mirror_a: self.mirror_a,
            mirror_b: self.mirror_b,
            color,
            max_connections: (self.max_connections >= 0)
                .then_some((self.max_connections as usize).min(LIMIT)),
            nutrient_priority: self.nutrient_priority.clamp(0.25, 4.),
            prioritize_when_starving: self.prioritize_when_starving,
            density: self.density.clamp(-3.3, 3.3),
            signal_channel: self.signal_channel.min(7),
            sensor: self.sensor.min(8),
            sensor_gain: self.sensor_gain.clamp(-16., 16.),
            target_color,
            sense_tolerance: self.sense_tolerance.clamp(0., 1.),
            secretion: self.secretion.min(22),
            secretion_rate: self.secretion_rate.clamp(0., 4.),
            transition_a: reference(self.transition_a),
            transition_b: reference(self.transition_b),
            transition_threshold_a: clamp_finite(self.transition_threshold_a, -1000., 1000.),
            transition_threshold_b: clamp_finite(self.transition_threshold_b, -1000., 1000.),
            offspring_mode: reference(self.offspring_mode),
            compatible_mode: compatible as usize,
            legacy_gamete_group: (compatible < 0).then_some((-compatible - 1) as u8),
            telomeres: self.telomeres.clamp(-1, 1000),
            cilia_left: self.cilia_left.clamp(-60., 60.),
            cilia_right: self.cilia_right.clamp(-60., 60.),
            contraction: self.contraction.clamp(-1., 1.),
            bending: self.bending.clamp(-1., 1.),
            lift: self.lift.clamp(-1., 1.),
            virus_copy_from: if self.virus_copy_from < 0 {
                usize::MAX
            } else {
                (self.virus_copy_from as usize).min(count - 1)
            },
            program_count: count_programs,
            programs,
        })
    }
}

fn clamp_finite(value: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        0.
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LevelProgram {
    pub target: u8,
    pub input: u8,
    /// 0 = linear, 1 = threshold.
    pub kind: u8,
    pub scale: f32,
    pub offset: f32,
    pub threshold: f32,
    pub below: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InitialState {
    pub cells: Vec<InitialCell>,
    pub links: Vec<[usize; 2]>,
    /// Particle positions in normalized dish coordinates. The optional third
    /// value is the original particle size and weights the tile food.
    pub food: Vec<Vec<f32>>,
}

impl Default for InitialState {
    fn default() -> Self {
        Self {
            cells: Vec::new(),
            links: Vec::new(),
            food: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InitialCell {
    pub x: f32,
    pub y: f32,
    pub angle: f32,
    pub mass: f32,
    pub mode: usize,
    pub genome: usize,
    pub generation: u32,
    pub age: f32,
    /// Substrate cells default to level class 0.
    pub class: u8,
}

impl Default for InitialCell {
    fn default() -> Self {
        Self {
            x: 0.5,
            y: 0.5,
            angle: 0.,
            mass: 1.,
            mode: 0,
            genome: 0,
            generation: 0,
            age: 0.,
            class: crate::components::CLASS_LEVEL,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DemoAction {
    /// Seconds after level start.
    pub at: f32,
    pub op: String,
    pub x: f32,
    pub y: f32,
    pub mode: usize,
    pub genome: Option<usize>,
    pub id: Option<u32>,
    pub mass: Option<f32>,
}

impl Default for DemoAction {
    fn default() -> Self {
        Self {
            at: 0.,
            op: String::new(),
            x: 0.5,
            y: 0.5,
            mode: 0,
            genome: None,
            id: None,
            mass: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TestOptions {
    pub max_seconds: Option<f32>,
    pub seed: Option<u32>,
}

impl Default for TestOptions {
    fn default() -> Self {
        Self {
            max_seconds: None,
            seed: Some(0xce11_ab),
        }
    }
}
