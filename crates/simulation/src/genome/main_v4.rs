//! Migration of the deployed v4 genome, which preceded the resource fields.

use super::{Genome, Mode, Program, V3_MODE_STRIDE, V4_MODE_STRIDE};
use crate::cell_types::{
    BUOYOCYTE, CILIOCYTE, FLAGELLOCYTE, GAMETE, GLUEOCYTE, LIPOCYTE, MYOCYTE, NEUROCYTE, NITROCYTE,
    SECROCYTE, SENSEOCYTE, STEMOCYTE, STEREOCYTE,
};

fn kind(id: u8) -> u8 {
    match id {
        6 => LIPOCYTE,
        7 => BUOYOCYTE,
        8 => GLUEOCYTE,
        9 => NITROCYTE,
        _ => id,
    }
}

pub(crate) fn normalize_signal(value: f32) -> f32 {
    value / 12.5
}

pub(super) fn valid_signal_fields(row: &[f32]) -> bool {
    row[19].fract() == 0.
        && (0. ..=3.).contains(&row[19])
        && row[20].fract() == 0.
        && (0. ..=15.).contains(&row[20])
        && (-60. ..=60.).contains(&row[21])
}

pub(super) fn matches_common_fields(genome: &Genome, original: &[f32]) -> bool {
    genome.values_v3()[2..]
        .chunks_exact(V3_MODE_STRIDE)
        .zip(original[2..].chunks_exact(V4_MODE_STRIDE))
        .all(|(current, old)| {
            current[0] == f32::from(kind(old[0] as u8)) && current[1..] == old[1..V3_MODE_STRIDE]
        })
}

fn rule(target: u8, input: u8, scale: f32, offset: f32, threshold: f32, below: f32) -> Program {
    Program {
        target,
        input,
        kind: 1,
        scale,
        offset,
        threshold,
        below,
    }
}

