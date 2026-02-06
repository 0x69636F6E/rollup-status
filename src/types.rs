use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

use crate::config::BroadcastConfig;

/// Represents an event from a rollup posted to L1
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RollupStatus {
    /// Latest batch posted to L1
    pub latest_batch: Option<String>,
    /// Transaction hash for the latest batch
    pub latest_batch_tx: Option<String>,
    /// Latest proof/assertion submitted
    pub latest_proof: Option<String>,
    /// Transaction hash for the latest proof
    pub latest_proof_tx: Option<String>,
    /// Latest finalized/confirmed state
    pub latest_finalized: Option<String>,
    /// Transaction hash for the latest finalized state
    pub latest_finalized_tx: Option<String>,
    /// Unix timestamp of last update
    pub last_updated: Option<u64>,
}

/// Health status of a rollup
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum HealthStatus {
    /// Rollup is operating normally
    #[default]
    Healthy,
    /// Rollup is experiencing delays
    Delayed,
    /// Rollup has halted (no updates for extended period)
    Halted,
    /// Rollup appears disconnected from L1
    Disconnected,
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
        Self::with_config(BroadcastConfig::default())
    }

    /// Create a new AppState with custom configuration
    pub fn with_config(config: BroadcastConfig) -> Self {
        let (tx, _rx) = broadcast::channel::<RollupEvent>(config.channel_capacity);
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
        match self.statuses.write() {
            Ok(mut statuses) => {
                let entry = statuses.entry(rollup.to_string()).or_default();
                updater(entry);
            }
            Err(poisoned) => {
                tracing::error!(
                    rollup = rollup,
                    "RwLock poisoned in update_status, recovering"
                );
                let mut statuses = poisoned.into_inner();
                let entry = statuses.entry(rollup.to_string()).or_default();
                updater(entry);
            }
        }
    }

    /// Get the status for a specific rollup
    pub fn get_status(&self, rollup: &str) -> RollupStatus {
        match self.statuses.read() {
            Ok(statuses) => statuses.get(rollup).cloned().unwrap_or_default(),
            Err(poisoned) => {
                tracing::error!(rollup = rollup, "RwLock poisoned in get_status, recovering");
                poisoned
                    .into_inner()
                    .get(rollup)
                    .cloned()
                    .unwrap_or_default()
            }
        }
    }

    /// Get all statuses (with lock error handling)
    pub fn get_all_statuses(&self) -> HashMap<String, RollupStatus> {
        match self.statuses.read() {
            Ok(statuses) => statuses.clone(),
            Err(poisoned) => {
                tracing::error!("RwLock poisoned in get_all_statuses, recovering");
                poisoned.into_inner().clone()
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_new() {
        let state = AppState::new();
        assert!(state.statuses.read().unwrap().is_empty());
    }

    #[test]
    fn test_app_state_update_and_get_status() {
        let state = AppState::new();

        // Initially empty
        let status = state.get_status("arbitrum");
        assert_eq!(status, RollupStatus::default());

        // Update status
        state.update_status("arbitrum", |s| {
            s.latest_batch = Some("100".to_string());
            s.last_updated = Some(1234567890);
        });

        let status = state.get_status("arbitrum");
        assert_eq!(status.latest_batch, Some("100".to_string()));
        assert_eq!(status.last_updated, Some(1234567890));

        // Other rollup still empty
        let starknet_status = state.get_status("starknet");
        assert_eq!(starknet_status, RollupStatus::default());
    }

    #[test]
    fn test_app_state_get_all_statuses() {
        let state = AppState::new();

        state.update_status("arbitrum", |s| {
            s.latest_batch = Some("100".to_string());
        });
        state.update_status("starknet", |s| {
            s.latest_batch = Some("200".to_string());
        });

        let all = state.get_all_statuses();
        assert_eq!(all.len(), 2);
        assert_eq!(
            all.get("arbitrum").unwrap().latest_batch,
            Some("100".to_string())
        );
        assert_eq!(
            all.get("starknet").unwrap().latest_batch,
            Some("200".to_string())
        );
    }

    #[test]
    fn test_app_state_broadcast() {
        let state = AppState::new();
        let mut rx = state.tx.subscribe();

        let event = RollupEvent {
            rollup: "arbitrum".to_string(),
            event_type: "BatchDelivered".to_string(),
            block_number: 12345,
            tx_hash: "0xabc".to_string(),
            batch_number: Some("100".to_string()),
            timestamp: Some(1234567890),
        };

        state.broadcast(event.clone());

        let received = rx.try_recv().unwrap();
        assert_eq!(received, event);
    }

    #[test]
    fn test_health_status_default() {
        assert_eq!(HealthStatus::default(), HealthStatus::Healthy);
    }

    #[test]
    fn test_rollup_status_default() {
        let status = RollupStatus::default();
        assert!(status.latest_batch.is_none());
        assert!(status.latest_batch_tx.is_none());
        assert!(status.latest_proof.is_none());
        assert!(status.latest_proof_tx.is_none());
        assert!(status.latest_finalized.is_none());
        assert!(status.latest_finalized_tx.is_none());
        assert!(status.last_updated.is_none());
    }
}
