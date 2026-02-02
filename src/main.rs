use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{header, Method};
use axum::serve;
use axum::{extract::State, response::IntoResponse, routing::{get, post}, Json, Router};
use dotenv::dotenv;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};

mod arbitrum;
mod base;
mod config;
mod health;
mod reconnect;
mod starknet;
mod types;

pub use config::Config;
pub use health::HealthMonitor;
pub use types::{AppState, RollupEvent, RollupStatus};

/// Combined state for API handlers
#[derive(Clone)]
pub struct ApiState {
    pub app: AppState,
    pub health: HealthMonitor,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    // Load configuration
    let config = Config::from_env();

    tracing::info!("Starting Rollup Proof Status backend");

    // Create cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();

    // Create shared global state
    let app_state = AppState::with_config(config.broadcast.clone());

    // Create health monitor
    let health_monitor = HealthMonitor::new();

    // Spawn the Arbitrum watcher
    let arbitrum_state = app_state.clone();
    let arbitrum_health = health_monitor.clone();
    let arbitrum_reconnect = config.reconnect.clone();
    let arbitrum_cancel = cancel_token.child_token();
    tokio::spawn(async move {
        if let Err(e) = arbitrum::start_arbitrum_watcher(
            arbitrum_state,
            arbitrum_health,
            arbitrum_reconnect,
            arbitrum_cancel,
        )
        .await
        {
            tracing::error!(rollup = "arbitrum", error = ?e, "Watcher failed to start");
        }
    });

    // Spawn the Starknet watcher
    let starknet_state = app_state.clone();
    let starknet_health = health_monitor.clone();
    let starknet_reconnect = config.reconnect.clone();
    let starknet_cancel = cancel_token.child_token();
    tokio::spawn(async move {
        if let Err(e) = starknet::start_starknet_watcher(
            starknet_state,
            starknet_health,
            starknet_reconnect,
            starknet_cancel,
        )
        .await
        {
            tracing::error!(rollup = "starknet", error = ?e, "Watcher failed to start");
        }
    });

    // Spawn the Base watcher
    let base_state = app_state.clone();
    let base_health = health_monitor.clone();
    let base_reconnect = config.reconnect.clone();
    let base_cancel = cancel_token.child_token();
    tokio::spawn(async move {
        if let Err(e) =
            base::start_base_watcher(base_state, base_health, base_reconnect, base_cancel).await
        {
            tracing::error!(rollup = "base", error = ?e, "Watcher failed to start");
        }
    });

    // Spawn the health monitor background task
    let monitor_clone = health_monitor.clone();
    let health_config = config.health.clone();
    let health_cancel = cancel_token.child_token();
    tokio::spawn(async move {
        health::start_health_monitor(monitor_clone, health_config, health_cancel).await;
    });

    // Combined API state
    let api_state = ApiState {
        app: app_state,
        health: health_monitor,
    };

    // CORS configuration for cross-origin requests from frontend
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE]);

    // Build Axum routes
    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(service_health))
        .route("/rollups", get(list_rollups))
        .route("/rollups/arbitrum/status", get(get_arbitrum_status))
        .route("/rollups/starknet/status", get(get_starknet_status))
        .route("/rollups/base/status", get(get_base_status))
        .route("/rollups/arbitrum/health", get(get_arbitrum_health))
        .route("/rollups/starknet/health", get(get_starknet_health))
        .route("/rollups/base/health", get(get_base_health))
        .route("/rollups/health", get(get_all_health))
        .route("/rollups/stream", get(ws_handler))
        .route("/test/event", post(post_test_event))
        .layer(cors)
        .with_state(api_state);

    // Parse socket address
    let addr: std::net::SocketAddr = config
        .server
        .addr()
        .parse()
        .map_err(|e| eyre::eyre!("Invalid server address '{}': {}", config.server.addr(), e))?;

    tracing::info!(
        host = %config.server.host,
        port = config.server.port,
        "API server starting"
    );
    tracing::info!("Endpoints:");
    tracing::info!("  GET  /                         - Root");
    tracing::info!("  GET  /health                   - Service health check");
    tracing::info!("  GET  /rollups                  - List supported rollups");
    tracing::info!("  GET  /rollups/arbitrum/status  - Arbitrum status");
    tracing::info!("  GET  /rollups/starknet/status  - Starknet status");
    tracing::info!("  GET  /rollups/base/status      - Base status");
    tracing::info!("  GET  /rollups/arbitrum/health  - Arbitrum health");
    tracing::info!("  GET  /rollups/starknet/health  - Starknet health");
    tracing::info!("  GET  /rollups/base/health      - Base health");
    tracing::info!("  GET  /rollups/health           - All rollups health");
    tracing::info!("  WS   /rollups/stream           - Real-time event stream");

    let listener = TcpListener::bind(addr).await?;

    // Setup graceful shutdown
    let shutdown_token = cancel_token.clone();
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                tracing::info!("Received shutdown signal, initiating graceful shutdown");
                shutdown_token.cancel();
            }
            Err(e) => {
                tracing::error!(error = ?e, "Failed to listen for shutdown signal");
            }
        }
    });

    // Run server with graceful shutdown
    serve(listener, app)
        .with_graceful_shutdown(async move {
            cancel_token.cancelled().await;
            tracing::info!("Shutting down HTTP server");
        })
        .await?;

    tracing::info!("Server shutdown complete");
    Ok(())
}

