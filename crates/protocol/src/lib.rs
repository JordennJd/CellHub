//! Wire contract shared by the authoritative server and the browser client.
//!
//! Control messages travel as JSON text frames. The hot scene path uses a
//! compact little-endian binary frame: one map chunk (cells, its bonds and its
//! terrain tiles) per message. See `docs/multiplayer.md` for the layout.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const WORLD_SIZE: f32 = 3072.;
pub const CHUNK_SIZE: f32 = 512.;
pub const CHUNKS_PER_AXIS: u32 = 6;
pub const CHUNK_COUNT: usize = (CHUNKS_PER_AXIS * CHUNKS_PER_AXIS) as usize;
pub const TERRAIN_GRID: usize = 48;
pub const TERRAIN_TILES_PER_CHUNK: usize = TERRAIN_GRID / CHUNKS_PER_AXIS as usize;
pub const CELL_STRIDE: usize = 12;
pub const BIN_CHUNK: u8 = 1;

pub fn chunk_of(x: f32, y: f32) -> (u8, u8) {
    let cx = (x / CHUNK_SIZE).clamp(0., (CHUNKS_PER_AXIS - 1) as f32) as u8;
    let cy = (y / CHUNK_SIZE).clamp(0., (CHUNKS_PER_AXIS - 1) as f32) as u8;
    (cx, cy)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    pub population: u32,
    pub players: u32,
    pub tick: u32,
    pub host: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub id: u32,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    List,
    Create {
        name: Option<String>,
        player: Option<String>,
    },
    Join {
        room: String,
        player: Option<String>,
    },
    Leave,
    Spawn {
        seq: u64,
        x: f32,
        y: f32,
        mode: u32,
        genome: Vec<f32>,
    },
    Edit {
        seq: u64,
        id: u32,
        mode: u32,
        genome: Vec<f32>,
    },
    Inspect {
        seq: u64,
        id: u32,
    },
    Reset {
        seq: u64,
    },
    Ping {
        t: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Rooms {
        rooms: Vec<RoomInfo>,
    },
    Welcome {
        protocol: u32,
        player: PlayerInfo,
        room: RoomInfo,
        host: bool,
    },
    Ack {
        seq: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        cell_id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Inspect {
        seq: u64,
        id: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        genome: Option<Vec<f32>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<u32>,
    },
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    Notice {
        message: String,
    },
    PlayerCount {
        players: u32,
    },
    WorldReset {
        tick: u32,
    },
    Pong {
        t: u64,
    },
    RoomClosed {
        reason: String,
    },
}

/// One authoritative chunk, ready for binary encoding.
#[derive(Clone, Debug, Default)]
pub struct Chunk {
    pub tick: u32,
    pub population: u32,
    pub births: u32,
    pub deaths: u32,
    pub infections: u32,
    pub chunk_x: u8,
    pub chunk_y: u8,
    pub cells: Vec<f32>,
    pub links: Vec<u32>,
    pub terrain: Vec<f32>,
}

impl Chunk {
    pub fn encode(&self) -> Result<Vec<u8>, &'static str> {
        if !self.cells.len().is_multiple_of(CELL_STRIDE)
            || !self.links.len().is_multiple_of(2)
            || !self.terrain.len().is_multiple_of(4)
            || self.cells.len() / CELL_STRIDE > u16::MAX as usize
            || self.links.len() / 2 > u16::MAX as usize
            || self.terrain.len() / 4 > u16::MAX as usize
        {
            return Err("Invalid chunk payload");
        }
        let mut out = Vec::with_capacity(
            1 + 5 * 4
                + 2
                + 8
                + self.cells.len() * 4
                + self.links.len() * 4
                + self.terrain.len() * 4,
        );
        out.push(BIN_CHUNK);
        out.extend(self.tick.to_le_bytes());
        out.extend(self.population.to_le_bytes());
        out.extend(self.births.to_le_bytes());
        out.extend(self.deaths.to_le_bytes());
        out.extend(self.infections.to_le_bytes());
        out.extend([self.chunk_x, self.chunk_y]);
        out.extend(((self.cells.len() / CELL_STRIDE) as u16).to_le_bytes());
        out.extend(((self.links.len() / 2) as u16).to_le_bytes());
        out.extend(((self.terrain.len() / 4) as u16).to_le_bytes());
        out.extend([0u8; 2]);
        for value in &self.cells {
            out.extend(value.to_le_bytes());
        }
        for value in &self.links {
            out.extend(value.to_le_bytes());
        }
        for value in &self.terrain {
            out.extend(value.to_le_bytes());
        }
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 31 || bytes[0] != BIN_CHUNK {
            return None;
        }
        let u32_at = |offset: usize| {
            bytes
                .get(offset..offset + 4)
                .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
        };
        let tick = u32_at(1)?;
        let population = u32_at(5)?;
        let births = u32_at(9)?;
        let deaths = u32_at(13)?;
        let infections = u32_at(17)?;
        let chunk_x = *bytes.get(21)?;
        let chunk_y = *bytes.get(22)?;
        let cell_count = u16::from_le_bytes(bytes.get(23..25)?.try_into().ok()?) as usize;
        let link_count = u16::from_le_bytes(bytes.get(25..27)?.try_into().ok()?) as usize;
        let tile_count = u16::from_le_bytes(bytes.get(27..29)?.try_into().ok()?) as usize;
        let expected = 1
            + 5 * 4
            + 2
            + 8
            + cell_count * CELL_STRIDE * 4
            + link_count * 2 * 4
            + tile_count * 4 * 4;
        if bytes.len() != expected {
            return None;
        }
        let floats = |start: usize, count: usize| -> Vec<f32> {
            bytes[start..start + count * 4]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| f32::from_le_bytes(*v))
                .collect()
        };
        let cells_start = 31;
        let links_start = cells_start + cell_count * CELL_STRIDE * 4;
        let terrain_start = links_start + link_count * 2 * 4;
        let cells = floats(cells_start, cell_count * CELL_STRIDE);
        let links = bytes[links_start..links_start + link_count * 2 * 4]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| u32::from_le_bytes(*v))
            .collect();
        let terrain = floats(terrain_start, tile_count * 4);
        Some(Self {
            tick,
            population,
            births,
            deaths,
            infections,
            chunk_x,
            chunk_y,
            cells,
            links,
            terrain,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_chunk_round_trips() {
        let chunk = Chunk {
            tick: 42,
            population: 7,
            births: 9,
            deaths: 2,
            infections: 1,
            chunk_x: 2,
            chunk_y: 5,
            cells: (0..CELL_STRIDE as u32).map(|v| v as f32 * 0.5).collect(),
            links: vec![3, 4],
            terrain: (0..4 * TERRAIN_TILES_PER_CHUNK * TERRAIN_TILES_PER_CHUNK)
                .map(|v| v as f32)
                .collect(),
        };
        let bytes = chunk.encode().unwrap();
        let decoded = Chunk::decode(&bytes).unwrap();
        assert_eq!(decoded.tick, 42);
        assert_eq!(decoded.chunk_x, 2);
        assert_eq!(decoded.cells, chunk.cells);
        assert_eq!(decoded.links, chunk.links);
        assert_eq!(decoded.terrain, chunk.terrain);
    }

    #[test]
    fn binary_chunk_rejects_truncated_payloads() {
        let chunk = Chunk {
            cells: vec![1.; CELL_STRIDE],
            ..Chunk::default()
        };
        let bytes = chunk.encode().unwrap();
        assert!(Chunk::decode(&bytes[..bytes.len() - 1]).is_none());
        assert!(Chunk::decode(&bytes[..8]).is_none());
    }

    #[test]
    fn chunk_lookup_stays_inside_the_world() {
        assert_eq!(chunk_of(-10., 10.), (0, 0));
        assert_eq!(chunk_of(WORLD_SIZE, WORLD_SIZE), (5, 5));
        assert_eq!(chunk_of(513., 100.), (1, 0));
    }
}
