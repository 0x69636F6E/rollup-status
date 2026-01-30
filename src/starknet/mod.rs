use crate::types::{AppState, RollupEvent};
use chrono::Utc;
use ethers::prelude::*;
use std::{env, sync::Arc};
use tokio_stream::StreamExt;

// Generate contract bindings from ABI
abigen!(Starknet, "abi/starknet_core_contract.json");

/// Start watching Starknet L1 contract events
pub async fn start_starknet_watcher(state: AppState) -> eyre::Result<()> {
    // Connect to Ethereum node
    let ws_url = env::var("RPC_WS")?;
    let provider = Provider::<Ws>::connect(&ws_url).await?;
    let client = Arc::new(provider);
    println!("[Starknet] Connected to Ethereum node");

    // Load contract address
    let starknet_core_address: Address = env::var("STARKNET_CORE_ADDRESS")?.parse()?;
    println!("[Starknet] Core contract: {:?}", starknet_core_address);

    // Instantiate contract binding
    let starknet_core = Arc::new(Starknet::new(starknet_core_address, client.clone()));

    // Spawn watcher for LogStateUpdate events
    spawn_state_update_watcher(starknet_core.clone(), state.clone());

    // Spawn watcher for LogMessageToL2 events
    spawn_message_watcher(starknet_core, state);

    Ok(())
}

/// Watch for LogStateUpdate events (state diffs posted to L1)
fn spawn_state_update_watcher(starknet_core: Arc<Starknet<Provider<Ws>>>, state: AppState) {
    tokio::spawn(async move {
        let event_filter = starknet_core
            .event::<LogStateUpdateFilter>()
            .from_block(BlockNumber::Latest);

        let mut stream = match event_filter.stream_with_meta().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[Starknet] Failed to create state update stream: {:?}", e);
                return;
            }
        };

        while let Some(Ok((event, meta))) = stream.next().await {
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

            state.broadcast(rollup_event);

            println!(
                "[Starknet] StateUpdate block {} @ L1 block {}",
                block_hash, block_number
            );
        }
    });
}

/// Watch for LogMessageToL2 events (L1 -> L2 messages)
fn spawn_message_watcher(starknet_core: Arc<Starknet<Provider<Ws>>>, state: AppState) {
    tokio::spawn(async move {
        let event_filter = starknet_core
            .event::<LogMessageToL2Filter>()
            .from_block(BlockNumber::Latest);

        let mut stream = match event_filter.stream_with_meta().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[Starknet] Failed to create message stream: {:?}", e);
                return;
            }
        };

        while let Some(Ok((event, meta))) = stream.next().await {
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

            state.broadcast(rollup_event);

            println!(
                "[Starknet] MessageLog selector {} @ L1 block {}",
                selector, block_number
            );
        }
    });
}
