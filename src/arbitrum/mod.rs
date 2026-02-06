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
abigen!(Sequencer, "abi/arbitrum_sequencer_inbox.json");
abigen!(RollupCore, "abi/arbitrum_rollup_core.json");

/// Start watching Arbitrum L1 contract events
pub async fn start_arbitrum_watcher(
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) -> eyre::Result<()> {
    // Connect to Ethereum node
    let ws_url = env::var("RPC_WS")?;
    let provider = Provider::<Ws>::connect(&ws_url).await?;
    let client = Arc::new(provider);
    tracing::info!(rollup = "arbitrum", "Connected to Ethereum node");

    // Load contract addresses
    let sequencer_address: Address = env::var("ARBITRUM_INBOX_ADDRESS")?.parse()?;
    let rollup_core_address: Address = env::var("ARBITRUM_ROLLUP_CORE")?.parse()?;

    tracing::info!(
        rollup = "arbitrum",
        sequencer_inbox = ?sequencer_address,
        rollup_core = ?rollup_core_address,
        "Contract addresses loaded"
    );

    // Instantiate contract bindings
    let sequencer = Sequencer::new(sequencer_address, client.clone());
    let rollup_core = Arc::new(RollupCore::new(rollup_core_address, client.clone()));

    // Spawn watcher for BatchDelivered events
    spawn_batch_watcher(
        sequencer,
        state.clone(),
        health.clone(),
        reconnect_config.clone(),
        cancel_token.child_token(),
    );

    // Spawn watcher for AssertionCreated events (proofs submitted)
    spawn_assertion_created_watcher(
        rollup_core.clone(),
        state.clone(),
        health.clone(),
        reconnect_config.clone(),
        cancel_token.child_token(),
    );

    // Spawn watcher for AssertionConfirmed events (proofs verified)
    spawn_assertion_confirmed_watcher(
        rollup_core,
        state,
        health,
        reconnect_config,
        cancel_token.child_token(),
    );

    Ok(())
}

