use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

/// Represents an event from a rollup posted to L1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollupEvent {
    /// Name of the rollup (e.g., "arbitrum", "starknet")
    pub rollup: String,
    /// Type of event (e.g., "BatchDelivered", "ProofSubmitted", "StateUpdate")
    pub event_type: String,
    /// L1 block number where the event was emitted
    pub block_number: u64,
    /// Transaction hash on L1
    pub tx_hash: String,
    /// Batch/assertion identifier (rollup-specific)
    pub batch_number: Option<String>,
    /// Unix timestamp when the event was detected
    pub timestamp: Option<u64>,
}

/// Current status of a rollup
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RollupStatus {
    /// Latest batch posted to L1
    pub latest_batch: Option<String>,
    /// Latest proof/assertion submitted
    pub latest_proof: Option<String>,
    /// Latest finalized/confirmed state
    pub latest_finalized: Option<String>,
    /// Unix timestamp of last update
    pub last_updated: Option<u64>,
}

/// Health status of a rollup
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    /// Rollup is operating normally
    Healthy,
    /// Rollup is experiencing delays
    Delayed,
    /// Rollup has halted (no updates for extended period)
    Halted,
    /// Rollup appears disconnected from L1
    Disconnected,
}

impl Default for HealthStatus {
    fn default() -> Self {
        HealthStatus::Healthy
    }
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Current status for each rollup
    pub statuses: Arc<RwLock<HashMap<String, RollupStatus>>>,
    /// Broadcast channel for real-time events
    pub tx: broadcast::Sender<RollupEvent>,
}

impl AppState {
    /// Create a new AppState with default values
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel::<RollupEvent>(100);
        Self {
            statuses: Arc::new(RwLock::new(HashMap::new())),
            tx,
        }
    }

    /// Update the status for a specific rollup
    pub fn update_status<F>(&self, rollup: &str, updater: F)
    where
        F: FnOnce(&mut RollupStatus),
    {
        let mut statuses = self.statuses.write().unwrap();
        let entry = statuses.entry(rollup.to_string()).or_default();
        updater(entry);
    }

    /// Get the status for a specific rollup
    pub fn get_status(&self, rollup: &str) -> RollupStatus {
        let statuses = self.statuses.read().unwrap();
        statuses.get(rollup).cloned().unwrap_or_default()
    }

    /// Broadcast an event to all WebSocket clients
    pub fn broadcast(&self, event: RollupEvent) {
        let _ = self.tx.send(event);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
