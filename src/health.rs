use crate::config::HealthCheckConfig;
use crate::types::{HealthStatus, RollupEvent};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;

/// Configuration for health monitoring thresholds
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Rollup-specific thresholds
    pub rollups: HashMap<String, RollupHealthConfig>,
    /// Default thresholds for unknown rollups
    pub default: RollupHealthConfig,
}

/// Health thresholds for a specific rollup
#[derive(Debug, Clone)]
pub struct RollupHealthConfig {
    /// Maximum seconds between events before marking as "delayed"
    pub delayed_threshold_secs: u64,
    /// Maximum seconds between events before marking as "halted"
    pub halted_threshold_secs: u64,
    /// Maximum seconds between batch posts
    pub batch_cadence_secs: u64,
    /// Maximum seconds between proof submissions
    pub proof_cadence_secs: u64,
}

impl Default for RollupHealthConfig {
    fn default() -> Self {
        Self {
            delayed_threshold_secs: 600, // 10 minutes
            halted_threshold_secs: 1800, // 30 minutes
            batch_cadence_secs: 300,     // 5 minutes
            proof_cadence_secs: 3600,    // 1 hour
        }
    }
}

impl Default for HealthConfig {
    fn default() -> Self {
        let mut rollups = HashMap::new();

        // Arbitrum: batches every ~few minutes, proofs every ~1 hour
        rollups.insert(
            "arbitrum".to_string(),
            RollupHealthConfig {
                delayed_threshold_secs: 600, // 10 minutes
                halted_threshold_secs: 1800, // 30 minutes
                batch_cadence_secs: 300,     // 5 minutes
                proof_cadence_secs: 3600,    // 1 hour
            },
        );

        // Starknet: state updates every ~few hours
        rollups.insert(
            "starknet".to_string(),
            RollupHealthConfig {
                delayed_threshold_secs: 7200, // 2 hours
                halted_threshold_secs: 14400, // 4 hours
                batch_cadence_secs: 3600,     // 1 hour
                proof_cadence_secs: 7200,     // 2 hours
            },
        );

        // Base: OP Stack with fault proofs, dispute games every ~1 hour
        rollups.insert(
            "base".to_string(),
            RollupHealthConfig {
                delayed_threshold_secs: 3600, // 1 hour
                halted_threshold_secs: 7200,  // 2 hours
                batch_cadence_secs: 1800,     // 30 minutes
                proof_cadence_secs: 3600,     // 1 hour
            },
        );

        // Optimism: OP Stack same as Base
        rollups.insert(
            "optimism".to_string(),
            RollupHealthConfig {
                delayed_threshold_secs: 3600, // 1 hour
                halted_threshold_secs: 7200,  // 2 hours
                batch_cadence_secs: 1800,     // 30 minutes
                proof_cadence_secs: 3600,     // 1 hour
            },
        );

        // zkSync Era: ZK proofs come in batches, ~hourly
        rollups.insert(
            "zksync".to_string(),
            RollupHealthConfig {
                delayed_threshold_secs: 7200, // 2 hours
                halted_threshold_secs: 14400, // 4 hours
                batch_cadence_secs: 1800,     // 30 minutes
                proof_cadence_secs: 3600,     // 1 hour
            },
        );

        Self {
            rollups,
            default: RollupHealthConfig::default(),
        }
    }
}

/// Tracks health state for all rollups
#[derive(Clone)]
pub struct HealthMonitor {
    /// Health configuration
    config: HealthConfig,
    /// Current health status for each rollup
    health_states: Arc<RwLock<HashMap<String, RollupHealthState>>>,
}

/// Internal health state tracking
#[derive(Debug, Clone, PartialEq)]
pub struct RollupHealthState {
    /// Current health status
    pub status: HealthStatus,
    /// Timestamp of last batch event
    pub last_batch_time: Option<u64>,
    /// Timestamp of last proof event
    pub last_proof_time: Option<u64>,
    /// Timestamp of last any event
    pub last_event_time: Option<u64>,
    /// Count of consecutive missed cadences
    pub missed_cadences: u32,
    /// Timestamp of last sequencer activity
    pub last_sequencer_activity: Option<u64>,
    /// Whether the L2 sequencer is producing blocks
    pub sequencer_producing: bool,
}

