//! The local laboratory uses a circular dish; the server retains its square world.

use crate::components::{SIZE, radius};

#[derive(Clone, Copy, Default)]
pub(super) enum Boundary {
    #[default]
    Square,
    Circle,
}

impl Boundary {
    /// Keep the full cell inside a dish, while preserving legacy square margins.
    pub(super) fn constrain(
        self,
        x: f32,
        y: f32,
        mass: f32,
        square_margin: f32,
    ) -> (f32, f32, bool) {
        match self {
            Self::Square => (
                x.clamp(square_margin, SIZE - square_margin),
                y.clamp(square_margin, SIZE - square_margin),
                !(square_margin..=SIZE - square_margin).contains(&x)
                    || !(square_margin..=SIZE - square_margin).contains(&y),
            ),
            Self::Circle => {
                let center = SIZE / 2.;
                let dx = x - center;
                let dy = y - center;
                let distance = dx.hypot(dy);
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
