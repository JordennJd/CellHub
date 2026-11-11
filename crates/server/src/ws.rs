//! WebSocket lobby and per-player connection handling.

use crate::room::{RoomCommand, RoomEvent, RoomHandle};
use crate::state::{AppState, sanitize_name};
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use cellhub_protocol::{ClientMessage, PROTOCOL_VERSION, PlayerInfo, ServerMessage};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tracing::{debug, info};

const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_COMMANDS_PER_SECOND: u32 = 240;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| connection(socket, state))
}

struct PlayerSlot {
    room: Arc<RoomHandle>,
}

impl Drop for PlayerSlot {
    fn drop(&mut self) {
        let previous = self.room.player_count.fetch_sub(1, Ordering::AcqRel);
        if previous > 0 {
            self.room
                .broadcast(RoomEvent::json(&ServerMessage::PlayerCount {
                    players: previous - 1,
                }));
        }
    }
}

enum Selected {
    Direct(RoomEvent),
    Room(Result<RoomEvent, RecvError>),
    Incoming(Option<Result<Message, axum::Error>>),
}

async fn connection(mut socket: WebSocket, state: Arc<AppState>) {
    let player_id = state.next_player_id();
    let mut player = PlayerInfo {
        id: player_id,
        name: format!("Игрок {player_id}"),
    };
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<RoomEvent>();
    let (dummy_tx, _) = tokio::sync::broadcast::channel::<RoomEvent>(1);
    let mut events = dummy_tx.subscribe();
    let mut joined: Option<PlayerSlot> = None;
    let mut limiter = RateLimiter::new();
    debug!(player_id, "websocket connected");

    loop {
        let selected = tokio::select! {
            biased;
            event = out_rx.recv() => match event {
                Some(event) => Selected::Direct(event),
                None => break,
            },
            event = events.recv() => Selected::Room(event),
            incoming = socket.recv() => Selected::Incoming(incoming),
        };
        match selected {
            Selected::Direct(event) | Selected::Room(Ok(event)) => {
                if let Err(error) = send_event(&mut socket, event).await {
                    debug!(%error, "websocket send failed");
                    break;
                }
            }
            Selected::Room(Err(RecvError::Lagged(skipped))) => {
                debug!(skipped, "room stream lagged");
            }
            Selected::Room(Err(RecvError::Closed)) => {
                joined = None;
                events = dummy_tx.subscribe();
                if let Err(error) = send_event(
                    &mut socket,
                    RoomEvent::json(&ServerMessage::RoomClosed {
                        reason: "Комната остановлена сервером.".to_owned(),
                    }),
                )
                .await
                {
                    debug!(%error, "websocket send failed");
                    break;
                }
            }
            Selected::Incoming(Some(Ok(Message::Close(_)))) => break,
            Selected::Incoming(Some(Ok(Message::Ping(payload)))) => {
                if socket.send(Message::Pong(payload)).await.is_err() {
                    break;
                }
            }
            Selected::Incoming(Some(Ok(Message::Pong(_)))) => {}
            Selected::Incoming(Some(Ok(Message::Binary(_)))) => {
                let _ = out_tx.send(RoomEvent::json(&ServerMessage::Error {
                    message: "Бинарные клиентские команды не поддерживаются.".to_owned(),
                    seq: None,
                }));
            }
            Selected::Incoming(Some(Ok(Message::Text(text)))) => {
                if !limiter.allow() {
                    let _ = out_tx.send(RoomEvent::json(&ServerMessage::Error {
                        message: "Слишком много команд. Подождите секунду.".to_owned(),
                        seq: None,
                    }));
                    continue;
                }
                let message: ClientMessage = match serde_json::from_str(text.as_str()) {
                    Ok(message) => message,
                    Err(error) => {
                        let _ = out_tx.send(RoomEvent::json(&ServerMessage::Error {
                            message: format!("Некорректная команда: {error}"),
                            seq: None,
                        }));
                        continue;
                    }
                };
                match message {
                    ClientMessage::Ping { t } => {
                        let _ = out_tx.send(RoomEvent::json(&ServerMessage::Pong { t }));
                    }
                    ClientMessage::List => {
                        let rooms = state.list_rooms().await;
                        let _ = out_tx.send(RoomEvent::json(&ServerMessage::Rooms { rooms }));
                    }
                    ClientMessage::Create {
                        name,
                        player: player_name,
                    } => {
                        if joined.is_some() {
                            send_busy(&out_tx);
                            continue;
                        }
                        if let Some(name) = player_name {
                            player.name = sanitize_name(&name, 32);
                        }
                        match state
                            .create_room(name.as_deref(), player.id, &player.name)
                            .await
                        {
                            Ok(room) => {
                                match join_room(&state, Arc::clone(&room), &player, &out_tx) {
                                    Ok((slot, receiver)) => {
                                        joined = Some(slot);
                                        events = receiver;
                                        info!(player_id, room = %room.id, "room created");
                                    }
                                    Err(error) => {
                                        let _ =
                                            out_tx.send(RoomEvent::json(&ServerMessage::Error {
                                                message: error,
                                                seq: None,
                                            }));
                                    }
                                }
                            }
                            Err(error) => {
                                let _ = out_tx.send(RoomEvent::json(&ServerMessage::Error {
                                    message: error,
                                    seq: None,
                                }));
                            }
                        }
                    }
                    ClientMessage::Join {
                        room,
                        player: player_name,
                    } => {
                        if joined.is_some() {
                            send_busy(&out_tx);
                            continue;
                        }
                        if let Some(name) = player_name {
                            player.name = sanitize_name(&name, 32);
                        }
                        let target = state.rooms.read().await.get(&room).cloned();
                        match target {
                            Some(room) => {
                                match join_room(&state, Arc::clone(&room), &player, &out_tx) {
                                    Ok((slot, receiver)) => {
                                        joined = Some(slot);
                                        events = receiver;
                                        info!(player_id, room = %room.id, "room joined");
                                    }
                                    Err(error) => {
                                        let _ =
                                            out_tx.send(RoomEvent::json(&ServerMessage::Error {
                                                message: error,
                                                seq: None,
                                            }));
                                    }
                                }
                            }
                            None => {
                                let _ = out_tx.send(RoomEvent::json(&ServerMessage::Error {
                                    message: "Комната не найдена или уже закрыта.".to_owned(),
                                    seq: None,
                                }));
                            }
                        }
                    }
                    ClientMessage::Leave => {
                        if joined.take().is_some() {
                            events = dummy_tx.subscribe();
                            let _ = out_tx.send(RoomEvent::json(&ServerMessage::Notice {
                                message: "Вы покинули комнату.".to_owned(),
                            }));
                        }
                    }
                    ClientMessage::Spawn { .. }
                    | ClientMessage::Edit { .. }
                    | ClientMessage::Inspect { .. }
                    | ClientMessage::Reset { .. } => {
                        if let Some(slot) = &joined {
                            if let Err(error) = slot.room.tx.try_send(RoomCommand {
                                player: player.id,
                                message,
                                reply: out_tx.clone(),
                            }) {
                                let seq = match error.into_inner().message {
                                    ClientMessage::Spawn { seq, .. }
                                    | ClientMessage::Edit { seq, .. }
                                    | ClientMessage::Inspect { seq, .. }
                                    | ClientMessage::Reset { seq } => seq,
                                    _ => unreachable!(),
                                };
                                let _ = out_tx.send(RoomEvent::json(&ServerMessage::Ack {
                                    seq,
                                    cell_id: None,
                                    error: Some(
                                        "Комната занята или уже остановлена. Повторите команду."
                                            .to_owned(),
                                    ),
                                }));
                            }
                        } else {
                            let _ = out_tx.send(RoomEvent::json(&ServerMessage::Error {
                                message: "Сначала создайте или выберите комнату.".to_owned(),
                                seq: None,
                            }));
                        }
                    }
                }
            }
            Selected::Incoming(Some(Err(error))) => {
                debug!(%error, "websocket receive failed");
                break;
            }
            Selected::Incoming(None) => break,
        }
    }
    drop(joined);
    debug!(player_id, "websocket disconnected");
}

