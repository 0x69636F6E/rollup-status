use crate::config::ReconnectConfig;
use crate::health::HealthMonitor;
use crate::reconnect::{connect_with_retry, ReconnectResult};
use crate::types::{AppState, RollupEvent};
use chrono::Utc;
use ethers::prelude::*;
use std::{env, sync::Arc};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

// Generate contract bindings from ABI
abigen!(ZkSyncEra, "abi/zksync_era_diamond.json");

/// Start watching zkSync Era L1 contract events
pub async fn start_zksync_watcher(
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) -> eyre::Result<()> {
    // Connect to Ethereum node
    let ws_url = env::var("RPC_WS")?;
    let provider = Provider::<Ws>::connect(&ws_url).await?;
    let client = Arc::new(provider);
    tracing::info!(rollup = "zksync", "Connected to Ethereum node");

    // Load contract address (zkSync Era mainnet Diamond Proxy)
    let diamond_address: Address = env::var("ZKSYNC_ERA_DIAMOND")?
        .parse()
        .map_err(|e| eyre::eyre!("Invalid ZKSYNC_ERA_DIAMOND address: {}", e))?;

    tracing::info!(
        rollup = "zksync",
        diamond_proxy = ?diamond_address,
        "Contract address loaded"
    );

    // Instantiate contract binding
    let diamond = Arc::new(ZkSyncEra::new(diamond_address, client.clone()));

    // Spawn watcher for BlockCommit events (batch submissions)
    spawn_block_commit_watcher(
        diamond.clone(),
        state.clone(),
        health.clone(),
        reconnect_config.clone(),
        cancel_token.child_token(),
    );

    // Spawn watcher for BlocksVerification events (proof verification)
    spawn_blocks_verification_watcher(
        diamond.clone(),
        state.clone(),
        health.clone(),
        reconnect_config.clone(),
        cancel_token.child_token(),
    );

    // Spawn watcher for BlockExecution events (finalization)
    spawn_block_execution_watcher(
        diamond,
        state,
        health,
        reconnect_config,
        cancel_token.child_token(),
    );

    Ok(())
}

