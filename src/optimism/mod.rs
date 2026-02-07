use crate::config::ReconnectConfig;
use crate::health::HealthMonitor;
use crate::reconnect::{connect_with_retry, ReconnectResult};
use crate::types::{AppState, RollupEvent};
use chrono::Utc;
use ethers::prelude::*;
use std::{env, sync::Arc};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

// Generate contract bindings from ABI (same OP Stack ABIs as Base, renamed types)
abigen!(OpDisputeGameFactory, "abi/base_dispute_game_factory.json");
abigen!(OpOptimismPortal, "abi/base_optimism_portal.json");

/// Start watching Optimism L1 contract events
pub async fn start_optimism_watcher(
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) -> eyre::Result<()> {
    // Connect to Ethereum node
    let ws_url = env::var("RPC_WS")?;
    let provider = Provider::<Ws>::connect(&ws_url).await?;
    let client = Arc::new(provider);
    tracing::info!(rollup = "optimism", "Connected to Ethereum node");

    // Load contract addresses (Optimism mainnet)
    let dispute_factory_address: Address = env::var("OPTIMISM_DISPUTE_GAME_FACTORY")?
        .parse()
        .map_err(|e| eyre::eyre!("Invalid OPTIMISM_DISPUTE_GAME_FACTORY address: {}", e))?;

    let portal_address: Address = env::var("OPTIMISM_PORTAL")?
        .parse()
        .map_err(|e| eyre::eyre!("Invalid OPTIMISM_PORTAL address: {}", e))?;

    tracing::info!(
        rollup = "optimism",
        dispute_game_factory = ?dispute_factory_address,
        optimism_portal = ?portal_address,
        "Contract addresses loaded"
    );

    // Instantiate contract bindings
    let dispute_factory = Arc::new(OpDisputeGameFactory::new(
        dispute_factory_address,
        client.clone(),
    ));
    let portal = Arc::new(OpOptimismPortal::new(portal_address, client.clone()));

    // Spawn watcher for DisputeGameCreated events (state root proposals)
    spawn_dispute_game_watcher(
        dispute_factory,
        state.clone(),
        health.clone(),
        reconnect_config.clone(),
        cancel_token.child_token(),
    );

    // Spawn watcher for WithdrawalProven events (withdrawal proofs)
    spawn_withdrawal_proven_watcher(
        portal,
        state,
        health,
        reconnect_config,
        cancel_token.child_token(),
    );

    Ok(())
}

