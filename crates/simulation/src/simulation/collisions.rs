//! Pairwise circle contacts within a deterministic interaction island.

use super::boundary::Boundary;
use crate::components::{MAX_RECOIL_MOMENTUM, Position, radius};

pub(super) const CONTACT_TRAVEL: f32 = 6.;
pub(super) const CONTACT_PASSES: usize = 5;

const CONTACT_SLOP: f32 = 0.005;
const RESTITUTION: f32 = 0.15;
const PENETRATION_BIAS: f32 = 0.2;
const MAX_BIAS_SPEED: f32 = 12.;

pub(super) struct CollisionBody {
    pub(super) position: Position,
    pub(super) momentum: [f32; 2],
    path: [Position; 3],
    moving: bool,
    drive: [f32; 2],
    min: [f32; 2],
    max: [f32; 2],
    mass: f32,
    inverse_mass: f32,
    radius: f32,
}

impl CollisionBody {
    pub(super) fn new(
        path: [Position; 3],
        drive: [f32; 2],
        momentum: [f32; 2],
        mass: f32,
        anchored: bool,
    ) -> Self {
        let [start, driven, position] = path;
        Self {
            position,
            momentum,
            path,
            moving: start.x != driven.x || start.y != driven.y,
            drive: if anchored { [0.; 2] } else { drive },
            min: [
                start.x.min(driven.x).min(position.x) - CONTACT_TRAVEL,
                start.y.min(driven.y).min(position.y) - CONTACT_TRAVEL,
            ],
            max: [
                start.x.max(driven.x).max(position.x) + CONTACT_TRAVEL,
                start.y.max(driven.y).max(position.y) + CONTACT_TRAVEL,
            ],
            mass,
            inverse_mass: if anchored { 0. } else { mass.recip() },
            radius: radius(mass),
        }
    }

    /// Solve one pair, with the normal pointing from this cell to the other.
    /// Only the first pass adds bounce or a separating speed for penetration.
    pub(super) fn solve(
        &mut self,
        other: &mut Self,
        pair: (u32, u32),
        step: f32,
        first_pass: bool,
        fusion_scale: Option<f32>,
        boundary: Boundary,
    ) -> bool {
        let inverse_mass = self.inverse_mass + other.inverse_mass;
        if inverse_mass == 0. {
            return false;
        }
        let separation = (self.radius + other.radius) * fusion_scale.unwrap_or(1.).min(1.);
        let elastic = fusion_scale.is_none();
        let mut changed = false;
        if first_pass
            && (self.moving || other.moving)
            && let Some(impact) = swept_impact(&self.path, &other.path, separation)
        {
            let penetration = separation - self.projected_distance(other, impact.normal);
            if penetration >= 0. {
                let restitution = if elastic { RESTITUTION } else { 0. };
                changed |= self.impulse(other, impact.normal, inverse_mass, 0., restitution);
                changed |= self.separate(other, impact.normal, penetration, inverse_mass, boundary);
                if self.projected_distance(other, impact.normal) < separation - CONTACT_SLOP {
                    // A lateral travel bound can prevent plane projection.
                    // The impact poses themselves stay on the original paths.
                    changed |= self.rewind(impact.time, boundary);
                    changed |= other.rewind(impact.time, boundary);
                }
                return changed;
            }
        }

        let dx = other.position.x - self.position.x;
        let dy = other.position.y - self.position.y;
        let distance_squared = dx * dx + dy * dy;
        if distance_squared > separation * separation {
            return false;
        }
        let distance = libm::sqrtf(distance_squared);
        let normal = if distance > 0.0001 {
            [dx / distance, dy / distance]
        } else {
            coincident_normal(pair)
        };
        let penetration = (separation - distance - CONTACT_SLOP).max(0.);
        let (bias, restitution) = if first_pass && elastic {
            (
                (PENETRATION_BIAS * penetration / step).min(MAX_BIAS_SPEED),
                RESTITUTION,
            )
        } else {
            (0., 0.)
        };
        changed |= self.impulse(other, normal, inverse_mass, bias, restitution);
        changed |= self.separate(other, normal, penetration, inverse_mass, boundary);
        changed
    }

    pub(super) fn within_contact_range(&self, other: &Self, fraction: f32) -> bool {
        let dx = other.position.x - self.position.x;
        let dy = other.position.y - self.position.y;
        let reach = (self.radius + other.radius) * fraction;
        dx * dx + dy * dy <= reach * reach
    }

    fn projected_distance(&self, other: &Self, [nx, ny]: [f32; 2]) -> f32 {
        (other.position.x - self.position.x) * nx + (other.position.y - self.position.y) * ny
    }