/// Watch for BlockCommit events (new batch submissions)
fn spawn_block_commit_watcher(
    diamond: Arc<ZkSyncEra<Provider<Ws>>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(
                    rollup = "zksync",
                    stream = "block_commit",
                    "Watcher cancelled"
                );
                return;
            }

            let event_filter = diamond
                .event::<BlockCommitFilter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "zksync",
                "block_commit",
                &reconnect_config,
                &cancel_token,
                || async { event_filter.stream_with_meta().await },
            )
            .await;

            let mut stream = match stream_result {
                ReconnectResult::Connected(s) => s,
                ReconnectResult::MaxRetriesExceeded => {
                    tracing::error!(
                        rollup = "zksync",
                        stream = "block_commit",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(
                        rollup = "zksync",
                        stream = "block_commit",
                        "Watcher cancelled"
                    );
                    return;
                }
            };

            tracing::info!(rollup = "zksync", stream = "block_commit", "Stream connected");

            loop {
                tokio::select! {
                    result = stream.next() => {
                        match result {
                            Some(Ok((event, meta))) => {
                                let block_number = meta.block_number.as_u64();
                                let tx_hash = format!("{:?}", meta.transaction_hash);
                                let batch_number = event.batch_number.to_string();

                                let rollup_event = RollupEvent {
                                    rollup: "zksync".into(),
                                    event_type: "BlockCommit".into(),
                                    block_number,
                                    tx_hash: tx_hash.clone(),
                                    batch_number: Some(batch_number.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                // Update shared state
                                state.update_status("zksync", |status| {
                                    status.latest_batch = Some(batch_number.clone());
                                    status.latest_batch_tx = Some(tx_hash.clone());
                                    status.last_updated = Some(Utc::now().timestamp() as u64);
                                });

                                // Record event for health monitoring
                                health.record_event(&rollup_event);

                                // Broadcast to WebSocket clients
                                state.broadcast(rollup_event);

                                tracing::info!(
                                    rollup = "zksync",
                                    event = "BlockCommit",
                                    batch = %batch_number,
                                    block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "zksync",
                                    stream = "block_commit",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "zksync",
                                    stream = "block_commit",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "zksync",
                            stream = "block_commit",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            rollup = "zksync",
                            stream = "block_commit",
                            "Watcher cancelled"
                        );
                        return;
                    }
                }
            }
        }
    });
}

/// Watch for BlocksVerification events (proof verification)
fn spawn_blocks_verification_watcher(
    diamond: Arc<ZkSyncEra<Provider<Ws>>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(
                    rollup = "zksync",
                    stream = "blocks_verification",
                    "Watcher cancelled"
                );
                return;
            }

            let event_filter = diamond
                .event::<BlocksVerificationFilter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "zksync",
                "blocks_verification",
                &reconnect_config,
                &cancel_token,
                || async { event_filter.stream_with_meta().await },
            )
            .await;

            let mut stream = match stream_result {
                ReconnectResult::Connected(s) => s,
                ReconnectResult::MaxRetriesExceeded => {
                    tracing::error!(
                        rollup = "zksync",
                        stream = "blocks_verification",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(
                        rollup = "zksync",
                        stream = "blocks_verification",
                        "Watcher cancelled"
                    );
                    return;
                }
            };

            tracing::info!(
                rollup = "zksync",
                stream = "blocks_verification",
                "Stream connected"
            );

            loop {
                tokio::select! {
                    result = stream.next() => {
                        match result {
                            Some(Ok((event, meta))) => {
                                let block_number = meta.block_number.as_u64();
                                let tx_hash = format!("{:?}", meta.transaction_hash);
                                let verified_batch = event.current_last_verified_batch.to_string();

                                let rollup_event = RollupEvent {
                                    rollup: "zksync".into(),
                                    event_type: "BlocksVerification".into(),
                                    block_number,
                                    tx_hash: tx_hash.clone(),
                                    batch_number: Some(verified_batch.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                // Update shared state
                                state.update_status("zksync", |status| {
                                    status.latest_proof = Some(verified_batch.clone());
                                    status.latest_proof_tx = Some(tx_hash.clone());
                                    status.last_updated = Some(Utc::now().timestamp() as u64);
                                });

                                // Record event for health monitoring
                                health.record_event(&rollup_event);

                                // Broadcast to WebSocket clients
                                state.broadcast(rollup_event);

                                tracing::info!(
                                    rollup = "zksync",
                                    event = "BlocksVerification",
                                    verified_batch = %verified_batch,
                                    block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "zksync",
                                    stream = "blocks_verification",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "zksync",
                                    stream = "blocks_verification",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "zksync",
                            stream = "blocks_verification",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            rollup = "zksync",
                            stream = "blocks_verification",
                            "Watcher cancelled"
                        );
                        return;
                    }
                }
            }
        }
    });
}

/// Watch for BlockExecution events (batch finalization)
fn spawn_block_execution_watcher(
    diamond: Arc<ZkSyncEra<Provider<Ws>>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(
                    rollup = "zksync",
                    stream = "block_execution",
                    "Watcher cancelled"
                );
                return;
            }

            let event_filter = diamond
                .event::<BlockExecutionFilter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "zksync",
                "block_execution",
                &reconnect_config,
                &cancel_token,
                || async { event_filter.stream_with_meta().await },
            )
            .await;

            let mut stream = match stream_result {
                ReconnectResult::Connected(s) => s,
                ReconnectResult::MaxRetriesExceeded => {
                    tracing::error!(
                        rollup = "zksync",
                        stream = "block_execution",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(
                        rollup = "zksync",
                        stream = "block_execution",
                        "Watcher cancelled"
                    );
                    return;
                }
            };

            tracing::info!(
                rollup = "zksync",
                stream = "block_execution",
                "Stream connected"
            );

            loop {
                tokio::select! {
                    result = stream.next() => {
                        match result {
                            Some(Ok((event, meta))) => {
                                let block_number = meta.block_number.as_u64();
                                let tx_hash = format!("{:?}", meta.transaction_hash);
                                let batch_number = event.batch_number.to_string();

                                let rollup_event = RollupEvent {
                                    rollup: "zksync".into(),
                                    event_type: "BlockExecution".into(),
                                    block_number,
                                    tx_hash: tx_hash.clone(),
                                    batch_number: Some(batch_number.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                // Update shared state
                                state.update_status("zksync", |status| {
                                    status.latest_finalized = Some(batch_number.clone());
                                    status.latest_finalized_tx = Some(tx_hash.clone());
                                    status.last_updated = Some(Utc::now().timestamp() as u64);
                                });

                                // Record event for health monitoring
                                health.record_event(&rollup_event);

                                // Broadcast to WebSocket clients
                                state.broadcast(rollup_event);

                                tracing::info!(
                                    rollup = "zksync",
                                    event = "BlockExecution",
                                    batch = %batch_number,
                                    block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "zksync",
                                    stream = "block_execution",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "zksync",
                                    stream = "block_execution",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "zksync",
                            stream = "block_execution",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            rollup = "zksync",
                            stream = "block_execution",
                            "Watcher cancelled"
                        );
                        return;
                    }
                }
            }
        }
    });
}
