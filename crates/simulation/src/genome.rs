//! Genome archetypes, validation and ABI encoding.

use wasm_bindgen::prelude::*;

pub(crate) const MODE_STRIDE: usize = 19;

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
        }
    }
}

#[derive(Clone)]
pub(crate) struct Genome(Vec<Mode>);

impl Genome {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn mode(&self, index: usize) -> &Mode {
        &self.0[index]
    }

    pub(crate) fn modes_mut(&mut self) -> &mut [Mode] {
        &mut self.0
    }

    /// Encode the genome back into the flat float ABI.
    pub(crate) fn values(&self) -> Vec<f32> {
        let mut output = vec![-3., self.len() as f32];
        output.extend(self.0.iter().flat_map(|m| {
            [
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
            ]
        }));
        output
    }

    pub(crate) fn legacy_values(&self) -> Vec<f32> {
        self.values()[2..]
            .chunks_exact(MODE_STRIDE)
            .flat_map(|m| m[..12].iter().copied())
            .collect()
    }
}

pub(crate) fn decode(values: &[f32]) -> Result<Genome, JsError> {
    decode_checked(values).map_err(|message| JsError::new(&message))
}

pub(crate) fn decode_checked(values: &[f32]) -> Result<Genome, String> {
    // v1 used eight modes of eight numbers, v2 raw rows of twelve; v3 adds a header.
    let version3 = values.first() == Some(&-3.);
    let count_hint = if version3 {
        values.get(1).copied().unwrap_or(0.)
    } else {
        0.
    };
    let values = if version3 {
        values.get(2..).unwrap_or(&[])
    } else {
        values
    };
    let legacy = !version3 && values.len() == 64;
    let stride = if version3 {
        MODE_STRIDE
    } else if legacy {
        8
    } else {
        12
    };
    let count = values.len() / stride;
    if count == 0
        || values.len() % stride != 0
        || values.iter().any(|v| !v.is_finite())
        || (version3 && count_hint != count as f32)
    {
        return Err(
            "Genome must contain finite numbers in v3 rows of 19, v2 rows of 12 or 64 legacy numbers"
                .into(),
        );
    }
    let mut modes = vec![Mode::default(); count];
    for (m, v) in modes.iter_mut().zip(values.chunks_exact(stride)) {
        if v[0].fract() != 0.
            || !(0. ..=5.).contains(&v[0])
            || [v[2], v[3]]
                .iter()
                .any(|n| n.fract() != 0. || *n < 0. || *n >= count as f32)
            || (!legacy && v[8..12].iter().any(|n| *n != 0. && *n != 1.))
        {
            return Err("Invalid archetype type, child reference or adhesion flag".into());
        }
        if version3
            && (v[12..14]
                .iter()
                .any(|a| a.abs() > std::f32::consts::PI + 0.000001)
                || v[14..16].iter().any(|v| *v != 0. && *v != 1.)
                || !(v[16..19].iter().all(|v| *v == -1.)
                    || v[16..19].iter().all(|v| (-0.000001..=1.000001).contains(v))))
        {
            return Err("Invalid daughter angle, mirror flag or color".into());
        }
        *m = Mode {
            kind: v[0].clamp(0., 5.) as u8,
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
            child_angle_a: if version3 {
                v[12].clamp(-std::f32::consts::PI, std::f32::consts::PI)
            } else {
                0.
            },
            child_angle_b: if version3 {
                v[13].clamp(-std::f32::consts::PI, std::f32::consts::PI)
            } else {
                0.
            },
            mirror_a: version3 && v[14] == 1.,
            mirror_b: version3 && v[15] == 1.,
            color: if version3 {
                [v[16], v[17], v[18]].map(|c| if c == -1. { c } else { c.clamp(0., 1.) })
            } else {
                [-1.; 3]
            },
        };
    }
    Ok(Genome(modes))
}
