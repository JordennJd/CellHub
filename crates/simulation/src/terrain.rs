//! Static substrate map and its tile layout.

pub(crate) const GRID: usize = 48;

#[derive(Clone, Copy)]
pub(crate) struct Tile {
    pub(crate) light: f32,
    pub(crate) radiation: f32,
    pub(crate) food: f32,
    pub(crate) capacity: f32,
    pub(crate) biome: f32,
    pub(crate) nitrates: f32,
    pub(crate) nitrate_capacity: f32,
    pub(crate) covered_food: f32,
}

pub(crate) fn nitrate_capacity(biome: f32) -> f32 {
    [0.85, 0.75, 1., 0.45][biome as usize]
}

pub(crate) fn tile_index(x: f32, y: f32) -> usize {
    ((y / 64.) as usize).min(GRID - 1) * GRID + ((x / 64.) as usize).min(GRID - 1)
}

pub(crate) fn map() -> Vec<Tile> {
    (0..GRID * GRID)
        .map(|i| {
            let x = (i % GRID) as f32 / GRID as f32;
            let y = (i / GRID) as f32 / GRID as f32;
            let (light, radiation, capacity, biome) =
                if (x - 0.77).powi(2) + (y - 0.27).powi(2) < 0.035 {
                    (0.35, 0.7, 0.45, 3.)
                } else if y > 0.64 + 0.09 * libm::sinf(x * 12.) {
                    (0.16, 0.02, 1., 2.)
                } else if x < 0.45 + 0.1 * libm::sinf(y * 9.) {
                    (0.95, 0.005, 0.35, 0.)
                } else {
                    (0.5, 0.035, 0.75, 1.)
                };
            let nitrate_capacity = nitrate_capacity(biome);
            Tile {
                light,
                radiation,
                food: capacity,
                capacity,
                biome,
                nitrates: nitrate_capacity,
                nitrate_capacity,
                covered_food: 0.,
            }
        })
        .collect()
}