/// Watch for DisputeGameCreated events (new state root proposals)
fn spawn_dispute_game_watcher(
    factory: Arc<OpDisputeGameFactory<Provider<Ws>>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(
                    rollup = "optimism",
                    stream = "dispute_game",
                    "Watcher cancelled"
                );
                return;
            }

            let event_filter = factory
                .event::<DisputeGameCreatedFilter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "optimism",
                "dispute_game",
                &reconnect_config,
                &cancel_token,
                || async { event_filter.stream_with_meta().await },
            )
            .await;

            let mut stream = match stream_result {
                ReconnectResult::Connected(s) => s,
                ReconnectResult::MaxRetriesExceeded => {
                    tracing::error!(
                        rollup = "optimism",
                        stream = "dispute_game",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(
                        rollup = "optimism",
                        stream = "dispute_game",
                        "Watcher cancelled"
                    );
                    return;
                }
            };

            tracing::info!(rollup = "optimism", stream = "dispute_game", "Stream connected");

            loop {
                tokio::select! {
                    result = stream.next() => {
                        match result {
                            Some(Ok((event, meta))) => {
                                let block_number = meta.block_number.as_u64();
                                let tx_hash = format!("{:?}", meta.transaction_hash);
                                let root_claim = format!("0x{}", hex::encode(event.root_claim));
                                let game_proxy = format!("{:?}", event.dispute_proxy);

                                let rollup_event = RollupEvent {
                                    rollup: "optimism".into(),
                                    event_type: "DisputeGameCreated".into(),
                                    block_number,
                                    tx_hash: tx_hash.clone(),
                                    batch_number: Some(root_claim.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                // Update shared state
                                state.update_status("optimism", |status| {
                                    status.latest_batch = Some(root_claim.clone());
                                    status.latest_batch_tx = Some(tx_hash.clone());
                                    status.latest_proof = Some(root_claim.clone());
                                    status.latest_proof_tx = Some(tx_hash.clone());
                                    status.last_updated = Some(Utc::now().timestamp() as u64);
                                });

                                // Record event for health monitoring
                                health.record_event(&rollup_event);

                                // Broadcast to WebSocket clients
                                state.broadcast(rollup_event);

                                let short_claim = if root_claim.len() >= 18 {
                                    &root_claim[..18]
                                } else {
                                    &root_claim
                                };

                                tracing::info!(
                                    rollup = "optimism",
                                    event = "DisputeGameCreated",
                                    root_claim = %short_claim,
                                    game_proxy = %game_proxy,
                                    block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "optimism",
                                    stream = "dispute_game",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "optimism",
                                    stream = "dispute_game",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "optimism",
                            stream = "dispute_game",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            rollup = "optimism",
                            stream = "dispute_game",
                            "Watcher cancelled"
                        );
                        return;
                    }
                }
            }
        }
    });
}

/// Watch for WithdrawalProven events
fn spawn_withdrawal_proven_watcher(
    portal: Arc<OpOptimismPortal<Provider<Ws>>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(
                    rollup = "optimism",
                    stream = "withdrawal_proven",
                    "Watcher cancelled"
                );
                return;
            }

            let event_filter = portal
                .event::<WithdrawalProvenFilter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "optimism",
                "withdrawal_proven",
                &reconnect_config,
                &cancel_token,
                || async { event_filter.stream_with_meta().await },
            )
            .await;

            let mut stream = match stream_result {
                ReconnectResult::Connected(s) => s,
                ReconnectResult::MaxRetriesExceeded => {
                    tracing::error!(
                        rollup = "optimism",
                        stream = "withdrawal_proven",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(
                        rollup = "optimism",
                        stream = "withdrawal_proven",
                        "Watcher cancelled"
                    );
                    return;
                }
            };

            tracing::info!(
                rollup = "optimism",
                stream = "withdrawal_proven",
                "Stream connected"
            );

            loop {
                tokio::select! {
                    result = stream.next() => {
                        match result {
                            Some(Ok((event, meta))) => {
                                let block_number = meta.block_number.as_u64();
                                let tx_hash = format!("{:?}", meta.transaction_hash);
                                let withdrawal_hash = format!("0x{}", hex::encode(event.withdrawal_hash));

                                let rollup_event = RollupEvent {
                                    rollup: "optimism".into(),
                                    event_type: "WithdrawalProven".into(),
                                    block_number,
                                    tx_hash: tx_hash.clone(),
                                    batch_number: Some(withdrawal_hash.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                // Update timestamp for health tracking
                                state.update_status("optimism", |status| {
                                    status.latest_finalized = Some(withdrawal_hash.clone());
                                    status.latest_finalized_tx = Some(tx_hash.clone());
                                    status.last_updated = Some(Utc::now().timestamp() as u64);
                                });

                                // Record event for health monitoring
                                health.record_event(&rollup_event);

                                // Broadcast to WebSocket clients
                                state.broadcast(rollup_event);

                                let short_hash = if withdrawal_hash.len() >= 18 {
                                    &withdrawal_hash[..18]
                                } else {
                                    &withdrawal_hash
                                };

                                tracing::info!(
                                    rollup = "optimism",
                                    event = "WithdrawalProven",
                                    withdrawal_hash = %short_hash,
                                    block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "optimism",
                                    stream = "withdrawal_proven",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "optimism",
                                    stream = "withdrawal_proven",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "optimism",
                            stream = "withdrawal_proven",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            rollup = "optimism",
                            stream = "withdrawal_proven",
                            "Watcher cancelled"
                        );
                        return;
                    }
                }
            }
        }
    });
}
