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
abigen!(DisputeGameFactory, "abi/base_dispute_game_factory.json");
abigen!(OptimismPortal, "abi/base_optimism_portal.json");

/// Start watching Base L1 contract events
pub async fn start_base_watcher(
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) -> eyre::Result<()> {
    // Connect to Ethereum node
    let ws_url = env::var("RPC_WS")?;
    let provider = Provider::<Ws>::connect(&ws_url).await?;
    let client = Arc::new(provider);
    tracing::info!(rollup = "base", "Connected to Ethereum node");

    // Load contract addresses (Base mainnet)
    let dispute_factory_address: Address = env::var("BASE_DISPUTE_GAME_FACTORY")?
        .parse()
        .map_err(|e| eyre::eyre!("Invalid BASE_DISPUTE_GAME_FACTORY address: {}", e))?;

    let portal_address: Address = env::var("BASE_OPTIMISM_PORTAL")?
        .parse()
        .map_err(|e| eyre::eyre!("Invalid BASE_OPTIMISM_PORTAL address: {}", e))?;

    tracing::info!(
        rollup = "base",
        dispute_game_factory = ?dispute_factory_address,
        optimism_portal = ?portal_address,
        "Contract addresses loaded"
    );

    // Instantiate contract bindings
    let dispute_factory = Arc::new(DisputeGameFactory::new(
        dispute_factory_address,
        client.clone(),
    ));
    let portal = Arc::new(OptimismPortal::new(portal_address, client.clone()));

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
    factory: Arc<DisputeGameFactory<Provider<Ws>>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(
                    rollup = "base",
                    stream = "dispute_game",
                    "Watcher cancelled"
                );
                return;
            }

            let event_filter = factory
                .event::<DisputeGameCreatedFilter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "base",
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
                        rollup = "base",
                        stream = "dispute_game",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(
                        rollup = "base",
                        stream = "dispute_game",
                        "Watcher cancelled"
                    );
                    return;
                }
            };

            tracing::info!(rollup = "base", stream = "dispute_game", "Stream connected");

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
                                    rollup: "base".into(),
                                    event_type: "DisputeGameCreated".into(),
                                    block_number,
                                    tx_hash: tx_hash.clone(),
                                    batch_number: Some(root_claim.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                // Update shared state
                                state.update_status("base", |status| {
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
                                    rollup = "base",
                                    event = "DisputeGameCreated",
                                    root_claim = %short_claim,
                                    game_proxy = %game_proxy,
                                    block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "base",
                                    stream = "dispute_game",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "base",
                                    stream = "dispute_game",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "base",
                            stream = "dispute_game",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            rollup = "base",
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
    portal: Arc<OptimismPortal<Provider<Ws>>>,
    state: AppState,
    health: HealthMonitor,
    reconnect_config: ReconnectConfig,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            if cancel_token.is_cancelled() {
                tracing::info!(
                    rollup = "base",
                    stream = "withdrawal_proven",
                    "Watcher cancelled"
                );
                return;
            }

            let event_filter = portal
                .event::<WithdrawalProvenFilter>()
                .from_block(BlockNumber::Latest);

            let stream_result = connect_with_retry(
                "base",
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
                        rollup = "base",
                        stream = "withdrawal_proven",
                        "Max retries exceeded, stopping watcher"
                    );
                    return;
                }
                ReconnectResult::Cancelled => {
                    tracing::info!(
                        rollup = "base",
                        stream = "withdrawal_proven",
                        "Watcher cancelled"
                    );
                    return;
                }
            };

            tracing::info!(
                rollup = "base",
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
                                    rollup: "base".into(),
                                    event_type: "WithdrawalProven".into(),
                                    block_number,
                                    tx_hash: tx_hash.clone(),
                                    batch_number: Some(withdrawal_hash.clone()),
                                    timestamp: Some(Utc::now().timestamp() as u64),
                                };

                                // Update timestamp for health tracking
                                state.update_status("base", |status| {
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
                                    rollup = "base",
                                    event = "WithdrawalProven",
                                    withdrawal_hash = %short_hash,
                                    block = block_number,
                                    "Event received"
                                );
                            }
                            Some(Err(e)) => {
                                tracing::warn!(
                                    rollup = "base",
                                    stream = "withdrawal_proven",
                                    error = ?e,
                                    "Stream error, will reconnect"
                                );
                                break;
                            }
                            None => {
                                tracing::warn!(
                                    rollup = "base",
                                    stream = "withdrawal_proven",
                                    "Stream ended, reconnecting"
                                );
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(reconnect_config.stale_timeout) => {
                        tracing::warn!(
                            rollup = "base",
                            stream = "withdrawal_proven",
                            timeout_secs = reconnect_config.stale_timeout.as_secs(),
                            "Stale filter detected, forcing reconnect"
                        );
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(
                            rollup = "base",
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
