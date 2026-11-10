//! Reusable uniform spatial hash for local interaction queries.
//!
//! Buckets live in one flat vector and keep their capacity between ticks, so a
//! dense world does not allocate or build a tree for every frame.

use crate::components::{Position, SIZE};
use hecs::Entity;

pub(super) const BUCKET_SIZE: f32 = 32.;
pub(super) const GRID_SIZE: usize = (SIZE / BUCKET_SIZE) as usize;

#[derive(Clone, Copy)]
pub(super) struct Neighbor {
    pub(super) entity: Entity,
    pub(super) position: Position,
    pub(super) kind: u8,
}

#[derive(Default)]
pub(super) struct SpatialGrid {
    buckets: Vec<Vec<Neighbor>>,
}

impl SpatialGrid {
    pub(super) fn new() -> Self {
        Self {
            buckets: (0..GRID_SIZE * GRID_SIZE).map(|_| Vec::new()).collect(),
        }
    }

    pub(super) fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
    }

    pub(super) fn insert(&mut self, position: Position, entity: Entity, kind: u8) {
        let bx = (position.x / BUCKET_SIZE) as usize;
        let by = (position.y / BUCKET_SIZE) as usize;
        let index = by.min(GRID_SIZE - 1) * GRID_SIZE + bx.min(GRID_SIZE - 1);
        self.buckets[index].push(Neighbor {
            entity,
            position,
            kind,
        });
    }

    pub(super) fn bucket(&self, bx: isize, by: isize) -> Option<&[Neighbor]> {
        if bx < 0 || by < 0 || bx >= GRID_SIZE as isize || by >= GRID_SIZE as isize {
            return None;
        }
        Some(&self.buckets[by as usize * GRID_SIZE + bx as usize])
    }
}