/// Watch for SequencerBatchDelivered events
fn spawn_batch_watcher(
    sequencer: Sequencer<Provider<Ws>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(rollup = "arbitrum", stream = "batch", "Watcher cancelled");
                return;
            }

            let event_filter = sequencer
                .event::<SequencerBatchDeliveredFilter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "arbitrum",
                "batch",
                &reconnect_config,
                &cancel_token,
                || async { event_filter.stream_with_meta().await },
            )
            .await;

            let mut stream = match stream_result {
                ReconnectResult::Connected(s) => s,
                ReconnectResult::MaxRetriesExceeded => {
                    tracing::error!(
                        rollup = "arbitrum",
                        stream = "batch",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(rollup = "arbitrum", stream = "batch", "Watcher cancelled");
                    return;
                }
            };

            tracing::info!(rollup = "arbitrum", stream = "batch", "Stream connected");

            loop {
                tokio::select! {
                    result = stream.next() => {
                        match result {
                            Some(Ok((event, meta))) => {
                                let block_number = meta.block_number.as_u64();
                                let tx_hash = format!("{:?}", meta.transaction_hash);
                                let batch_num = event.batch_sequence_number.to_string();

                                let rollup_event = RollupEvent {
                                    rollup: "arbitrum".into(),
                                    event_type: "BatchDelivered".into(),
                                    block_number,
                                    tx_hash: tx_hash.clone(),
                                    batch_number: Some(batch_num.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                // Update shared state
                                state.update_status("arbitrum", |status| {
                                    status.latest_batch = Some(batch_num.clone());
                                    status.latest_batch_tx = Some(tx_hash.clone());
                                    status.last_updated = Some(Utc::now().timestamp() as u64);
                                });

                                // Record event for health monitoring
                                health.record_event(&rollup_event);

                                // Broadcast to WebSocket clients
                                state.broadcast(rollup_event);

                                tracing::info!(
                                    rollup = "arbitrum",
                                    event = "BatchDelivered",
                                    batch = %batch_num,
                                    block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "arbitrum",
                                    stream = "batch",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "arbitrum",
                                    stream = "batch",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "arbitrum",
                            stream = "batch",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(rollup = "arbitrum", stream = "batch", "Watcher cancelled");
                        return;
                    }
                }
            }
        }
    });
}

/// Watch for AssertionCreated events (proofs submitted)
fn spawn_assertion_created_watcher(
    rollup_core: Arc<RollupCore<Provider<Ws>>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(
                    rollup = "arbitrum",
                    stream = "assertion_created",
                    "Watcher cancelled"
                );
                return;
            }

            let event_filter = rollup_core
                .event::<AssertionCreatedFilter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "arbitrum",
                "assertion_created",
                &reconnect_config,
                &cancel_token,
                || async { event_filter.stream_with_meta().await },
            )
            .await;

            let mut stream = match stream_result {
                ReconnectResult::Connected(s) => s,
                ReconnectResult::MaxRetriesExceeded => {
                    tracing::error!(
                        rollup = "arbitrum",
                        stream = "assertion_created",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(
                        rollup = "arbitrum",
                        stream = "assertion_created",
                        "Watcher cancelled"
                    );
                    return;
                }
            };

            tracing::info!(
                rollup = "arbitrum",
                stream = "assertion_created",
                "Stream connected"
            );

            loop {
                tokio::select! {
                    result = stream.next() => {
                        match result {
                            Some(Ok((event, meta))) => {
                                let block_number = meta.block_number.as_u64();
                                let tx_hash = format!("{:?}", meta.transaction_hash);
                                let assertion_hash = format!("0x{}", hex::encode(event.assertion_hash));

                                let rollup_event = RollupEvent {
                                    rollup: "arbitrum".into(),
                                    event_type: "ProofSubmitted".into(),
                                    block_number,
                                    tx_hash: tx_hash.clone(),
                                    batch_number: Some(assertion_hash.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                state.update_status("arbitrum", |status| {
                                    status.latest_proof = Some(assertion_hash.clone());
                                    status.latest_proof_tx = Some(tx_hash.clone());
                                    status.last_updated = Some(Utc::now().timestamp() as u64);
                                });

                                // Record event for health monitoring
                                health.record_event(&rollup_event);

                                state.broadcast(rollup_event);

                                let short_hash = if assertion_hash.len() >= 18 {
                                    &assertion_hash[..18]
                                } else {
                                    &assertion_hash
                                };

                                tracing::info!(
                                    rollup = "arbitrum",
                                    event = "ProofSubmitted",
                                    assertion = %short_hash,
                                    block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "arbitrum",
                                    stream = "assertion_created",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "arbitrum",
                                    stream = "assertion_created",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "arbitrum",
                            stream = "assertion_created",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            rollup = "arbitrum",
                            stream = "assertion_created",
                            "Watcher cancelled"
                        );
                        return;
                    }
                }
            }
        }
    });
}

/// Watch for AssertionConfirmed events (proofs verified/finalized)
fn spawn_assertion_confirmed_watcher(
    rollup_core: Arc<RollupCore<Provider<Ws>>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(
                    rollup = "arbitrum",
                    stream = "assertion_confirmed",
                    "Watcher cancelled"
                );
                return;
            }

            let event_filter = rollup_core
                .event::<AssertionConfirmedFilter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "arbitrum",
                "assertion_confirmed",
                &reconnect_config,
                &cancel_token,
                || async { event_filter.stream_with_meta().await },
            )
            .await;

            let mut stream = match stream_result {
                ReconnectResult::Connected(s) => s,
                ReconnectResult::MaxRetriesExceeded => {
                    tracing::error!(
                        rollup = "arbitrum",
                        stream = "assertion_confirmed",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(
                        rollup = "arbitrum",
                        stream = "assertion_confirmed",
                        "Watcher cancelled"
                    );
                    return;
                }
            };

            tracing::info!(
                rollup = "arbitrum",
                stream = "assertion_confirmed",
                "Stream connected"
            );

            loop {
                tokio::select! {
                    result = stream.next() => {
                        match result {
                            Some(Ok((event, meta))) => {
                                let block_number = meta.block_number.as_u64();
                                let tx_hash = format!("{:?}", meta.transaction_hash);
                                let assertion_hash = format!("0x{}", hex::encode(event.assertion_hash));

                                let rollup_event = RollupEvent {
                                    rollup: "arbitrum".into(),
                                    event_type: "ProofVerified".into(),
                                    block_number,
                                    tx_hash: tx_hash.clone(),
                                    batch_number: Some(assertion_hash.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                state.update_status("arbitrum", |status| {
                                    status.latest_finalized = Some(assertion_hash.clone());
                                    status.latest_finalized_tx = Some(tx_hash.clone());
                                    status.last_updated = Some(Utc::now().timestamp() as u64);
                                });

                                // Record event for health monitoring
                                health.record_event(&rollup_event);

                                state.broadcast(rollup_event);

                                let short_hash = if assertion_hash.len() >= 18 {
                                    &assertion_hash[..18]
                                } else {
                                    &assertion_hash
                                };

                                tracing::info!(
                                    rollup = "arbitrum",
                                    event = "ProofVerified",
                                    assertion = %short_hash,
                                    block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "arbitrum",
                                    stream = "assertion_confirmed",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "arbitrum",
                                    stream = "assertion_confirmed",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "arbitrum",
                            stream = "assertion_confirmed",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            rollup = "arbitrum",
                            stream = "assertion_confirmed",
                            "Watcher cancelled"
                        );
                        return;
                    }
                }
            }
        }
    });
}
