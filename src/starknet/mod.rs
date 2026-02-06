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
abigen!(Starknet, "abi/starknet_core_contract.json");

/// Start watching Starknet L1 contract events
pub async fn start_starknet_watcher(
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) -> eyre::Result<()> {
    // Connect to Ethereum node
    let ws_url = env::var("RPC_WS")?;
    let provider = Provider::<Ws>::connect(&ws_url).await?;
    let client = Arc::new(provider);
    tracing::info!(rollup = "starknet", "Connected to Ethereum node");

    // Load contract address
    let starknet_core_address: Address = env::var("STARKNET_CORE_ADDRESS")?.parse()?;
    tracing::info!(
        rollup = "starknet",
        core_contract = ?starknet_core_address,
        "Contract address loaded"
    );

    // Instantiate contract binding
    let starknet_core = Arc::new(Starknet::new(starknet_core_address, client.clone()));

    // Spawn watcher for LogStateUpdate events
    spawn_state_update_watcher(
        starknet_core.clone(),
        state.clone(),
        health.clone(),
        reconnect_config.clone(),
        cancel_token.child_token(),
    );

    // Spawn watcher for LogMessageToL2 events
    spawn_message_watcher(
        starknet_core,
        state,
        health,
        reconnect_config,
        cancel_token.child_token(),
    );

    Ok(())
}

/// Watch for LogStateUpdate events (state diffs posted to L1)
fn spawn_state_update_watcher(
    starknet_core: Arc<Starknet<Provider<Ws>>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(
                    rollup = "starknet",
                    stream = "state_update",
                    "Watcher cancelled"
                );
                return;
            }

            let event_filter = starknet_core
                .event::<LogStateUpdateFilter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "starknet",
                "state_update",
                &reconnect_config,
                &cancel_token,
                || async { event_filter.stream_with_meta().await },
            )
            .await;

            let mut stream = match stream_result {
                ReconnectResult::Connected(s) => s,
                ReconnectResult::MaxRetriesExceeded => {
                    tracing::error!(
                        rollup = "starknet",
                        stream = "state_update",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(
                        rollup = "starknet",
                        stream = "state_update",
                        "Watcher cancelled"
                    );
                    return;
                }
            };

            tracing::info!(
                rollup = "starknet",
                stream = "state_update",
                "Stream connected"
            );

            loop {
                tokio::select! {
                    result = stream.next() => {
                        match result {
                            Some(Ok((event, meta))) => {
                                let block_number = meta.block_number.as_u64();
                                let tx_hash = format!("{:?}", meta.transaction_hash);
                                let block_hash = event.block_hash.to_string();

                                let rollup_event = RollupEvent {
                                    rollup: "starknet".into(),
                                    event_type: "StateUpdate".into(),
                                    block_number,
                                    tx_hash,
                                    batch_number: Some(block_hash.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                state.update_status("starknet", |status| {
                                    status.latest_batch = Some(block_hash.clone());
                                    // Starknet state updates are verified by STARK proofs
                                    status.latest_proof = Some(block_hash.clone());
                                    status.latest_finalized = Some(block_hash.clone());
                                    status.last_updated = Some(Utc::now().timestamp() as u64);
                                });

                                // Record event for health monitoring
                                health.record_event(&rollup_event);

                                state.broadcast(rollup_event);

                                tracing::info!(
                                    rollup = "starknet",
                                    event = "StateUpdate",
                                    starknet_block = %block_hash,
                                    l1_block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "starknet",
                                    stream = "state_update",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "starknet",
                                    stream = "state_update",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "starknet",
                            stream = "state_update",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            rollup = "starknet",
                            stream = "state_update",
                            "Watcher cancelled"
                        );
                        return;
                    }
                }
            }
        }
    });
}

/// Watch for LogMessageToL2 events (L1 -> L2 messages)
fn spawn_message_watcher(
    starknet_core: Arc<Starknet<Provider<Ws>>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(rollup = "starknet", stream = "message", "Watcher cancelled");
                return;
            }

            let event_filter = starknet_core
                .event::<LogMessageToL2Filter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "starknet",
                "message",
                &reconnect_config,
                &cancel_token,
                || async { event_filter.stream_with_meta().await },
            )
            .await;

            let mut stream = match stream_result {
                ReconnectResult::Connected(s) => s,
                ReconnectResult::MaxRetriesExceeded => {
                    tracing::error!(
                        rollup = "starknet",
                        stream = "message",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(rollup = "starknet", stream = "message", "Watcher cancelled");
                    return;
                }
            };

            tracing::info!(rollup = "starknet", stream = "message", "Stream connected");

            loop {
                tokio::select! {
                    result = stream.next() => {
                        match result {
                            Some(Ok((event, meta))) => {
                                let block_number = meta.block_number.as_u64();
                                let tx_hash = format!("{:?}", meta.transaction_hash);
                                let selector = event.selector.to_string();

                                let rollup_event = RollupEvent {
                                    rollup: "starknet".into(),
                                    event_type: "MessageLog".into(),
                                    block_number,
                                    tx_hash,
                                    batch_number: Some(selector.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                state.update_status("starknet", |status| {
                                    status.last_updated = Some(Utc::now().timestamp() as u64);
                                });

                                // Record event for health monitoring
                                health.record_event(&rollup_event);

                                state.broadcast(rollup_event);

                                tracing::info!(
                                    rollup = "starknet",
                                    event = "MessageLog",
                                    selector = %selector,
                                    l1_block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "starknet",
                                    stream = "message",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "starknet",
                                    stream = "message",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "starknet",
                            stream = "message",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(rollup = "starknet", stream = "message", "Watcher cancelled");
                        return;
                    }
                }
            }
        }
    });
}
