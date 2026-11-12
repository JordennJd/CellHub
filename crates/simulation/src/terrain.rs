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

#[cfg(test)]
pub(crate) fn tile_index(x: f32, y: f32) -> usize {
    index(x, y, GRID)
}

#[cfg(test)]
pub(crate) fn index(x: f32, y: f32, grid: usize) -> usize {
    index_rect(x, y, grid, grid)
}

pub(crate) fn index_rect(x: f32, y: f32, columns: usize, rows: usize) -> usize {
    ((y / 64.) as usize).min(rows - 1) * columns + ((x / 64.) as usize).min(columns - 1)
}

pub(crate) const CHAOS_GRID: usize = 48;
pub(crate) const CHAOS_SIZE: f32 = CHAOS_GRID as f32 * 64.;
/// Older worlds used a 96 × 96 chaos dish. Their dimensions are still read
/// from v9+ checkpoints so existing experiments keep running unchanged.
pub(crate) const CHAOS_SIZE_LEGACY: f32 = 6144.;

/// light, radiation, food capacity, nitrate capacity, food regeneration multiplier.
pub(crate) const BIOMES: [[f32; 5]; 10] = [
    [1.20, 0.005, 0.35, 0.70, 1.0], // Solar shallows
    [0.45, 0.015, 1.15, 0.90, 1.4], // Nutrient lagoon
    [0.75, 0.010, 0.65, 1.50, 1.1], // Nitrogen springs
    [0.85, 0.005, 0.20, 0.35, 0.6], // Sparse flats
    [0.65, 0.280, 0.70, 0.80, 1.0], // Mutation marsh
    [0.55, 0.020, 1.30, 0.55, 1.7], // Organic delta
    [0.35, 0.005, 0.45, 1.20, 0.8], // Twilight refuge
    [1.10, 0.035, 0.75, 1.00, 1.1], // Algal gardens
    [0.70, 0.090, 0.90, 1.35, 1.3], // Mineral ridge
    [0.90, 0.010, 0.55, 0.95, 0.9], // Quiet oasis
];

pub(crate) fn random(rng: &mut u32) -> f32 {
    *rng ^= *rng << 13;
    *rng ^= *rng >> 17;
    *rng ^= *rng << 5;
    (*rng >> 8) as f32 / 16_777_216.
}

/// Seeded scattered regions, blended at their boundaries and modulated by
/// continuous resource patches. Each biome has an anchor inside the dish.
pub(crate) fn chaos_map(seed: u32) -> Vec<Tile> {
    chaos_map_rect(seed, CHAOS_GRID, CHAOS_GRID)
}

pub(crate) fn chaos_map_rect(seed: u32, columns: usize, rows: usize) -> Vec<Tile> {
    let mut rng = (seed ^ 0x9e3779b9).max(1);
    let phase = random(&mut rng) * std::f32::consts::TAU;
    let anchors: Vec<_> = (0..30)
        .map(|i| {
            let angle = phase + i as f32 * 2.399963;
            let r = if i < 10 {
                0.32
            } else {
                0.12 + random(&mut rng) * 0.34
            };
            (
                0.5 + libm::cosf(angle) * r,
                0.5 + libm::sinf(angle) * r,
                i % 10,
            )
        })
        .collect();
    (0..columns * rows)
        .map(|i| {
            let x = (i % columns) as f32 / columns as f32;
            let y = (i / columns) as f32 / rows as f32;
            let wx = x + 0.025 * libm::sinf(y * 23. + phase);
            let wy = y + 0.025 * libm::cosf(x * 19. - phase);
            let mut nearest = [(f32::MAX, 0); 2];
            for &(ax, ay, biome) in &anchors {
                let d = (wx - ax).powi(2) + (wy - ay).powi(2);
                if d < nearest[0].0 {
                    nearest[1] = nearest[0];
                    nearest[0] = (d, biome);
                } else if d < nearest[1].0 {
                    nearest[1] = (d, biome);
                }
            }
            let blend = ((nearest[1].0 - nearest[0].0) / 0.004).clamp(0., 1.) * 0.5 + 0.5;
            let a = BIOMES[nearest[0].1];
            let b = BIOMES[nearest[1].1];
            let property = |n: usize| b[n] + (a[n] - b[n]) * blend;
            let patch = 0.85 + 0.15 * libm::sinf(x * 57. + phase) * libm::cosf(y * 49. - phase);
            let capacity = property(2) * patch;
            let nitrate_capacity = property(3) * (1.7 - patch);
            Tile {
                light: property(0),
                radiation: property(1),
                food: capacity,
                capacity,
                biome: nearest[0].1 as f32,
                nitrates: nitrate_capacity,
                nitrate_capacity,
                covered_food: 0.,
            }
        })
        .collect()
}

pub(crate) fn map() -> Vec<Tile> {
    map_rect(GRID, GRID)
}

pub(crate) fn map_rect(columns: usize, rows: usize) -> Vec<Tile> {
    (0..columns * rows)
        .map(|i| {
            let x = (i % columns) as f32 / columns as f32;
            let y = (i / columns) as f32 / rows as f32;
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
