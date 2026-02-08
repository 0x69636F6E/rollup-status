use std::time::Duration;

use ethers::providers::{Http, Middleware, Provider};
use ethers::types::BlockNumber;
use tokio_util::sync::CancellationToken;

use crate::health::HealthMonitor;
use crate::types::AppState;

/// Configuration for an L2 chain sequencer poller
#[derive(Debug, Clone)]
pub struct L2ChainConfig {
    /// Name of the rollup (e.g., "arbitrum", "base")
    pub name: String,
    /// HTTP RPC URL for the L2 chain
    pub rpc_url: String,
    /// How often to poll for new blocks
    pub poll_interval: Duration,
    /// How long without a new block before declaring downtime
    pub downtime_threshold: Duration,
}

/// Start polling an L2 chain's sequencer for latest block info.
///
/// Updates `AppState` sequencer status and records activity/downtime on `HealthMonitor`.
/// Runs until the `cancel_token` is cancelled.
pub async fn start_sequencer_poller(
    config: L2ChainConfig,
    state: AppState,
    health: HealthMonitor,
    cancel_token: CancellationToken,
) {
    let provider = match Provider::<Http>::try_from(config.rpc_url.as_str()) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(
                rollup = %config.name,
                error = ?e,
                "Failed to create L2 provider"
            );
            return;
        }
    };

    tracing::info!(
        rollup = %config.name,
        rpc_url = %config.rpc_url,
        poll_ms = config.poll_interval.as_millis() as u64,
        "Starting L2 sequencer poller"
    );

    let mut interval = tokio::time::interval(config.poll_interval);
    let mut prev_block: Option<u64> = None;
    let mut prev_poll_time: Option<u64> = None;

    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = cancel_token.cancelled() => {
                tracing::info!(rollup = %config.name, "Sequencer poller shutting down");
                return;
            }
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        match provider.get_block(BlockNumber::Latest).await {
            Ok(Some(block)) => {
                let block_number = block.number.map(|n| n.as_u64());
                let block_timestamp = block.timestamp.as_u64();

                // Calculate blocks per second from previous poll
                let blocks_per_second = match (block_number, prev_block, prev_poll_time) {
                    (Some(current), Some(previous), Some(prev_time))
                        if current > previous && now > prev_time =>
                    {
                        let block_delta = (current - previous) as f64;
                        let time_delta = (now - prev_time) as f64;
                        Some(block_delta / time_delta)
                    }
                    _ => None,
                };

                // Detect downtime: now - block_timestamp > threshold
                let seconds_since_last_block = now.saturating_sub(block_timestamp);
                let is_producing = seconds_since_last_block < config.downtime_threshold.as_secs();

                state.update_sequencer_status(&config.name, |s| {
                    s.latest_block = block_number;
                    s.latest_block_timestamp = Some(block_timestamp);
                    if let Some(bps) = blocks_per_second {
                        s.blocks_per_second = Some(bps);
                    }
                    s.is_producing = is_producing;
                    s.seconds_since_last_block = Some(seconds_since_last_block);
                    s.last_polled = Some(now);
                });

                if is_producing {
                    health.record_sequencer_activity(&config.name);
                } else {
                    health.record_sequencer_downtime(&config.name, seconds_since_last_block);
                }

                tracing::debug!(
                    rollup = %config.name,
                    block = ?block_number,
                    timestamp = block_timestamp,
                    bps = ?blocks_per_second,
                    producing = is_producing,
                    "L2 sequencer poll"
                );

                if let Some(bn) = block_number {
                    prev_block = Some(bn);
                }
                prev_poll_time = Some(now);
            }
            Ok(None) => {
                tracing::warn!(
                    rollup = %config.name,
                    "L2 latest block returned None"
                );

                state.update_sequencer_status(&config.name, |s| {
                    s.is_producing = false;
                    s.last_polled = Some(now);
                });
                health.record_sequencer_downtime(&config.name, 0);
            }
            Err(e) => {
                tracing::warn!(
                    rollup = %config.name,
                    error = ?e,
                    "Failed to fetch L2 latest block"
                );

                state.update_sequencer_status(&config.name, |s| {
                    s.is_producing = false;
                    s.last_polled = Some(now);
                });
                health.record_sequencer_downtime(&config.name, 0);
            }
        }
    }
}