impl Default for RollupHealthState {
    fn default() -> Self {
        Self {
            status: HealthStatus::Healthy,
            last_batch_time: None,
            last_proof_time: None,
            last_event_time: None,
            missed_cadences: 0,
            last_sequencer_activity: None,
            sequencer_producing: false,
        }
    }
}

/// Health check result with details
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct HealthCheckResult {
    pub rollup: String,
    pub status: HealthStatus,
    pub last_event_age_secs: Option<u64>,
    pub last_batch_age_secs: Option<u64>,
    pub last_proof_age_secs: Option<u64>,
    pub sequencer_down: Option<bool>,
    pub sequencer_down_secs: Option<u64>,
    pub issues: Vec<String>,
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new() -> Self {
        Self {
            config: HealthConfig::default(),
            health_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get config for a specific rollup
    pub fn get_config(&self, rollup: &str) -> &RollupHealthConfig {
        self.config
            .rollups
            .get(rollup)
            .unwrap_or(&self.config.default)
    }

    /// Get current unix timestamp
    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Record an event and update health state
    pub fn record_event(&self, event: &RollupEvent) {
        let now = Self::now();

        let mut states = match self.health_states.write() {
            Ok(states) => states,
            Err(poisoned) => {
                tracing::error!(
                    rollup = %event.rollup,
                    "RwLock poisoned in record_event, recovering"
                );
                poisoned.into_inner()
            }
        };

        let state = states.entry(event.rollup.clone()).or_default();

        // Update timestamps based on event type
        state.last_event_time = Some(now);

        match event.event_type.as_str() {
            "BatchDelivered" | "StateUpdate" | "BlockCommit" => {
                state.last_batch_time = Some(now);
            }
            "ProofSubmitted" | "ProofVerified" | "AssertionCreated" | "AssertionConfirmed"
            | "DisputeGameCreated" | "BlocksVerification" | "BlockExecution" => {
                state.last_proof_time = Some(now);
            }
            "WithdrawalProven" => {
                // Withdrawal proofs indicate activity but are user-initiated
                state.last_proof_time = Some(now);
            }
            _ => {}
        }

        // Reset missed cadences on any event
        state.missed_cadences = 0;

        // Re-evaluate health
        let config = self.get_config(&event.rollup);
        state.status = Self::evaluate_health_static(state, config);
    }

    /// Evaluate health status based on current state (static version for internal use)
    fn evaluate_health_static(
        state: &RollupHealthState,
        config: &RollupHealthConfig,
    ) -> HealthStatus {
        let now = Self::now();

        // Check last event time
        if let Some(last_event) = state.last_event_time {
            let age = now.saturating_sub(last_event);

            if age > config.halted_threshold_secs {
                return HealthStatus::Halted;
            }
            if age > config.delayed_threshold_secs {
                return HealthStatus::Delayed;
            }
        } else {
            // No events ever received
            return HealthStatus::Disconnected;
        }

        HealthStatus::Healthy
    }

    /// Record L2 sequencer activity (block production observed)
    pub fn record_sequencer_activity(&self, rollup: &str) {
        let now = Self::now();
        let mut states = match self.health_states.write() {
            Ok(states) => states,
            Err(poisoned) => {
                tracing::error!(
                    rollup = rollup,
                    "RwLock poisoned in record_sequencer_activity, recovering"
                );
                poisoned.into_inner()
            }
        };
        let state = states.entry(rollup.to_string()).or_default();
        state.last_sequencer_activity = Some(now);
        state.sequencer_producing = true;
    }

    /// Record L2 sequencer downtime (no new blocks for given seconds)
    pub fn record_sequencer_downtime(&self, rollup: &str, _secs: u64) {
        let mut states = match self.health_states.write() {
            Ok(states) => states,
            Err(poisoned) => {
                tracing::error!(
                    rollup = rollup,
                    "RwLock poisoned in record_sequencer_downtime, recovering"
                );
                poisoned.into_inner()
            }
        };
        let state = states.entry(rollup.to_string()).or_default();
        state.sequencer_producing = false;
    }

    /// Run a health check for a specific rollup
    pub fn check_health(&self, rollup: &str) -> HealthCheckResult {
        let now = Self::now();

        let states = match self.health_states.read() {
            Ok(states) => states,
            Err(poisoned) => {
                tracing::error!(
                    rollup = rollup,
                    "RwLock poisoned in check_health, recovering"
                );
                poisoned.into_inner()
            }
        };

        let config = self.get_config(rollup);
        let state = states.get(rollup);
        let mut issues = Vec::new();

        let (status, last_event_age, last_batch_age, last_proof_age, seq_down, seq_down_secs) =
            match state {
                Some(s) => {
                    let event_age = s.last_event_time.map(|t| now.saturating_sub(t));
                    let batch_age = s.last_batch_time.map(|t| now.saturating_sub(t));
                    let proof_age = s.last_proof_time.map(|t| now.saturating_sub(t));

                    // Check for issues
                    if let Some(age) = event_age {
                        if age > config.halted_threshold_secs {
                            issues.push(format!(
                                "No events for {} seconds (halted threshold: {})",
                                age, config.halted_threshold_secs
                            ));
                        } else if age > config.delayed_threshold_secs {
                            issues.push(format!(
                                "No events for {} seconds (delayed threshold: {})",
                                age, config.delayed_threshold_secs
                            ));
                        }
                    }

                    if let Some(age) = batch_age {
                        if age > config.batch_cadence_secs {
                            issues.push(format!(
                                "No batch for {} seconds (expected cadence: {})",
                                age, config.batch_cadence_secs
                            ));
                        }
                    }

                    if let Some(age) = proof_age {
                        if age > config.proof_cadence_secs {
                            issues.push(format!(
                                "No proof for {} seconds (expected cadence: {})",
                                age, config.proof_cadence_secs
                            ));
                        }
                    }

                    // Sequencer health
                    let (seq_down, seq_secs) =
                        if let Some(last_activity) = s.last_sequencer_activity {
                            let age = now.saturating_sub(last_activity);
                            if !s.sequencer_producing {
                                issues.push(format!(
                                    "Sequencer not producing blocks for {} seconds",
                                    age
                                ));
                                (Some(true), Some(age))
                            } else {
                                (Some(false), Some(age))
                            }
                        } else {
                            (None, None)
                        };

                    (
                        s.status.clone(),
                        event_age,
                        batch_age,
                        proof_age,
                        seq_down,
                        seq_secs,
                    )
                }
                None => {
                    issues.push("No events received yet".to_string());
                    (HealthStatus::Disconnected, None, None, None, None, None)
                }
            };

        HealthCheckResult {
            rollup: rollup.to_string(),
            status,
            last_event_age_secs: last_event_age,
            last_batch_age_secs: last_batch_age,
            last_proof_age_secs: last_proof_age,
            sequencer_down: seq_down,
            sequencer_down_secs: seq_down_secs,
            issues,
        }
    }

    /// Run periodic health evaluation for all rollups
    pub fn evaluate_all(&self) -> Vec<HealthCheckResult> {
        ["arbitrum", "starknet", "base", "optimism", "zksync"]
            .iter()
            .map(|r| self.check_health(r))
            .collect()
    }

    /// Get current health status for a rollup
    pub fn get_status(&self, rollup: &str) -> HealthStatus {
        let states = match self.health_states.read() {
            Ok(states) => states,
            Err(poisoned) => {
                tracing::error!(rollup = rollup, "RwLock poisoned in get_status, recovering");
                poisoned.into_inner()
            }
        };

        states
            .get(rollup)
            .map(|s| s.status.clone())
            .unwrap_or(HealthStatus::Disconnected)
    }
}

/// Start the background health monitoring task
pub async fn start_health_monitor(
    monitor: HealthMonitor,
    health_config: HealthCheckConfig,
    cancel_token: CancellationToken,
) {
    let check_interval = health_config.check_interval;

    tracing::info!(
        interval_secs = check_interval.as_secs(),
        "Starting health monitor"
    );

    loop {
        tokio::select! {
            _ = tokio::time::sleep(check_interval) => {}
            _ = cancel_token.cancelled() => {
                tracing::info!("Health monitor shutting down");
                return;
            }
        }

        // Re-evaluate health for all rollups
        let results = monitor.evaluate_all();

        for result in &results {
            if !result.issues.is_empty() {
                tracing::warn!(
                    rollup = %result.rollup,
                    status = ?result.status,
                    issues = ?result.issues,
                    "Health check issues detected"
                );
            }
        }

        // Update health states based on time passage
        let mut states = match monitor.health_states.write() {
            Ok(states) => states,
            Err(poisoned) => {
                tracing::error!("RwLock poisoned in health monitor loop, recovering");
                poisoned.into_inner()
            }
        };

        for rollup in ["arbitrum", "starknet", "base", "optimism", "zksync"] {
            let config = monitor.get_config(rollup);
            if let Some(state) = states.get_mut(rollup) {
                state.status = HealthMonitor::evaluate_health_static(state, config);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_monitor_new() {
        let monitor = HealthMonitor::new();
        assert_eq!(monitor.get_status("arbitrum"), HealthStatus::Disconnected);
        assert_eq!(monitor.get_status("starknet"), HealthStatus::Disconnected);
    }

    #[test]
    fn test_record_batch_event() {
        let monitor = HealthMonitor::new();

        let event = RollupEvent {
            rollup: "arbitrum".to_string(),
            event_type: "BatchDelivered".to_string(),
            block_number: 12345,
            tx_hash: "0xabc".to_string(),
            batch_number: Some("100".to_string()),
            timestamp: Some(1234567890),
        };

        monitor.record_event(&event);

        // After recording an event, status should be healthy
        assert_eq!(monitor.get_status("arbitrum"), HealthStatus::Healthy);
    }

    #[test]
    fn test_record_proof_event() {
        let monitor = HealthMonitor::new();

        let event = RollupEvent {
            rollup: "arbitrum".to_string(),
            event_type: "ProofSubmitted".to_string(),
            block_number: 12345,
            tx_hash: "0xabc".to_string(),
            batch_number: Some("assertion_hash".to_string()),
            timestamp: Some(1234567890),
        };

        monitor.record_event(&event);

        assert_eq!(monitor.get_status("arbitrum"), HealthStatus::Healthy);
    }

    #[test]
    fn test_check_health_no_events() {
        let monitor = HealthMonitor::new();

        let result = monitor.check_health("arbitrum");
        assert_eq!(result.status, HealthStatus::Disconnected);
        assert!(result
            .issues
            .contains(&"No events received yet".to_string()));
    }

    #[test]
    fn test_check_health_with_events() {
        let monitor = HealthMonitor::new();

        let event = RollupEvent {
            rollup: "arbitrum".to_string(),
            event_type: "BatchDelivered".to_string(),
            block_number: 12345,
            tx_hash: "0xabc".to_string(),
            batch_number: Some("100".to_string()),
            timestamp: Some(1234567890),
        };

        monitor.record_event(&event);

        let result = monitor.check_health("arbitrum");
        assert_eq!(result.status, HealthStatus::Healthy);
        assert!(result.last_event_age_secs.is_some());
        assert!(result.last_batch_age_secs.is_some());
    }

    #[test]
    fn test_evaluate_all() {
        let monitor = HealthMonitor::new();

        let results = monitor.evaluate_all();
        assert_eq!(results.len(), 5);

        let arbitrum = results.iter().find(|r| r.rollup == "arbitrum").unwrap();
        let starknet = results.iter().find(|r| r.rollup == "starknet").unwrap();
        let base = results.iter().find(|r| r.rollup == "base").unwrap();
        let optimism = results.iter().find(|r| r.rollup == "optimism").unwrap();
        let zksync = results.iter().find(|r| r.rollup == "zksync").unwrap();

        assert_eq!(arbitrum.status, HealthStatus::Disconnected);
        assert_eq!(starknet.status, HealthStatus::Disconnected);
        assert_eq!(base.status, HealthStatus::Disconnected);
        assert_eq!(optimism.status, HealthStatus::Disconnected);
        assert_eq!(zksync.status, HealthStatus::Disconnected);
    }

    #[test]
    fn test_health_config_defaults() {
        let config = HealthConfig::default();

        let arbitrum_config = config.rollups.get("arbitrum").unwrap();
        assert_eq!(arbitrum_config.delayed_threshold_secs, 600);
        assert_eq!(arbitrum_config.halted_threshold_secs, 1800);

        let starknet_config = config.rollups.get("starknet").unwrap();
        assert_eq!(starknet_config.delayed_threshold_secs, 7200);
        assert_eq!(starknet_config.halted_threshold_secs, 14400);

        let base_config = config.rollups.get("base").unwrap();
        assert_eq!(base_config.delayed_threshold_secs, 3600);
        assert_eq!(base_config.halted_threshold_secs, 7200);

        let optimism_config = config.rollups.get("optimism").unwrap();
        assert_eq!(optimism_config.delayed_threshold_secs, 3600);
        assert_eq!(optimism_config.halted_threshold_secs, 7200);

        let zksync_config = config.rollups.get("zksync").unwrap();
        assert_eq!(zksync_config.delayed_threshold_secs, 7200);
        assert_eq!(zksync_config.halted_threshold_secs, 14400);
    }

    #[test]
    fn test_evaluate_health_static_no_events() {
        let state = RollupHealthState::default();
        let config = RollupHealthConfig::default();

        let status = HealthMonitor::evaluate_health_static(&state, &config);
        assert_eq!(status, HealthStatus::Disconnected);
    }

    #[test]
    fn test_evaluate_health_static_healthy() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let state = RollupHealthState {
            status: HealthStatus::Healthy,
            last_event_time: Some(now),
            last_batch_time: Some(now),
            last_proof_time: Some(now),
            missed_cadences: 0,
            last_sequencer_activity: None,
            sequencer_producing: false,
        };
        let config = RollupHealthConfig::default();

        let status = HealthMonitor::evaluate_health_static(&state, &config);
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn test_record_sequencer_activity() {
        let monitor = HealthMonitor::new();

        monitor.record_sequencer_activity("arbitrum");

        let states = monitor.health_states.read().unwrap();
        let state = states.get("arbitrum").unwrap();
        assert!(state.sequencer_producing);
        assert!(state.last_sequencer_activity.is_some());
    }

    #[test]
    fn test_record_sequencer_downtime() {
        let monitor = HealthMonitor::new();

        // First record activity so the state exists
        monitor.record_sequencer_activity("arbitrum");
        // Then record downtime
        monitor.record_sequencer_downtime("arbitrum", 45);

        let states = monitor.health_states.read().unwrap();
        let state = states.get("arbitrum").unwrap();
        assert!(!state.sequencer_producing);
    }

    #[test]
    fn test_check_health_with_sequencer_down() {
        let monitor = HealthMonitor::new();

        // Record an L1 event first so status isn't Disconnected
        let event = RollupEvent {
            rollup: "arbitrum".to_string(),
            event_type: "BatchDelivered".to_string(),
            block_number: 12345,
            tx_hash: "0xabc".to_string(),
            batch_number: Some("100".to_string()),
            timestamp: Some(1234567890),
        };
        monitor.record_event(&event);

        // Record sequencer activity then downtime
        monitor.record_sequencer_activity("arbitrum");
        monitor.record_sequencer_downtime("arbitrum", 60);

        let result = monitor.check_health("arbitrum");
        assert_eq!(result.sequencer_down, Some(true));
        assert!(result.sequencer_down_secs.is_some());
        assert!(result
            .issues
            .iter()
            .any(|i| i.contains("Sequencer not producing")));
    }
}
