//! Optional authoritative multiplayer server for CellHub.
//!
//! It serves the built frontend (when `dist/` exists), a health endpoint and a
//! WebSocket lobby at `/ws`. Browsers create or join public rooms; each room
//! owns one fixed-step simulation and streams one map chunk per tick.

mod room;
mod state;
mod ws;

use axum::Router;
use axum::http::{HeaderName, HeaderValue};
use axum::routing::get;
use state::AppState;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("cellhub_server=info,tower_http=warn")),
        )
        .init();
    let config = Config::from_args();
    let state = AppState::new(config.max_rooms, config.max_players);
    let index = config.static_dir.join("index.html");
    let app = Router::new()
        .route("/ws", get(ws::ws_handler))
        .route("/health", get(health))
        .route("/debug/rooms", get(room_metrics))
        .fallback_service(
            ServeDir::new(&config.static_dir).not_found_service(ServeFile::new(index)),
        )
        .with_state(Arc::clone(&state))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("cross-origin-opener-policy"),
            HeaderValue::from_static("same-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("cross-origin-embedder-policy"),
            HeaderValue::from_static("require-corp"),
        ))
        .layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .unwrap_or_else(|error| panic!("cannot bind {}: {error}", config.bind));
    info!(
        address = %listener.local_addr().expect("listener address"),
        static_dir = %config.static_dir.display(),
        "CellHub server listening"
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(state))
        .await
        .expect("server failed");
}

async fn health() -> &'static str {
    "ok"
}

async fn room_metrics(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::Json<serde_json::Value> {
    use std::sync::atomic::Ordering::Relaxed;
    let rooms = state.rooms.read().await;
    let values: Vec<_> = rooms
        .values()
        .map(|room| {
            serde_json::json!({
                "id": room.id,
                "players": room.player_count.load(Relaxed),
                "population": room.population.load(Relaxed),
                "tick": room.tick.load(Relaxed),
                "stepUs": room.step_us.load(Relaxed),
                "streamUs": room.stream_us.load(Relaxed),
                "tickUs": room.tick_us.load(Relaxed),
                "overruns": room.overruns.load(Relaxed),
                "estimatedOutboundBytes": room.bytes.load(Relaxed),
                "queuedCommands": room.tx.max_capacity() - room.tx.capacity(),
            })
        })
        .collect();
    axum::Json(serde_json::json!({ "tickBudgetUs": 33333, "rooms": values }))
}

struct Config {
    bind: SocketAddr,
    static_dir: PathBuf,
    max_rooms: usize,
    max_players: usize,
}

impl Config {
    fn from_args() -> Self {
        let mut config = Self {
            bind: "127.0.0.1:8082".parse().expect("default bind"),
            static_dir: PathBuf::from("dist"),
            max_rooms: 64,
            max_players: 40,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            let value = args.next().unwrap_or_else(|| {
                panic!("missing value for {arg}");
            });
            match arg.as_str() {
                "--bind" => {
                    config.bind = value.parse().unwrap_or_else(|error| {
                        panic!("invalid --bind address {value}: {error}");
                    })
                }
                "--static" => config.static_dir = PathBuf::from(value),
                "--max-rooms" => {
                    config.max_rooms = value.parse().unwrap_or_else(|error| {
                        panic!("invalid --max-rooms {value}: {error}");
                    })
                }
                "--max-players" => {
                    config.max_players = value.parse().unwrap_or_else(|error| {
                        panic!("invalid --max-players {value}: {error}");
                    })
                }
                other => panic!("unknown argument {other}"),
            }
        }
        config
    }
}

async fn shutdown_signal(state: Arc<AppState>) {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
    for room in state.rooms.read().await.values() {
        room.stopping
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
