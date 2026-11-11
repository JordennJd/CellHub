//! Genome archetypes, validation and ABI encoding.

#[path = "genome/main_v4.rs"]
mod main_v4;

pub(crate) use crate::cell_types::{COUNT as KIND_COUNT, NITROCYTE as NITROCYTE_KIND};
use crate::components::LIMIT;
pub(crate) use main_v4::normalize_signal as normalize_v4_signal;
use std::sync::Arc;

pub(crate) const MODE_STRIDE: usize = 102;
const V4_MODE_STRIDE: usize = 22;
pub(crate) const MAX_MODES: usize = 1000;
const V3_MODE_STRIDE: usize = 19;
const TYPE_COLORS: [u32; 18] = [
    0xa4db71, 0xf1bf72, 0x71cbd4, 0xc997ed, 0xb3c3d0, 0xf08373, 0x8ed6b3, 0xe4b978, 0x8ebfe8,
    0xccbd78, 0xb8a0e5, 0xa7cbe6, 0xe68fa1, 0xb89ce0, 0xdca18c, 0x9ed8bd, 0xe7c893, 0x88c9c3,
];

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Program {
    pub(crate) target: u8,
    pub(crate) input: u8,
    pub(crate) kind: u8,
    pub(crate) scale: f32,
    pub(crate) offset: f32,
    pub(crate) threshold: f32,
    pub(crate) below: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Mode {
    pub(crate) kind: u8,
    pub(crate) threshold: f32,
    pub(crate) child_a: usize,
    pub(crate) child_b: usize,
    pub(crate) ratio: f32,
    pub(crate) speed: f32,
    pub(crate) angle: f32,
    pub(crate) lifespan: f32,
    pub(crate) can_divide: bool,
    pub(crate) bind_children: bool,
    pub(crate) keep_a: bool,
    pub(crate) keep_b: bool,
    pub(crate) child_angle_a: f32,
    pub(crate) child_angle_b: f32,
    pub(crate) mirror_a: bool,
    pub(crate) mirror_b: bool,
    pub(crate) color: [f32; 3],
    pub(crate) max_connections: Option<usize>,
    pub(crate) nutrient_priority: f32,
    pub(crate) prioritize_when_starving: bool,
    pub(crate) density: f32,
    pub(crate) signal_channel: u8,
    pub(crate) sensor: u8,
    pub(crate) sensor_gain: f32,
    pub(crate) target_color: [f32; 3],
    pub(crate) sense_tolerance: f32,
    pub(crate) secretion: u8,
    pub(crate) secretion_rate: f32,
    pub(crate) transition_a: usize,
    pub(crate) transition_b: usize,
    pub(crate) transition_threshold_a: f32,
    pub(crate) transition_threshold_b: f32,
    pub(crate) offspring_mode: usize,
    pub(crate) compatible_mode: usize,
    pub(crate) legacy_gamete_group: Option<u8>,
    pub(crate) telomeres: i32,
    pub(crate) cilia_left: f32,
    pub(crate) cilia_right: f32,
    pub(crate) contraction: f32,
    pub(crate) bending: f32,
    pub(crate) lift: f32,
    pub(crate) virus_copy_from: usize,
    pub(crate) program_count: usize,
    pub(crate) programs: [Program; 8],
}

impl Default for Mode {
    fn default() -> Self {
        Self {
            kind: 0,
            threshold: 2.4,
            child_a: 0,
            child_b: 0,
            ratio: 0.5,
            speed: 12.,
            angle: 0.8,
            lifespan: 240.,
            can_divide: true,
            bind_children: false,
            keep_a: false,
            keep_b: false,
            child_angle_a: 0.,
            child_angle_b: 0.,
            mirror_a: false,
            mirror_b: false,
            color: [-1.; 3],
            max_connections: None,
            nutrient_priority: 1.,
            prioritize_when_starving: false,
            density: 0.,
            signal_channel: 0,
            sensor: 0,
            sensor_gain: 1.,
            target_color: [-1.; 3],
            sense_tolerance: 0.2,
            secretion: 0,
            secretion_rate: 0.2,
            transition_a: 0,
            transition_b: 0,
            transition_threshold_a: 0.5,
            transition_threshold_b: 1.,
            offspring_mode: 0,
            compatible_mode: 0,
            legacy_gamete_group: None,
            telomeres: -1,
            cilia_left: 6.,
            cilia_right: 6.,
            contraction: 0.3,
            bending: 0.,
            lift: 0.,
            virus_copy_from: usize::MAX,
            program_count: 0,
            programs: [Program::default(); 8],
        }
    }
}

impl Mode {
    pub(crate) fn resolved_color(&self) -> [f32; 3] {
        if self.color[0] >= 0. {
            return self.color;
        }
        let color = TYPE_COLORS[self.kind as usize];
        [
            ((color >> 16) & 255) as f32 / 255.,
            ((color >> 8) & 255) as f32 / 255.,
            (color & 255) as f32 / 255.,
        ]
    }
}

#[derive(Clone)]
pub(crate) struct Genome {
    expressed: Arc<Vec<Mode>>,
    inherited: Option<Arc<Vec<Mode>>>,
}

impl Genome {
    pub(crate) fn from_modes(modes: Vec<Mode>) -> Self {
        Self {
            expressed: Arc::new(modes),
            inherited: None,
        }
    }

    pub(crate) fn cloned_modes(&self) -> Vec<Mode> {
        self.expressed.as_ref().clone()
    }

    pub(crate) fn len(&self) -> usize {
        self.expressed.len()
    }

    pub(crate) fn mode(&self, index: usize) -> &Mode {
        &self.expressed[index]
    }

    #[cfg(test)]
    pub(crate) fn modes_mut(&mut self) -> &mut [Mode] {
        Arc::make_mut(&mut self.expressed).as_mut_slice()
    }

    /// Radiation changes a copy of the genome that the cell passes to offspring.
    pub(crate) fn inherited_modes_mut(&mut self) -> &mut [Mode] {
        Arc::make_mut(
            self.inherited
                .get_or_insert_with(|| Arc::clone(&self.expressed)),
        )
        .as_mut_slice()
    }

    pub(crate) fn inherited_genome(&self) -> Option<Self> {
        self.inherited.as_ref().map(|modes| Self {
            expressed: Arc::clone(modes),
            inherited: None,
        })
    }

    pub(crate) fn with_inherited(mut self, genome: Self) -> Self {
        debug_assert_eq!(self.len(), genome.len());
        self.inherited = Some(genome.expressed);
        self
    }

    pub(crate) fn offspring(&self) -> Self {
        self.inherited_genome().unwrap_or_else(|| self.clone())
    }

    /// Encode the expressed genome into the flat float ABI.
    pub(crate) fn values(&self) -> Vec<f32> {
        let mut output = vec![-5., self.len() as f32];
        for m in self.expressed.iter() {
            output.extend([
                m.kind as f32,
                m.threshold,
                m.child_a as f32,
                m.child_b as f32,
                m.ratio,
                m.speed,
                m.angle,
                m.lifespan,
                m.can_divide as u8 as f32,
                m.bind_children as u8 as f32,
                m.keep_a as u8 as f32,
                m.keep_b as u8 as f32,
                m.child_angle_a,
                m.child_angle_b,
                m.mirror_a as u8 as f32,
                m.mirror_b as u8 as f32,
                m.color[0],
                m.color[1],
                m.color[2],
                m.max_connections.map_or(-1., |limit| limit as f32),
                m.nutrient_priority,
                m.prioritize_when_starving as u8 as f32,
                m.density,
                m.signal_channel as f32,
                m.sensor as f32,
                m.sensor_gain,
                m.target_color[0],
                m.target_color[1],
                m.target_color[2],
                m.sense_tolerance,
                m.secretion as f32,
                m.secretion_rate,
                m.transition_a as f32,
                m.transition_b as f32,
                m.transition_threshold_a,
                m.transition_threshold_b,
                m.offspring_mode as f32,
                m.legacy_gamete_group
                    .map_or(m.compatible_mode as f32, |group| -(f32::from(group) + 1.)),
                m.telomeres as f32,
                m.cilia_left,
                m.cilia_right,
                m.contraction,
                m.bending,
                m.lift,
                if m.virus_copy_from == usize::MAX {
                    -1.
                } else {
                    m.virus_copy_from as f32
                },
                m.program_count as f32,
            ]);
            for program in &m.programs {
                output.extend([
                    program.target as f32,
                    program.input as f32,
                    program.kind as f32,
                    program.scale,
                    program.offset,
                    program.threshold,
                    program.below,
                ]);
            }
        }
        output
    }

    pub(crate) fn legacy_values(&self) -> Vec<f32> {
        self.values()[2..]
            .chunks_exact(MODE_STRIDE)
            .flat_map(|m| m[..12].iter().copied())
            .collect()
    }

    pub(crate) fn values_v4(&self) -> Vec<f32> {
        let mut output = vec![-4., self.len() as f32];
        output.extend(
            self.values()[2..]
                .chunks_exact(MODE_STRIDE)
                .flat_map(|m| m[..V4_MODE_STRIDE].iter().copied()),
        );
        output
    }

    pub(crate) fn values_v3(&self) -> Vec<f32> {
        let mut output = vec![-3., self.len() as f32];
        output.extend(
            self.values()[2..]
                .chunks_exact(MODE_STRIDE)
                .flat_map(|m| m[..V3_MODE_STRIDE].iter().copied()),
        );
        output
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum V4Layout {
    MainSignals,
    BranchResources,
}

pub(crate) fn decode_checked(values: &[f32]) -> Result<Genome, String> {
    decode(values, None)
}

pub(crate) fn decode_v4_checkpoint(values: &[f32], layout: V4Layout) -> Result<Genome, String> {
    if values.first() != Some(&-4.) {
        return Err("Missing version 4 genome header".into());
    }
    let genome = decode(values, Some(layout))?;
    let exact = match layout {
        V4Layout::MainSignals => main_v4::matches_common_fields(&genome, values),
        V4Layout::BranchResources => genome.values_v4() == values,
    };
    if !exact {
        return Err("Некорректный геном в сохранении".into());
    }
    Ok(genome)
}

fn valid_resource_fields(row: &[f32]) -> bool {
    row[19].fract() == 0.
        && (-1. ..=LIMIT as f32).contains(&row[19])
        && (0.25..=4.).contains(&row[20])
        && [0., 1.].contains(&row[21])
}

#[allow(clippy::approx_constant)] // Genes intentionally use a compact ±3.14 radian clamp.
fn decode(values: &[f32], layout: Option<V4Layout>) -> Result<Genome, String> {
    // v1 used eight modes of eight numbers, v2 raw rows of twelve, v3 adds a header.
    let version5 = values.first() == Some(&-5.);
    let version4 = values.first() == Some(&-4.);
    let version3 = values.first() == Some(&-3.);
    let extended = version3 || version4 || version5;
    let count_hint = if extended {
        values.get(1).copied().unwrap_or(0.)
    } else {
        0.
    };
    let values = if extended {
        values.get(2..).unwrap_or(&[])
    } else {
        values
    };
    let legacy = !extended && values.len() == 64;
    let stride = if version5 {
        MODE_STRIDE
    } else if version4 {
        V4_MODE_STRIDE
    } else if version3 {
        V3_MODE_STRIDE
    } else if legacy {
        8
    } else {
        12
    };
    let count = values.len() / stride;
    if count == 0
        || count > MAX_MODES
        || !values.len().is_multiple_of(stride)
        || values.iter().any(|v| !v.is_finite())
        || (extended && count_hint != count as f32)
    {
        return Err("Genome must contain finite numbers in a supported format".into());
    }
    // Both v4 producers used the same header and stride. The shipped main
    // format wins when every row fits it; a world supplies its known layout.
    let layout = layout.unwrap_or_else(|| {
        if version4
            && values
                .chunks_exact(V4_MODE_STRIDE)
                .all(main_v4::valid_signal_fields)
        {
            V4Layout::MainSignals
        } else {
            V4Layout::BranchResources
        }
    });
    let main_v4 = version4 && layout == V4Layout::MainSignals;
    let has_resources = version5 || (version4 && layout == V4Layout::BranchResources);
    let mut modes = vec![Mode::default(); count];
    let max_kind = if version5 || main_v4 {
        KIND_COUNT - 1
    } else if version4 {
        NITROCYTE_KIND
    } else {
        NITROCYTE_KIND - 1
    };
    for (index, (m, v)) in modes
        .iter_mut()
        .zip(values.chunks_exact(stride))
        .enumerate()
    {
        if v[0].fract() != 0.
            || !(0. ..=f32::from(max_kind)).contains(&v[0])
            || [v[2], v[3]]
                .iter()
                .any(|n| n.fract() != 0. || *n < 0. || *n >= count as f32)
            || (!legacy && v[8..12].iter().any(|n| *n != 0. && *n != 1.))
        {
            return Err("Invalid archetype type, child reference or adhesion flag".into());
        }
        if extended
            && (v[12..14]
                .iter()
                .any(|a| a.abs() > std::f32::consts::PI + 0.000001)
                || v[14..16].iter().any(|v| *v != 0. && *v != 1.)
                || !(v[16..19].iter().all(|v| *v == -1.)
                    || v[16..19].iter().all(|v| (-0.000001..=1.000001).contains(v))))
        {
            return Err("Invalid daughter angle, mirror flag or color".into());
        }
        if has_resources && !valid_resource_fields(v) {
            return Err("Invalid connection limit or nutrient priority".into());
        }
        if main_v4 && !main_v4::valid_signal_fields(v) {
            return Err("Invalid signal channel, subtype or strength".into());
        }

        if version5 {
            let integer = |index: usize, max: usize| {
                v[index].fract() == 0. && (0. ..=max as f32).contains(&v[index])
            };
            let legacy_group = v[37].fract() == 0. && (-16. ..=-1.).contains(&v[37]);
            if !(-3.3..=3.3).contains(&v[22])
                || !integer(23, 7)
                || !integer(24, 8)
                || v[25].abs() > 16.
                || !(v[26..29].iter().all(|value| *value == -1.)
                    || v[26..29].iter().all(|value| (0. ..=1.).contains(value)))
                || !(0. ..=1.).contains(&v[29])
                || !integer(30, 22)
                || !(0. ..=4.).contains(&v[31])
                || [32, 33, 36].iter().any(|&index| !integer(index, count - 1))
                || !(integer(37, count - 1) || legacy_group)
                || !(v[44] == -1. || integer(44, count - 1))
                || v[34..36].iter().any(|value| value.abs() > 1000.)
                || v[38].fract() != 0.
                || !(-1. ..=1000.).contains(&v[38])
                || v[39..41].iter().any(|value| value.abs() > 60.)
                || v[41..44].iter().any(|value| value.abs() > 1.)
                || !integer(45, 8)
            {
                return Err("Invalid cell-specific settings".into());
            }
            for row in v[46..].chunks_exact(7) {
                if row[0].fract() != 0.
                    || !(0. ..=17.).contains(&row[0])
                    || row[1].fract() != 0.
                    || !(0. ..=13.).contains(&row[1])
                    || ![0., 1.].contains(&row[2])
                    || row[3..].iter().any(|value| value.abs() > 1000.)
                {
                    return Err("Invalid cell program".into());
                }
            }
        }
        *m = Mode {
            kind: v[0] as u8,
            threshold: v[1].clamp(1., 8.),
            child_a: v[2] as usize,
            child_b: v[3] as usize,
            ratio: v[4].clamp(0.2, 0.8),
            speed: v[5].clamp(0., 60.),
            angle: v[6].clamp(-3.14, 3.14),
            lifespan: v[7].clamp(20., 1000.),
            can_divide: legacy || v[8] == 1.,
            bind_children: !legacy && v[9] == 1.,
            keep_a: !legacy && v[10] == 1.,
            keep_b: !legacy && v[11] == 1.,
            child_angle_a: if extended {
                v[12].clamp(-std::f32::consts::PI, std::f32::consts::PI)
            } else {
                0.
            },
            child_angle_b: if extended {
                v[13].clamp(-std::f32::consts::PI, std::f32::consts::PI)
            } else {
                0.
            },
            mirror_a: extended && v[14] == 1.,
            mirror_b: extended && v[15] == 1.,
            color: if extended {
                [v[16], v[17], v[18]].map(|c| if c == -1. { c } else { c.clamp(0., 1.) })
            } else {
                [-1.; 3]
            },
            max_connections: if has_resources && v[19] >= 0. {
                Some(v[19] as usize)
            } else {
                None
            },
            nutrient_priority: if has_resources { v[20] } else { 1. },
            prioritize_when_starving: has_resources && v[21] == 1.,
            transition_a: index,
            transition_b: index,
            offspring_mode: index,
            compatible_mode: index,
            ..Mode::default()
        };
        if main_v4 {
            main_v4::migrate_mode(m, v[19] as u8, v[20] as u8, v[21]);
        }
        if version5 {
            m.density = v[22];
            m.signal_channel = v[23] as u8;
            m.sensor = v[24] as u8;
            m.sensor_gain = v[25];
            m.target_color = [v[26], v[27], v[28]];
            m.sense_tolerance = v[29];
            m.secretion = v[30] as u8;
            m.secretion_rate = v[31];
            m.transition_a = v[32] as usize;
            m.transition_b = v[33] as usize;
            m.transition_threshold_a = v[34];
            m.transition_threshold_b = v[35];
            m.offspring_mode = v[36] as usize;
            // Negative values carry v4 compatibility groups in the same ABI slot.
            if v[37] < 0. {
                m.legacy_gamete_group = Some((-v[37] - 1.) as u8);
            } else {
                m.compatible_mode = v[37] as usize;
            }
            m.telomeres = v[38] as i32;
            m.cilia_left = v[39];
            m.cilia_right = v[40];
            m.contraction = v[41];
            m.bending = v[42];
            m.lift = v[43];
            m.virus_copy_from = if v[44] < 0. {
                usize::MAX
            } else {
                v[44] as usize
            };
            m.program_count = v[45] as usize;
            for (program, row) in m.programs.iter_mut().zip(v[46..].chunks_exact(7)) {
                *program = Program {
                    target: row[0] as u8,
                    input: row[1] as u8,
                    kind: row[2] as u8,
                    scale: row[3],
                    offset: row[4],
                    threshold: row[5],
                    below: row[6],
                };
            }
        }
    }
    Ok(Genome::from_modes(modes))
}
