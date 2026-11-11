//! Rectangular local fields, legacy circular saves and the square server world.

use crate::components::{SIZE, radius};

#[derive(Clone, Copy, Default)]
pub(super) enum Boundary {
    #[default]
    Square,
    Circle,
    Chaos,
    Rectangle {
        width: f32,
        height: f32,
    },
}

impl Boundary {
    pub(super) fn size(self) -> f32 {
        if let Self::Rectangle { height, .. } = self {
            return height;
        }
        if matches!(self, Self::Chaos) {
            crate::terrain::CHAOS_SIZE
        } else {
            SIZE
        }
    }

    pub(super) fn width(self) -> f32 {
        if let Self::Rectangle { width, .. } = self {
            width
        } else {
            self.size()
        }
    }
    pub(super) fn columns(self) -> usize {
        (self.width() / 64.) as usize
    }
    pub(super) fn rows(self) -> usize {
        (self.size() / 64.) as usize
    }
    pub(super) fn tile(self, x: f32, y: f32) -> usize {
        crate::terrain::index_rect(x, y, self.columns(), self.rows())
    }
    pub(super) fn wall_distance(self, x: f32, y: f32) -> f32 {
        match self {
            Self::Square | Self::Rectangle { .. } => {
                x.min(y).min(self.width() - x).min(self.size() - y)
            }
            Self::Circle | Self::Chaos => {
                self.size() / 2. - libm::hypotf(x - self.size() / 2., y - self.size() / 2.)
            }
        }
    }

    /// Keep the full cell inside a dish, while preserving legacy square margins.
    pub(super) fn constrain(
        self,
        x: f32,
        y: f32,
        mass: f32,
        square_margin: f32,
    ) -> (f32, f32, bool) {
        match self {
            Self::Square | Self::Rectangle { .. } => {
                let margin = if matches!(self, Self::Square) {
                    square_margin
                } else {
                    square_margin.max(radius(mass))
                };
                (
                    x.clamp(margin, self.width() - margin),
                    y.clamp(margin, self.size() - margin),
                    !(margin..=self.width() - margin).contains(&x)
                        || !(margin..=self.size() - margin).contains(&y),
                )
            }
            Self::Circle | Self::Chaos => {
                let center = self.size() / 2.;
                let dx = x - center;
                let dy = y - center;
                let distance = libm::hypotf(dx, dy);
                let limit = (center - radius(mass).max(square_margin)).max(0.);
                if distance > limit {
                    let scale = limit / distance;
                    (center + dx * scale, center + dy * scale, true)
                } else {
                    (x, y, false)
                }
            }
        }
    }
}