// ------------------------------------------
// REST Endpoints
// ------------------------------------------

async fn root() -> &'static str {
    "Rollup Proof Status API - Track L2 rollup commitments on Ethereum L1"
}

async fn service_health() -> impl IntoResponse {
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
                "health_endpoint": "/rollups/arbitrum/health",
                "events": ["BatchDelivered", "ProofSubmitted", "ProofVerified"]
            },
            {
                "name": "starknet",
                "status_endpoint": "/rollups/starknet/status",
                "health_endpoint": "/rollups/starknet/health",
                "events": ["StateUpdate", "MessageLog"]
            },
            {
                "name": "base",
                "status_endpoint": "/rollups/base/status",
                "health_endpoint": "/rollups/base/health",
                "events": ["DisputeGameCreated", "WithdrawalProven"]
            }
        ]
    }))
}

async fn get_arbitrum_status(State(state): State<ApiState>) -> impl IntoResponse {
    Json(state.app.get_status("arbitrum"))
}

async fn get_starknet_status(State(state): State<ApiState>) -> impl IntoResponse {
    Json(state.app.get_status("starknet"))
}

async fn get_arbitrum_health(State(state): State<ApiState>) -> impl IntoResponse {
    Json(state.health.check_health("arbitrum"))
}

async fn get_starknet_health(State(state): State<ApiState>) -> impl IntoResponse {
    Json(state.health.check_health("starknet"))
}

async fn get_base_status(State(state): State<ApiState>) -> impl IntoResponse {
    Json(state.app.get_status("base"))
}

async fn get_base_health(State(state): State<ApiState>) -> impl IntoResponse {
    Json(state.health.check_health("base"))
}

async fn get_all_health(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "rollups": state.health.evaluate_all()
    }))
}

// ------------------------------------------
// WebSocket Endpoint
// ------------------------------------------

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<ApiState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: ApiState) {
    let mut rx = state.app.tx.subscribe();

    tracing::info!("New WebSocket client connected");

    // Send initial status to the client (including health)
    let statuses = state.app.get_all_statuses();
    let health = state.health.evaluate_all();
    let initial = serde_json::json!({
        "type": "initial",
        "statuses": statuses,
        "health": health
    });
    if let Ok(json_msg) = serde_json::to_string(&initial) {
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

    tracing::info!("WebSocket client disconnected");
}

// ------------------------------------------
// Test Endpoint (for development only)
// ------------------------------------------

/// Request body for test event endpoint
#[derive(serde::Deserialize)]
struct TestEventRequest {
    rollup: Option<String>,
    event_type: Option<String>,
    block_number: Option<u64>,
    batch_number: Option<String>,
    tx_hash: Option<String>,
}

/// POST /test/event - Broadcast a test event to all WebSocket clients
async fn post_test_event(
    State(state): State<ApiState>,
    Json(req): Json<TestEventRequest>,
) -> impl IntoResponse {
    let event = RollupEvent {
        rollup: req.rollup.unwrap_or_else(|| "arbitrum".to_string()),
        event_type: req.event_type.unwrap_or_else(|| "BatchDelivered".to_string()),
        block_number: req.block_number.unwrap_or(19_000_000),
        tx_hash: req.tx_hash.unwrap_or_else(|| format!("0x{:064x}", rand::random::<u64>())),
        batch_number: req.batch_number.or_else(|| Some("12345".to_string())),
        timestamp: Some(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)),
    };

    tracing::info!(
        rollup = %event.rollup,
        event_type = %event.event_type,
        "Broadcasting test event"
    );

    state.app.broadcast(event.clone());

    Json(serde_json::json!({
        "status": "ok",
        "event": event
    }))
}