    fn impulse(
        &mut self,
        other: &mut Self,
        normal: [f32; 2],
        inverse_mass: f32,
        bias: f32,
        restitution: f32,
    ) -> bool {
        let mut normal_speed = 0.;
        for (axis, direction) in normal.into_iter().enumerate() {
            let relative = other.drive[axis] - self.drive[axis]
                + other.momentum[axis] * other.inverse_mass
                - self.momentum[axis] * self.inverse_mass;
            normal_speed += relative * direction;
        }
        if normal_speed > 0. {
            return false;
        }
        let outgoing_speed = bias.max(-normal_speed * restitution);
        let amount = (outgoing_speed - normal_speed) / inverse_mass;
        let mut changed = false;
        for (axis, direction) in normal.into_iter().enumerate() {
            let component = direction * amount;
            changed |= self.add_momentum(axis, -component);
            changed |= other.add_momentum(axis, component);
        }
        changed
    }

    fn add_momentum(&mut self, axis: usize, impulse: f32) -> bool {
        if self.inverse_mass == 0. {
            return false;
        }
        let previous = self.momentum[axis];
        self.momentum[axis] = (previous + impulse).clamp(-MAX_RECOIL_MOMENTUM, MAX_RECOIL_MOMENTUM);
        self.momentum[axis] != previous
    }

    fn separate(
        &mut self,
        other: &mut Self,
        [nx, ny]: [f32; 2],
        penetration: f32,
        inverse_mass: f32,
        boundary: Boundary,
    ) -> bool {
        if penetration <= 0. {
            return false;
        }
        let a = penetration * (self.inverse_mass / inverse_mass);
        let b = penetration * (other.inverse_mass / inverse_mass);
        let moved = self.place(self.position.x - nx * a, self.position.y - ny * a, boundary);
        other.place(
            other.position.x + nx * b,
            other.position.y + ny * b,
            boundary,
        ) || moved
    }

    fn rewind(&mut self, time: f32, boundary: Boundary) -> bool {
        let [start, driven, _] = self.path;
        self.place(
            start.x + (driven.x - start.x) * time,
            start.y + (driven.y - start.y) * time,
            boundary,
        )
    }

    fn place(&mut self, x: f32, y: f32, boundary: Boundary) -> bool {
        if self.inverse_mass == 0. {
            return false;
        }
        let (x, y, _) = boundary.constrain(
            x.clamp(self.min[0], self.max[0]),
            y.clamp(self.min[1], self.max[1]),
            self.mass,
            12.,
        );
        // The circular rim can project a coordinate outside the travel box.
        // In that case retain the last feasible position to protect the broad phase.
        if !(self.min[0]..=self.max[0]).contains(&x) || !(self.min[1]..=self.max[1]).contains(&y) {
            return false;
        }
        let changed = self.position.x != x || self.position.y != y;
        self.position.x = x;
        self.position.y = y;
        changed
    }
}

struct Impact {
    time: f32,
    normal: [f32; 2],
}

fn swept_impact(a: &[Position; 3], b: &[Position; 3], separation: f32) -> Option<Impact> {
    // Use only the free-motion segment. Bond corrections are projections,
    // not additional movement with a physical velocity.
    let x = f64::from(b[0].x) - f64::from(a[0].x);
    let y = f64::from(b[0].y) - f64::from(a[0].y);
    let dx = (f64::from(b[1].x) - f64::from(a[1].x)) - x;
    let dy = (f64::from(b[1].y) - f64::from(a[1].y)) - y;
    let separation = f64::from(separation);
    let distance_squared = x * x + y * y;
    let outside = distance_squared - separation * separation;
    let approach = x * dx + y * dy;
    if approach >= 0. {
        return None;
    }
    if outside < 0. {
        // Let ordinary overlaps use the end positions. If the centers would
        // swap sides, preserve their initial order even from inside contact.
        if distance_squared + approach > 0. {
            return None;
        }
        let distance = libm::sqrt(distance_squared);
        return Some(Impact {
            time: 0.,
            normal: [(x / distance) as f32, (y / distance) as f32],
        });
    }
    let motion = dx * dx + dy * dy;
    let discriminant = approach * approach - motion * outside;
    if discriminant < 0. {
        return None;
    }
    // Equivalent to the smaller quadratic root, with less cancellation
    // when the centers start very close to contact.
    let time = outside / (-approach + libm::sqrt(discriminant));
    if time > 1. {
        return None;
    }
    let x = x + dx * time;
    let y = y + dy * time;
    let distance = libm::hypot(x, y);
    Some(Impact {
        time: time as f32,
        normal: [(x / distance) as f32, (y / distance) as f32],
    })
}

fn coincident_normal((a, b): (u32, u32)) -> [f32; 2] {
    const DIAGONAL: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const DIRECTIONS: [[f32; 2]; 8] = [
        [1., 0.],
        [DIAGONAL, DIAGONAL],
        [0., 1.],
        [-DIAGONAL, DIAGONAL],
        [-1., 0.],
        [-DIAGONAL, -DIAGONAL],
        [0., -1.],
        [DIAGONAL, -DIAGONAL],
    ];
    let hash = a.wrapping_mul(0x9e3779b9) ^ b.wrapping_mul(0x85ebca6b);
    DIRECTIONS[((hash ^ (hash >> 16)) & 7) as usize]
}
