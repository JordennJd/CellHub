//! Genome archetypes, validation and ABI encoding.

use wasm_bindgen::prelude::*;

pub(crate) const MAX_MODES: usize = 40;
pub(crate) const MODE_STRIDE: usize = 12;

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
        self.0
            .iter()
            .flat_map(|m| {
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
                ]
            })
            .collect()
    }
}

pub(crate) fn decode(values: &[f32]) -> Result<Genome, JsError> {
    // v1 used exactly eight modes of eight numbers. v2 uses 1–40 modes of twelve.
    let legacy = values.len() == 64;
    let stride = if legacy { 8 } else { MODE_STRIDE };
    let count = values.len() / stride;
    if count == 0
        || count > MAX_MODES
        || values.len() % stride != 0
        || values.iter().any(|v| !v.is_finite())
    {
        return Err(JsError::new(
            "Genome must contain 1–40 archetypes of 12 finite numbers (or 64 legacy numbers)",
        ));
    }
    let mut modes = vec![Mode::default(); count];
    for (m, v) in modes.iter_mut().zip(values.chunks_exact(stride)) {
        if v[0].fract() != 0.
            || !(0. ..=5.).contains(&v[0])
            || [v[2], v[3]]
                .iter()
                .any(|n| n.fract() != 0. || *n < 0. || *n >= count as f32)
            || (!legacy && v[8..].iter().any(|n| *n != 0. && *n != 1.))
        {
            return Err(JsError::new(
                "Invalid archetype type, child reference or adhesion flag",
            ));
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
        };
    }
    Ok(Genome(modes))
}
