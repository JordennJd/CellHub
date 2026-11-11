//! Finite signal expressions shared by sensors, muscles and lifecycle controls.

use crate::components::Cell;
use crate::genome::{Mode, Program};

pub(super) const SIGNAL_CHANNELS: usize = 8;
pub(super) const SPEED_TARGET: usize = 8;
pub(super) const DENSITY_TARGET: usize = 9;
pub(super) const CONTRACTION_TARGET: usize = 10;
pub(super) const BENDING_TARGET: usize = 11;
pub(super) const LIFT_TARGET: usize = 12;
pub(super) const CILIA_LEFT_TARGET: usize = 13;
pub(super) const CILIA_RIGHT_TARGET: usize = 14;
pub(super) const SECRETION_RATE_TARGET: usize = 15;
pub(super) const DIFFERENTIATION_A_TARGET: usize = 16;
pub(super) const DIFFERENTIATION_B_TARGET: usize = 17;

fn input(cell: &Cell, source: u8, connections: usize) -> f32 {
    match source {
        0 => 1.,
        1..=8 => cell.signals[usize::from(source - 1)],
        9 => cell.mass,
        10 => cell.age,
        11 => cell.nitrogen,
        12 => connections as f32,
        13 => libm::sinf(cell.age),
        _ => 0.,
    }
}

fn evaluate(cell: &Cell, rule: &Program, connections: usize) -> f32 {
    let value = input(cell, rule.input, connections);
    if rule.kind == 1 && value < rule.threshold {
        rule.below
    } else {
        rule.offset + rule.scale * value
    }
}

/// Matching equations add their contributions in their stored order. Controls
/// without equations keep the explicit constant from the mode.
pub(super) fn controlled_value(cell: &Cell, mode: &Mode, target: usize, connections: usize) -> f32 {
    let mut sum = 0_f64;
    let mut programmed = false;
    for equation in mode.programs.iter().take(mode.program_count) {
        if usize::from(equation.target) == target {
            sum += f64::from(evaluate(cell, equation, connections));
            programmed = true;
        }
    }
    let value = if programmed {
        sum as f32
    } else {
        match target {
            0..=7 => {
                if usize::from(mode.signal_channel) == target {
                    mode.sensor_gain
                } else {
                    0.
                }
            }
            SPEED_TARGET => mode.speed,
            DENSITY_TARGET => mode.density,
            CONTRACTION_TARGET => mode.contraction,
            BENDING_TARGET => mode.bending,
            LIFT_TARGET => mode.lift,
            CILIA_LEFT_TARGET => mode.cilia_left,
            CILIA_RIGHT_TARGET => mode.cilia_right,
            SECRETION_RATE_TARGET => mode.secretion_rate,
            DIFFERENTIATION_A_TARGET => {
                cell.signals[usize::from(mode.signal_channel)] - mode.transition_threshold_a
            }
            DIFFERENTIATION_B_TARGET => {
                cell.signals[usize::from(mode.signal_channel)] - mode.transition_threshold_b
            }
            _ => 0.,
        }
    };
    match target {
        SPEED_TARGET => value.clamp(0., 60.),
        DENSITY_TARGET => value.clamp(-3.3, 3.3),
        CONTRACTION_TARGET..=LIFT_TARGET => value.clamp(-1., 1.),
        CILIA_LEFT_TARGET..=CILIA_RIGHT_TARGET => value.clamp(-60., 60.),
        SECRETION_RATE_TARGET => value.clamp(0., 4.),
        _ => value.clamp(-8., 8.),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell() -> Cell {
        Cell {
            id: 1,
            mode: 0,
            mass: 2.,
            nitrogen: 0.5,
            age: 3.,
            generation: 0,
            mutations: 0,
            cooldown: 0.,
            mirrored: false,
            signals: [-0.5, 0.25, 0., 0., 0., 0., 0., 0.],
            toxin: 0.,
            injury: 0.,
            telomeres: -1,
            anchored: false,
            class: crate::components::CLASS_USER,
        }
    }

    #[test]
    fn equations_add_in_order_and_keep_signed_signals() {
        let mut mode = Mode::default();
        mode.program_count = 2;
        mode.programs[0] = Program {
            target: 0,
            input: 1,
            scale: 2.,
            ..Program::default()
        };
        mode.programs[1] = Program {
            target: 0,
            input: 0,
            offset: -0.5,
            ..Program::default()
        };
        assert_eq!(controlled_value(&cell(), &mode, 0, 0), -1.5);
        assert_eq!(
            controlled_value(&cell(), &mode, SPEED_TARGET, 0),
            mode.speed
        );
    }

    #[test]
    fn threshold_rules_use_mass_age_nitrogen_and_connection_inputs() {
        let mut mode = Mode::default();
        mode.program_count = 1;
        mode.programs[0] = Program {
            target: DIFFERENTIATION_A_TARGET as u8,
            input: 9,
            kind: 1,
            scale: 0.,
            offset: 1.,
            threshold: 2.,
            below: -1.,
        };
        assert_eq!(
            controlled_value(&cell(), &mode, DIFFERENTIATION_A_TARGET, 0),
            1.
        );
        mode.programs[0].threshold = 2.1;
        assert_eq!(
            controlled_value(&cell(), &mode, DIFFERENTIATION_A_TARGET, 0),
            -1.
        );
        for (source, threshold) in [(10, 3.), (11, 0.5), (12, 4.)] {
            mode.programs[0].input = source;
            mode.programs[0].threshold = threshold;
            assert_eq!(
                controlled_value(&cell(), &mode, DIFFERENTIATION_A_TARGET, 4),
                1.
            );
        }
    }
}
