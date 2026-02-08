//! Rollup Proof Status - Track L2 rollup commitments on Ethereum L1
//!
//! This library provides types and utilities for monitoring rollup proof
//! submissions and state updates on Ethereum.

pub mod config;
pub mod health;
pub mod reconnect;
pub mod sequencer;
pub mod types;

// Re-export commonly used types
pub use config::{
    BroadcastConfig, Config, HealthCheckConfig, ReconnectConfig, SequencerConfig, ServerConfig,
};
pub use health::{HealthCheckResult, HealthConfig, HealthMonitor, RollupHealthConfig};
pub use reconnect::{connect_with_retry, ReconnectResult};
pub use sequencer::L2ChainConfig;
pub use types::{AppState, HealthStatus, RollupEvent, RollupStatus, SequencerStatus};
