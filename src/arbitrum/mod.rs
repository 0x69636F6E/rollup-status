use crate::types::{AppState, RollupEvent};
use chrono::Utc;
use ethers::prelude::*;
use std::{env, sync::Arc};
use tokio_stream::StreamExt;

// Generate contract bindings from ABI
abigen!(Sequencer, "abi/arbitrum_sequencer_inbox.json");
abigen!(RollupCore, "abi/arbitrum_rollup_core.json");

/// Start watching Arbitrum L1 contract events
pub async fn start_arbitrum_watcher(state: AppState) -> eyre::Result<()> {
    // Connect to Ethereum node
    let ws_url = env::var("RPC_WS")?;
    let provider = Provider::<Ws>::connect(&ws_url).await?;
    let client = Arc::new(provider);
    println!("[Arbitrum] Connected to Ethereum node");

    // Load contract addresses
    let sequencer_address: Address = env::var("ARBITRUM_INBOX_ADDRESS")?.parse()?;
    let rollup_core_address: Address = env::var("ARBITRUM_ROLLUP_CORE")?.parse()?;

    println!("[Arbitrum] SequencerInbox: {:?}", sequencer_address);
    println!("[Arbitrum] RollupCore: {:?}", rollup_core_address);

    // Instantiate contract bindings
    let sequencer = Sequencer::new(sequencer_address, client.clone());
    let rollup_core = Arc::new(RollupCore::new(rollup_core_address, client.clone()));

    // Spawn watcher for BatchDelivered events
    spawn_batch_watcher(sequencer, state.clone());

    // Spawn watcher for AssertionCreated events (proofs submitted)
    spawn_assertion_created_watcher(rollup_core.clone(), state.clone());

    // Spawn watcher for AssertionConfirmed events (proofs verified)
    spawn_assertion_confirmed_watcher(rollup_core, state);

    Ok(())
}

/// Watch for SequencerBatchDelivered events
fn spawn_batch_watcher(sequencer: Sequencer<Provider<Ws>>, state: AppState) {
    tokio::spawn(async move {
        let event_filter = sequencer
            .event::<SequencerBatchDeliveredFilter>()
            .from_block(BlockNumber::Latest);

        let mut stream = match event_filter.stream_with_meta().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[Arbitrum] Failed to create batch stream: {:?}", e);
                return;
            }
        };

        while let Some(Ok((event, meta))) = stream.next().await {
            let block_number = meta.block_number.as_u64();
            let tx_hash = format!("{:?}", meta.transaction_hash);
            let batch_num = event.batch_sequence_number.to_string();

            let rollup_event = RollupEvent {
                rollup: "arbitrum".into(),
                event_type: "BatchDelivered".into(),
                block_number,
                tx_hash,
                batch_number: Some(batch_num.clone()),
                timestamp: Some(Utc::now().timestamp() as u64),
            };

            // Update shared state
            state.update_status("arbitrum", |status| {
                status.latest_batch = Some(batch_num.clone());
                status.last_updated = Some(Utc::now().timestamp() as u64);
            });

            // Broadcast to WebSocket clients
            state.broadcast(rollup_event);

            println!(
                "[Arbitrum] BatchDelivered #{} @ L1 block {}",
                batch_num, block_number
            );
        }
    });
}

/// Watch for AssertionCreated events (proofs submitted)
fn spawn_assertion_created_watcher(rollup_core: Arc<RollupCore<Provider<Ws>>>, state: AppState) {
    tokio::spawn(async move {
        let event_filter = rollup_core
            .event::<AssertionCreatedFilter>()
            .from_block(BlockNumber::Latest);

        let mut stream = match event_filter.stream_with_meta().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[Arbitrum] Failed to create assertion stream: {:?}", e);
                return;
            }
        };

        while let Some(Ok((event, meta))) = stream.next().await {
            let block_number = meta.block_number.as_u64();
            let tx_hash = format!("{:?}", meta.transaction_hash);
            let assertion_hash = format!("0x{}", hex::encode(event.assertion_hash));

            let rollup_event = RollupEvent {
                rollup: "arbitrum".into(),
                event_type: "ProofSubmitted".into(),
                block_number,
                tx_hash,
                batch_number: Some(assertion_hash.clone()),
                timestamp: Some(Utc::now().timestamp() as u64),
            };

            state.update_status("arbitrum", |status| {
                status.latest_proof = Some(assertion_hash.clone());
                status.last_updated = Some(Utc::now().timestamp() as u64);
            });

            state.broadcast(rollup_event);

            println!(
                "[Arbitrum] ProofSubmitted {} @ L1 block {}",
                &assertion_hash[..18],
                block_number
            );
        }
    });
}

/// Watch for AssertionConfirmed events (proofs verified/finalized)
fn spawn_assertion_confirmed_watcher(rollup_core: Arc<RollupCore<Provider<Ws>>>, state: AppState) {
    tokio::spawn(async move {
        let event_filter = rollup_core
            .event::<AssertionConfirmedFilter>()
            .from_block(BlockNumber::Latest);

        let mut stream = match event_filter.stream_with_meta().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[Arbitrum] Failed to create confirmation stream: {:?}", e);
                return;
            }
        };

        while let Some(Ok((event, meta))) = stream.next().await {
            let block_number = meta.block_number.as_u64();
            let tx_hash = format!("{:?}", meta.transaction_hash);
            let assertion_hash = format!("0x{}", hex::encode(event.assertion_hash));

            let rollup_event = RollupEvent {
                rollup: "arbitrum".into(),
                event_type: "ProofVerified".into(),
                block_number,
                tx_hash,
                batch_number: Some(assertion_hash.clone()),
                timestamp: Some(Utc::now().timestamp() as u64),
            };

            state.update_status("arbitrum", |status| {
                status.latest_finalized = Some(assertion_hash.clone());
                status.last_updated = Some(Utc::now().timestamp() as u64);
            });

            state.broadcast(rollup_event);

            println!(
                "[Arbitrum] ProofVerified {} @ L1 block {}",
                &assertion_hash[..18],
                block_number
            );
        }
    });
}
