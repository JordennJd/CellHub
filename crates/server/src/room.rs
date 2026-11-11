//! One authoritative public room: a fixed-step simulation and its chunk stream.

use cellhub_protocol::{
    CHUNK_COUNT, CHUNK_SIZE, CHUNKS_PER_AXIS, Chunk, ClientMessage, RoomInfo, ServerMessage,
};
use cellhub_simulation::Simulation;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::{info, warn};

pub type RoomRegistry = Arc<RwLock<HashMap<String, Arc<RoomHandle>>>>;

const TICK_RATE: u64 = 30;
const EMPTY_ROOM_TTL: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub enum RoomEvent {
    Binary(axum::body::Bytes),
    Text(Arc<str>),
}

impl RoomEvent {
    pub fn json(message: &ServerMessage) -> Self {
        Self::Text(Arc::from(serde_json::to_string(message).unwrap_or_else(
            |_| r#"{"type":"error","message":"serialization failed"}"#.to_owned(),
        )))
    }
}

pub struct RoomCommand {
    pub player: u32,
    pub message: ClientMessage,
    pub reply: mpsc::UnboundedSender<RoomEvent>,
}

pub struct RoomHandle {
    pub id: String,
    pub name: String,
    pub host: u32,
    pub host_name: String,
    pub player_count: AtomicU32,
    pub tick: AtomicU32,
    pub population: AtomicU32,
    pub tx: mpsc::Sender<RoomCommand>,
    pub step_us: AtomicU64,
    pub stream_us: AtomicU64,
    pub tick_us: AtomicU64,
    pub overruns: AtomicU64,
    pub bytes: AtomicU64,
    pub stopping: AtomicBool,
    pub events: broadcast::Sender<RoomEvent>,
}

impl RoomHandle {
    pub fn info(&self) -> RoomInfo {
        RoomInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            population: self.population.load(Ordering::Relaxed),
            players: self.player_count.load(Ordering::Relaxed),
            tick: self.tick.load(Ordering::Relaxed),
            host: self.host_name.clone(),
        }
    }

    pub fn broadcast(&self, event: RoomEvent) {
        let _ = self.events.send(event);
    }
}

/// Start a room task and return its public handle. The caller stores the
/// handle in the registry; the task removes itself after its empty TTL.
pub fn spawn_room(
    registry: RoomRegistry,
    id: String,
    name: String,
    host: u32,
    host_name: String,
    seed: u32,
) -> Arc<RoomHandle> {
    let (tx, rx) = mpsc::channel(1024);
    let (events, _) = broadcast::channel(96);
    let room = Arc::new(RoomHandle {
        id: id.clone(),
        name,
        host,
        host_name,
        player_count: AtomicU32::new(0),
        tick: AtomicU32::new(0),
        population: AtomicU32::new(0),
        tx,
        events,
        step_us: AtomicU64::new(0),
        stream_us: AtomicU64::new(0),
        tick_us: AtomicU64::new(0),
        overruns: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
        stopping: AtomicBool::new(false),
    });
    let task_room = Arc::clone(&room);
    let task_registry = Arc::clone(&registry);
    tokio::spawn(async move {
        if let Err(error) = tokio::task::spawn_blocking(move || run_room(task_room, rx, seed)).await
        {
            warn!(%error, "room worker failed");
        }
        task_registry.write().await.remove(&id);
        info!(%id, "room closed");
    });
    room
}

