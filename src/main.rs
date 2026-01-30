use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::serve;
use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use dotenv::dotenv;
use tokio::net::TcpListener;

mod arbitrum;
mod starknet;
mod types;

pub use types::{AppState, RollupEvent, RollupStatus};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt::init();
    println!("Starting Rollup Proof Status backend...");

    // Create shared global state
    let state = AppState::new();

    // Spawn the Arbitrum watcher
    let arbitrum_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = arbitrum::start_arbitrum_watcher(arbitrum_state).await {
            eprintln!("Arbitrum watcher failed: {:?}", e);
        }
    });

    // Spawn the Starknet watcher
    let starknet_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = starknet::start_starknet_watcher(starknet_state).await {
            eprintln!("Starknet watcher failed: {:?}", e);
        }
    });

    // Build Axum routes
    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health_check))
        .route("/rollups", get(list_rollups))
        .route("/rollups/arbitrum/status", get(get_arbitrum_status))
        .route("/rollups/starknet/status", get(get_starknet_status))
        .route("/rollups/stream", get(ws_handler))
        .with_state(state);

    // Run Axum HTTP server
    let addr: std::net::SocketAddr = "0.0.0.0:8080".parse().unwrap();
    println!("API running on http://{}", addr);
    println!("Endpoints:");
    println!("  GET  /                        - Root");
    println!("  GET  /health                  - Health check");
    println!("  GET  /rollups                 - List supported rollups");
    println!("  GET  /rollups/arbitrum/status - Arbitrum status");
    println!("  GET  /rollups/starknet/status - Starknet status");
    println!("  WS   /rollups/stream          - Real-time event stream");

    let listener = TcpListener::bind(addr).await?;
    serve(listener, app).await?;
    Ok(())
}

// ------------------------------------------
// REST Endpoints
// ------------------------------------------

async fn root() -> &'static str {
    "Rollup Proof Status API - Track L2 rollup commitments on Ethereum L1"
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "rollup-proof-status"
    }))
}

async fn list_rollups() -> impl IntoResponse {
    Json(serde_json::json!({
        "rollups": [
            {
                "name": "arbitrum",
                "status_endpoint": "/rollups/arbitrum/status",
                "events": ["BatchDelivered", "ProofSubmitted", "ProofVerified"]
            },
            {
                "name": "starknet",
                "status_endpoint": "/rollups/starknet/status",
                "events": ["StateUpdate", "MessageLog"]
            }
        ]
    }))
}

async fn get_arbitrum_status(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.get_status("arbitrum"))
}

async fn get_starknet_status(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.get_status("starknet"))
}

// ------------------------------------------
// WebSocket Endpoint
// ------------------------------------------

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: AppState) {
    let mut rx = state.tx.subscribe();

    println!("New WebSocket client connected");

    // Send initial status to the client
    let statuses = state.statuses.read().unwrap().clone();
    if let Ok(json_msg) = serde_json::to_string(&statuses) {
        let _ = socket.send(Message::Text(json_msg.into())).await;
    }

    // Stream events as they arrive
    while let Ok(event) = rx.recv().await {
        if let Ok(json_msg) = serde_json::to_string(&event) {
            if socket.send(Message::Text(json_msg.into())).await.is_err() {
                break;
            }
        }
    }

    println!("WebSocket client disconnected");
}