fn send_busy(out_tx: &mpsc::UnboundedSender<RoomEvent>) {
    let _ = out_tx.send(RoomEvent::json(&ServerMessage::Error {
        message: "Вы уже находитесь в комнате. Сначала выйдите.".to_owned(),
        seq: None,
    }));
}

fn join_room(
    state: &Arc<AppState>,
    room: Arc<RoomHandle>,
    player: &PlayerInfo,
    out_tx: &mpsc::UnboundedSender<RoomEvent>,
) -> Result<(PlayerSlot, tokio::sync::broadcast::Receiver<RoomEvent>), String> {
    let mut current = room.player_count.load(Ordering::Relaxed);
    loop {
        if current as usize >= state.max_players {
            return Err(format!(
                "В комнате нет мест (максимум {}).",
                state.max_players
            ));
        }
        match room.player_count.compare_exchange_weak(
            current,
            current + 1,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
    let receiver = room.events.subscribe();
    let players = current + 1;
    let host = player.id == room.host;
    let _ = out_tx.send(RoomEvent::json(&ServerMessage::Welcome {
        protocol: PROTOCOL_VERSION,
        player: player.clone(),
        room: room.info(),
        host,
    }));
    room.broadcast(RoomEvent::json(&ServerMessage::PlayerCount { players }));
    Ok((PlayerSlot { room }, receiver))
}

struct RateLimiter {
    window: Instant,
    count: u32,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            window: Instant::now(),
            count: 0,
        }
    }

    fn allow(&mut self) -> bool {
        if self.window.elapsed() >= Duration::from_secs(1) {
            self.window = Instant::now();
            self.count = 0;
        }
        self.count += 1;
        self.count <= MAX_COMMANDS_PER_SECOND
    }
}

async fn send_event(socket: &mut WebSocket, event: RoomEvent) -> Result<(), axum::Error> {
    match event {
        RoomEvent::Binary(bytes) => socket.send(Message::Binary(bytes)).await,
        RoomEvent::Text(text) => socket.send(Message::Text(text.to_string().into())).await,
    }
}
