//! Uniform broad phase over swept bounds. Candidate pairs are visited once per body.

use crate::components::{Position, SIZE};

const BUCKET_SIZE: f32 = 32.;
const GRID_SIZE: usize = (SIZE / BUCKET_SIZE) as usize;
const MAX_BUCKETS_PER_BODY: usize = 16;

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
        let bounds = self.bounds[index];
        let (rows, direct) = if bounds.is_large() {
            (0..0, 0..self.bounds.len())
        } else {
            (bounds.first_y..bounds.last_y + 1, 0..self.large.len())
        };
        let buckets = rows.flat_map(move |y| {
            (bounds.first_x..=bounds.last_x).flat_map(move |x| {
                self.buckets[y * GRID_SIZE + x]
                    .iter()
                    .copied()
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
            .chain(direct.map(move |i| if bounds.is_large() { i } else { self.large[i] }))
            .filter(move |&other| other != index && bounds.overlaps(self.bounds[other]))
    }
}