fn run_room(room: Arc<RoomHandle>, mut commands: mpsc::Receiver<RoomCommand>, seed: u32) {
    let mut simulation = Simulation::new(seed);
    let mut cursor = 0usize;
    let mut empty_since = Some(Instant::now());
    let period = Duration::from_secs_f64(1. / TICK_RATE as f64);
    let mut deadline = Instant::now();
    loop {
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        if room.stopping.load(Ordering::Relaxed) {
            room.broadcast(RoomEvent::json(&ServerMessage::RoomClosed {
                reason: "Сервер останавливается.".to_owned(),
            }));
            break;
        }
        let tick_started = Instant::now();
        // A busy player cannot starve the fixed simulation tick.
        for _ in 0..256 {
            let Ok(command) = commands.try_recv() else {
                break;
            };
            apply_command(&room, &mut simulation, &mut cursor, command);
        }
        let players = room.player_count.load(Ordering::Relaxed);
        if players == 0 {
            let since = *empty_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= EMPTY_ROOM_TTL {
                room.broadcast(RoomEvent::json(&ServerMessage::RoomClosed {
                    reason: "Комната закрыта: нет игроков".to_owned(),
                }));
                break;
            }
        } else {
            empty_since = None;
        }

        let step_started = Instant::now();
        simulation.advance(1);
        room.step_us
            .store(step_started.elapsed().as_micros() as u64, Ordering::Relaxed);
        let stream_started = Instant::now();
        let stats = simulation.stats();
        let chunk_x = (cursor % CHUNKS_PER_AXIS as usize) as u32;
        let chunk_y = (cursor / CHUNKS_PER_AXIS as usize) as u32;
        let snapshot = simulation.chunk_snapshot(chunk_x, chunk_y, CHUNK_SIZE);
        let chunk = Chunk {
            tick: simulation.tick_number(),
            population: stats[1] as u32,
            births: stats[2] as u32,
            deaths: stats[3] as u32,
            infections: stats[4] as u32,
            chunk_x: chunk_x as u8,
            chunk_y: chunk_y as u8,
            cells: snapshot.cells,
            links: snapshot.links,
            terrain: snapshot.terrain,
        };
        match chunk.encode() {
            Ok(bytes) => {
                room.bytes
                    .fetch_add(bytes.len() as u64 * players as u64, Ordering::Relaxed);
                room.tick.store(chunk.tick, Ordering::Relaxed);
                room.population.store(chunk.population, Ordering::Relaxed);
                room.broadcast(RoomEvent::Binary(bytes.into()));
            }
            Err(error) => warn!(%error, "cannot encode room chunk"),
        }
        cursor = (cursor + 1) % CHUNK_COUNT;
        room.stream_us.store(
            stream_started.elapsed().as_micros() as u64,
            Ordering::Relaxed,
        );
        let elapsed = tick_started.elapsed();
        room.tick_us
            .store(elapsed.as_micros() as u64, Ordering::Relaxed);
        deadline += period;
        if elapsed > period {
            room.overruns.fetch_add(1, Ordering::Relaxed);
            deadline = Instant::now();
        }
    }
}

fn apply_command(
    room: &RoomHandle,
    simulation: &mut Simulation,
    cursor: &mut usize,
    command: RoomCommand,
) {
    let seq = match &command.message {
        ClientMessage::Spawn { seq, .. }
        | ClientMessage::Edit { seq, .. }
        | ClientMessage::Inspect { seq, .. }
        | ClientMessage::Reset { seq, .. } => *seq,
        _ => 0,
    };
    match command.message {
        ClientMessage::Spawn {
            x, y, mode, genome, ..
        } => {
            let result = simulation.spawn_checked(x, y, mode as usize, &genome);
            let reply = match result {
                Ok(cell_id) => ServerMessage::Ack {
                    seq,
                    cell_id: Some(cell_id),
                    error: None,
                },
                Err(error) => ServerMessage::Ack {
                    seq,
                    cell_id: None,
                    error: Some(error),
                },
            };
            let _ = command.reply.send(RoomEvent::json(&reply));
        }
        ClientMessage::Edit {
            id, mode, genome, ..
        } => {
            let reply = match simulation.edit_checked(id, mode as usize, &genome) {
                Ok(()) => ServerMessage::Ack {
                    seq,
                    cell_id: None,
                    error: None,
                },
                Err(error) => ServerMessage::Ack {
                    seq,
                    cell_id: None,
                    error: Some(error),
                },
            };
            let _ = command.reply.send(RoomEvent::json(&reply));
        }
        ClientMessage::Inspect { id, .. } => {
            let genome = simulation.genome(id);
            let reply = if genome.is_empty() {
                ServerMessage::Inspect {
                    seq,
                    id,
                    genome: None,
                    mode: None,
                }
            } else {
                ServerMessage::Inspect {
                    seq,
                    id,
                    genome: Some(genome),
                    mode: Some(simulation.cell_mode(id) as u32),
                }
            };
            let _ = command.reply.send(RoomEvent::json(&reply));
        }
        ClientMessage::Reset { .. } => {
            if command.player != room.host {
                let _ = command.reply.send(RoomEvent::json(&ServerMessage::Error {
                    message: "Очистить мир может только создатель комнаты.".to_owned(),
                    seq: Some(seq),
                }));
                return;
            }
            simulation.reset();
            *cursor = 0;
            room.population.store(0, Ordering::Relaxed);
            room.tick.store(0, Ordering::Relaxed);
            room.broadcast(RoomEvent::json(&ServerMessage::WorldReset { tick: 0 }));
            let _ = command.reply.send(RoomEvent::json(&ServerMessage::Ack {
                seq,
                cell_id: None,
                error: None,
            }));
        }
        _ => {}
    }
}
