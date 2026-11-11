//! Uniform broad phase over swept bounds. Candidate pairs are visited once per body.

use crate::components::Position;

const BUCKET_SIZE: f32 = 32.;
const GRID_SIZE: usize = (crate::terrain::CHAOS_SIZE / BUCKET_SIZE) as usize;
const MAX_BUCKETS_PER_BODY: usize = 16;
const CROWDED_BUCKET: usize = 96;
const CROWD_SAMPLE: usize = 24;
const PACKED_BUCKET: usize = 512;
const PACKED_SAMPLE: usize = 12;

#[derive(Clone, Copy)]
struct Bounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    first_x: usize,
    first_y: usize,
    last_x: usize,
    last_y: usize,
}

impl Bounds {
    fn is_large(self) -> bool {
        (self.last_x - self.first_x + 1) * (self.last_y - self.first_y + 1) > MAX_BUCKETS_PER_BODY
    }

    fn overlaps(self, other: Self) -> bool {
        self.min_x <= other.max_x
            && other.min_x <= self.max_x
            && self.min_y <= other.max_y
            && other.min_y <= self.max_y
    }
}

fn bucket(value: f32) -> usize {
    ((value / BUCKET_SIZE) as usize).min(GRID_SIZE - 1)
}

#[derive(Default)]
pub(super) struct SpatialGrid {
    buckets: Vec<Vec<usize>>,
    occupied: Vec<usize>,
    large: Vec<usize>,
    bounds: Vec<Bounds>,
}

impl SpatialGrid {
    pub(super) fn new() -> Self {
        Self {
            buckets: (0..GRID_SIZE * GRID_SIZE).map(|_| Vec::new()).collect(),
            ..Self::default()
        }
    }

    pub(super) fn clear(&mut self) {
        for index in self.occupied.drain(..) {
            self.buckets[index].clear();
        }
        self.bounds.clear();
        self.large.clear();
    }

    pub(super) fn insert(&mut self, path: &[Position; 3], reach: f32) {
        let min_x = path[0].x.min(path[1].x).min(path[2].x) - reach;
        let min_y = path[0].y.min(path[1].y).min(path[2].y) - reach;
        let max_x = path[0].x.max(path[1].x).max(path[2].x) + reach;
        let max_y = path[0].y.max(path[1].y).max(path[2].y) + reach;
        let bounds = Bounds {
            min_x,
            min_y,
            max_x,
            max_y,
            first_x: bucket(min_x),
            first_y: bucket(min_y),
            last_x: bucket(max_x),
            last_y: bucket(max_y),
        };
        let index = self.bounds.len();
        self.bounds.push(bounds);
        // Unusually large cells and long bond corrections are stored once.
        // Duplicating them into every covered bucket has an unbounded cost.
        if bounds.is_large() {
            self.large.push(index);
            return;
        }
        for y in bounds.first_y..=bounds.last_y {
            for x in bounds.first_x..=bounds.last_x {
                let key = y * GRID_SIZE + x;
                if self.buckets[key].is_empty() {
                    self.occupied.push(key);
                }
                self.buckets[key].push(index);
            }
        }
    }

    pub(super) fn candidates(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        self.query(index, None)
    }

    pub(super) fn collision_candidates(
        &self,
        index: usize,
        tick: u32,
    ) -> impl Iterator<Item = usize> + '_ {
        self.query(index, Some(tick))
    }

    /// Overloaded buckets may omit physical pairs. Joining all their occupants
    /// still keeps every potential contact in the same island, including biology.
    pub(super) fn connect_crowds(&self, mut join: impl FnMut(usize, usize)) -> Vec<usize> {
        if self.large.len() > CROWDED_BUCKET {
            for index in 1..self.bounds.len() {
                join(0, index);
            }
            return vec![0];
        }
        let mut representatives = Vec::new();
        for &key in &self.occupied {
            let bodies = &self.buckets[key];
            if bodies.len() > CROWDED_BUCKET {
                representatives.push(bodies[0]);
                for &other in &bodies[1..] {
                    join(bodies[0], other);
                }
            }
        }
        representatives
    }

    fn query(&self, index: usize, tick: Option<u32>) -> impl Iterator<Item = usize> + '_ {
        let bounds = self.bounds[index];
        let is_large = bounds.is_large();
        let (rows, direct_end) = if is_large {
            (0..0, self.bounds.len())
        } else {
            (bounds.first_y..bounds.last_y + 1, self.large.len())
        };
        let direct_start = match tick {
            None => 0,
            Some(_) if is_large => index + 1,
            Some(_) => self.large.partition_point(|&other| other <= index),
        };
        let direct_sample = sample_for(tick, self.large.len());
        let buckets = rows.flat_map(move |y| {
            (bounds.first_x..=bounds.last_x).flat_map(move |x| {
                let bucket = &self.buckets[y * GRID_SIZE + x];
                let start = tick.map_or(0, |_| bucket.partition_point(|&other| other <= index));
                let sample = sample_for(tick, bucket.len());
                rotating_window(start, bucket.len(), sample)
                    .map(move |slot| bucket[slot])
                    .filter(move |&other| {
                        let other_bounds = self.bounds[other];
                        // Visiting only the first shared bucket avoids materializing pairs
                        // or keeping a deduplication set for densely occupied buckets.
                        x == bounds.first_x.max(other_bounds.first_x)
                            && y == bounds.first_y.max(other_bounds.first_y)
                    })
            })
        });
        buckets
            .chain(
                rotating_window(direct_start, direct_end, direct_sample)
                    .map(move |i| if is_large { i } else { self.large[i] }),
            )
            .filter(move |&other| other != index && bounds.overlaps(self.bounds[other]))
    }
}

fn sample_for(tick: Option<u32>, occupants: usize) -> Option<(u32, usize)> {
    tick.filter(|_| occupants > CROWDED_BUCKET).map(|tick| {
        let count = if occupants > PACKED_BUCKET {
            PACKED_SAMPLE
        } else {
            CROWD_SAMPLE
        };
        (tick, count)
    })
}

fn rotating_window(
    start: usize,
    end: usize,
    sample: Option<(u32, usize)>,
) -> impl Iterator<Item = usize> {
    let available = end - start;
    let (count, offset) = match sample {
        Some((tick, count)) if available > count => (
            count,
            ((u64::from(tick) * count as u64) % available as u64) as usize,
        ),
        _ => (available, 0),
    };
    (0..count).map(move |i| {
        let slot = start + offset + i;
        if slot < end { slot } else { slot - available }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crowded_collision_samples_rotate_without_losing_biological_candidates() {
        let mut grid = SpatialGrid::new();
        let position = Position {
            x: 500.,
            y: 500.,
            angle: 0.,
        };
        let count = CROWDED_BUCKET + 12;
        for _ in 0..count {
            grid.insert(&[position; 3], 18.);
        }

        assert_eq!(
            grid.candidates(0).collect::<Vec<_>>(),
            (1..count).collect::<Vec<_>>()
        );
        let first = grid.collision_candidates(0, 0).collect::<Vec<_>>();
        let second = grid.collision_candidates(0, 1).collect::<Vec<_>>();
        assert_eq!(first, (1..=CROWD_SAMPLE).collect::<Vec<_>>());
        assert_eq!(
            second,
            (CROWD_SAMPLE + 1..=2 * CROWD_SAMPLE).collect::<Vec<_>>()
        );

        let mut connected = vec![false; count];
        let crowds = grid.connect_crowds(|a, b| {
            assert_eq!(a, 0);
            connected[b] = true;
        });
        assert!(!crowds.is_empty());
        assert!(connected[1..].iter().all(|&value| value));
    }
}
