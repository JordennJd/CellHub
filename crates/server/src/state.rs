//! Shared lobby state: public room registry and identity counters.

use crate::room::{RoomHandle, RoomRegistry, spawn_room};
use cellhub_protocol::{ChaosPreset, RoomInfo, RoomMode};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct AppState {
    pub rooms: RoomRegistry,
    pub next_room: AtomicU32,
    pub next_player: AtomicU32,
    pub max_rooms: usize,
    pub max_players: usize,
}

impl AppState {
    pub fn new(max_rooms: usize, max_players: usize) -> Arc<Self> {
        Arc::new(Self {
            rooms: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            next_room: AtomicU32::new(1),
            next_player: AtomicU32::new(1),
            max_rooms,
            max_players,
        })
    }

    pub fn next_player_id(&self) -> u32 {
        self.next_player.fetch_add(1, Ordering::Relaxed)
    }

    pub async fn list_rooms(&self) -> Vec<RoomInfo> {
        let rooms = self.rooms.read().await;
        let mut list: Vec<_> = rooms.values().map(|room| room.info()).collect();
        list.sort_by(|a, b| b.players.cmp(&a.players).then_with(|| a.id.cmp(&b.id)));
        list
    }

    pub async fn create_room(
        self: &Arc<Self>,
        raw_name: Option<&str>,
        mode: RoomMode,
        chaos: Option<ChaosPreset>,
        host_player: u32,
        host_name: &str,
    ) -> Result<Arc<RoomHandle>, String> {
        let mut rooms = self.rooms.write().await;
        if rooms.len() >= self.max_rooms {
            return Err("Сервер занят: достигнут лимит комнат.".to_owned());
        }
        let id = loop {
            let candidate = room_code(self.next_room.fetch_add(1, Ordering::Relaxed));
            if !rooms.contains_key(&candidate) {
                break candidate;
            }
        };
        let name = match raw_name.map(str::trim).filter(|name| !name.is_empty()) {
            Some(name) => sanitize_name(name, 48),
            None => format!("{} {id}", mode.label()),
        };
        let seed = room_seed(&id);
        let room = spawn_room(
            Arc::clone(&self.rooms),
            id.clone(),
            name,
            mode,
            chaos,
            host_player,
            host_name.to_owned(),
            seed,
        );
        rooms.insert(id, Arc::clone(&room));
        Ok(room)
    }
}

pub fn sanitize_name(name: &str, max_chars: usize) -> String {
    let filtered: String = name
        .chars()
        .filter(|ch| !ch.is_control())
        .take(max_chars)
        .collect();
    let trimmed = filtered.trim();
    if trimmed.is_empty() {
        "Игрок".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn room_code(value: u32) -> String {
    const ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
    let mut value = value;
    let mut code = String::with_capacity(5);
    for _ in 0..5 {
        code.push(ALPHABET[(value as usize) % ALPHABET.len()] as char);
        value /= ALPHABET.len() as u32;
    }
    code
}

fn room_seed(id: &str) -> u32 {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos() as u32)
        .unwrap_or(1);
    id.bytes()
        .fold(time ^ 0x9e37_79b9, |seed, byte| {
            seed.rotate_left(5) ^ byte as u32
        })
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_limited_and_never_empty() {
        assert_eq!(sanitize_name("  colony  ", 48), "colony");
        assert_eq!(sanitize_name("\n\t", 48), "Игрок");
        assert_eq!(sanitize_name("абвгдеёж", 3), "абв");
    }

    #[test]
    fn room_codes_are_stable_and_url_safe() {
        assert_eq!(room_code(0), "22222");
        assert_eq!(room_code(1), "32222");
        assert_eq!(room_code(1), room_code(1));
        assert!(
            room_code(123456)
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric())
        );
    }
}