pub(super) fn migrate_mode(mode: &mut Mode, channel: u8, subtype: u8, signal: f32) {
    mode.kind = kind(mode.kind);
    match mode.kind {
        FLAGELLOCYTE => {
            // v4 scaled flagellar speed by 1 + clamp(S1/60, −0.8, 1).
            let speed = mode.speed;
            let slope = (f64::from(speed) / 4.8) as f32;
            mode.program_count = 2;
            mode.programs[0] = rule(8, 1, slope, speed, -3.84, speed / 5.);
            mode.programs[1] = rule(8, 1, -slope, speed, 4.8, 0.);
        }
        LIPOCYTE => mode.nutrient_priority = 4.,
        BUOYOCYTE => mode.density = (-signal).clamp(-3.3, 3.3),
        GLUEOCYTE => mode.max_connections = Some(signal.round().clamp(1., 5.) as usize),
        STEREOCYTE | SENSEOCYTE => {
            mode.signal_channel = channel;
            mode.sensor = match subtype {
                0 => 3,
                1 => 1,
                2 => 7,
                3 => 0,
                4 => 4,
                5 => 8,
                _ => 3,
            };
            mode.sensor_gain = if subtype <= 5 {
                normalize_signal(signal.max(0.))
            } else {
                0.
            };
        }
        MYOCYTE => {
            // The deployed muscle used S1 clamped to 0..60 as its activation.
            let strength = mode.speed / 60.;
            let slope = mode.speed / 288.;
            mode.program_count = 2;
            mode.programs[0] = rule(10, 1, slope, 0., 0., 0.);
            mode.programs[1] = rule(10, 1, -slope, strength, 4.8, 0.);
        }
        NEUROCYTE => {
            mode.signal_channel = channel;
            mode.sensor_gain = normalize_signal(signal);
        }
        SECROCYTE => {
            mode.secretion = match subtype {
                0..=2 => subtype,
                3..=6 => 7 + 2 * (subtype - 3),
                7..=10 => 8 + 2 * (subtype - 7),
                11..=14 => subtype - 8,
                _ => 0,
            };
            let minimum = if subtype <= 1 { 1. } else { 0. };
            mode.secretion_rate = if subtype <= 14 {
                signal.max(minimum) / 15.
            } else {
                0.
            };
        }
        STEMOCYTE => {
            mode.transition_a = mode.child_a;
            mode.transition_b = mode.child_b;
            mode.program_count = 2;
            mode.programs[0] = rule(16, 1, 0., 1., 0.04, -1.);
            mode.programs[1] = rule(17, 2, 0., 1., 0.04, -1.);
        }
        GAMETE => {
            mode.offspring_mode = mode.child_a;
            mode.legacy_gamete_group = Some(subtype);
        }
        CILIOCYTE => {
            mode.cilia_left = mode.speed * 0.5;
            mode.cilia_right = mode.speed * 0.5;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::{MODE_STRIDE, V4Layout, decode_checked, decode_v4_checkpoint};
    use super::*;

    fn genome(rows: &[(u8, u8, u8, f32)]) -> Vec<f32> {
        let mut genes = vec![-4., rows.len() as f32];
        for &(kind, channel, subtype, strength) in rows {
            genes.extend([
                kind as f32,
                2.4,
                0.,
                0.,
                0.5,
                12.,
                0.8,
                240.,
                1.,
                0.,
                0.,
                0.,
                0.,
                0.,
                0.,
                0.,
                -1.,
                -1.,
                -1.,
                channel as f32,
                subtype as f32,
                strength,
            ]);
        }
        genes
    }

    #[test]
    fn raw_v4_uses_main_for_ambiguous_rows_and_resources_when_distinct() {
        let mut genes = genome(&[(6, 1, 1, 1.)]);
        let main = decode_checked(&genes).unwrap();
        assert_eq!(main.mode(0).kind, LIPOCYTE);
        assert_eq!(main.mode(0).nutrient_priority, 4.);

        let branch = decode_v4_checkpoint(&genes, V4Layout::BranchResources).unwrap();
        assert_eq!(branch.mode(0).kind, NITROCYTE);
        assert_eq!(branch.mode(0).max_connections, Some(1));
        assert!(branch.mode(0).prioritize_when_starving);

        genes[21] = 5.;
        let branch = decode_checked(&genes).unwrap();
        assert_eq!(branch.mode(0).kind, NITROCYTE);
        assert_eq!(branch.mode(0).max_connections, Some(5));
    }

    #[test]
    fn main_genes_translate_types_and_type_specific_settings() {
        let genes = genome(&[
            (7, 0, 0, 1.25),
            (8, 0, 0, 3.4),
            (9, 0, 0, 30.),
            (11, 3, 2, 30.),
            (13, 2, 0, -30.),
            (14, 0, 8, 45.),
            (15, 0, 0, 0.),
            (16, 0, 4, 0.),
            (17, 0, 0, 0.),
        ]);
        let genome = decode_v4_checkpoint(&genes, V4Layout::MainSignals).unwrap();
        assert_eq!(genome.mode(0).kind, BUOYOCYTE);
        assert_eq!(genome.mode(0).density, -1.25);
        assert_eq!(genome.mode(1).kind, GLUEOCYTE);
        assert_eq!(genome.mode(1).max_connections, Some(3));
        assert_eq!(genome.mode(2).kind, NITROCYTE);
        assert_eq!(genome.mode(3).signal_channel, 3);
        assert_eq!(genome.mode(3).sensor, 7);
        assert_eq!(genome.mode(3).sensor_gain, 2.4);
        assert_eq!(genome.mode(4).sensor_gain, -2.4);
        assert_eq!(genome.mode(5).secretion, 10);
        assert_eq!(genome.mode(5).secretion_rate, 3.);
        assert_eq!(genome.mode(6).transition_a, 0);
        assert_eq!(genome.mode(6).programs[1].input, 2);
        assert_eq!(genome.mode(7).offspring_mode, 0);
        assert_eq!(genome.mode(7).legacy_gamete_group, Some(4));
        assert_eq!(genome.mode(8).cilia_right, 6.);
        let mut current = genome.values();
        let gamete_start = 2 + 7 * MODE_STRIDE;
        assert_eq!(current[gamete_start + 37], -5.);
        assert_eq!(decode_checked(&current).unwrap().values(), current);
        current[gamete_start] = 0.; // Type mutation must preserve the group.
        let mutated = decode_checked(&current).unwrap();
        assert_eq!(mutated.mode(7).legacy_gamete_group, Some(4));
        assert_eq!(mutated.mode(7).compatible_mode, 7);
        current[gamete_start + 37] = -17.;
        assert!(decode_checked(&current).is_err());

        let mut invalid = genes;
        invalid[3] = 80.;
        assert!(decode_checked(&invalid).is_ok());
        assert!(decode_v4_checkpoint(&invalid, V4Layout::MainSignals).is_err());
    }
}
