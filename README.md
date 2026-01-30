# Rollup Proof Status

A real-time dashboard and API that tracks the lifecycle of Layer 2 rollup commitments as they move through the Ethereum L1 pipeline.

## Overview

Ethereum Layer 2 rollups (Arbitrum, Starknet, Optimism, zkSync, etc.) periodically post batches, proofs, and state commitments to Ethereum L1. This service monitors those on-chain events and provides:

- **Real-time event streaming** via WebSocket
- **REST API** for polling current status
- **Health monitoring** to detect delays or halted rollups

## The Rollup Commitment Lifecycle

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     ROLLUP COMMITMENT LIFECYCLE                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  L2 Sequencer          L1 Ethereum              Verification                │
│  ────────────          ───────────              ────────────                 │
│                                                                              │
│  ┌──────────┐         ┌──────────────┐         ┌──────────────┐             │
│  │ Execute  │         │    Batch     │         │    Proof     │             │
│  │   Txs    │ ──────► │   Posted     │ ──────► │   Submitted  │             │
│  └──────────┘         └──────────────┘         └──────────────┘             │
│       │                     │                        │                       │
│       │                     │                        │                       │
│       ▼                     ▼                        ▼                       │
│  Transactions          Commitment               Cryptographic               │
│  ordered &             on L1                    proof verified               │
│  batched                                                                     │
│                                                                              │
│                                                 ┌──────────────┐             │
│                                                 │   Finalized  │             │
│                                         ──────► │   on L1      │             │
│                                                 └──────────────┘             │
│                                                       │                      │
│                                                       ▼                      │
│                                                 State is                     │
│                                                 irreversible                 │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Event Types Tracked

| Rollup    | Event Type      | Description                                      |
|-----------|-----------------|--------------------------------------------------|
| Arbitrum  | BatchDelivered  | Sequencer posts transaction batch to L1          |
| Arbitrum  | ProofSubmitted  | Assertion/proof submitted (AssertionCreated)     |
| Arbitrum  | ProofVerified   | Assertion confirmed after challenge period       |
| Starknet  | StateUpdate     | State diff and proof posted to L1                |
| Starknet  | MessageLog      | L1<>L2 message logged                            |

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         SYSTEM ARCHITECTURE                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐     ┌──────────────┐     ┌──────────────┐     │
│  │   Ethereum   │     │   Watcher    │     │    Shared    │     │
│  │   Node (WS)  │────►│   Tasks      │────►│    State     │     │
│  └──────────────┘     └──────────────┘     └──────────────┘     │
│                              │                    │              │
│                              │                    │              │
│                              ▼                    ▼              │
│                       ┌──────────────┐     ┌──────────────┐     │
│                       │  Broadcast   │     │   REST API   │     │
│                       │   Channel    │     │   Endpoints  │     │
│                       └──────────────┘     └──────────────┘     │
│                              │                    │              │
│                              ▼                    ▼              │
│                       ┌──────────────┐     ┌──────────────┐     │
│                       │  WebSocket   │     │    JSON      │     │
│                       │   Clients    │     │   Response   │     │
│                       └──────────────┘     └──────────────┘     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Layers

1. **Data Layer** - Connects to Ethereum node via WebSocket, listens to contract events
2. **Interpretation Layer** - Converts raw events into rollup-specific meaning
3. **Assessment Layer** - Applies health rules (lag detection, cadence monitoring)
4. **Reporting Layer** - REST API + WebSocket streaming

## API Endpoints

### REST

| Endpoint                      | Method | Description                    |
|-------------------------------|--------|--------------------------------|
| `/`                           | GET    | Health check                   |
| `/rollups/arbitrum/status`    | GET    | Current Arbitrum status        |
| `/rollups/starknet/status`    | GET    | Current Starknet status        |

### WebSocket

| Endpoint           | Description                              |
|--------------------|------------------------------------------|
| `/rollups/stream`  | Real-time event stream for all rollups   |

### Response Format

**Status Response:**
```json
{
  "latest_batch": "12345",
  "latest_proof": "0xabc...",
  "latest_finalized": "0xdef...",
  "last_updated": 1706000000
}
```

**WebSocket Event:**
```json
{
  "rollup": "arbitrum",
  "event_type": "BatchDelivered",
  "block_number": 19000000,
  "tx_hash": "0x123...",
  "batch_number": "12345",
  "timestamp": 1706000000
}
```

## Configuration

Create a `.env` file:

```bash
# Ethereum WebSocket RPC endpoint
RPC_WS=wss://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY

# Arbitrum contract addresses (Mainnet)
ARBITRUM_INBOX_ADDRESS=0x1c479675ad559DC151F6Ec7ed3FbF8ceE79582B6
ARBITRUM_ROLLUP_CORE=0x5eF0D09d1E6204141B4d37530808eD19f60FBa35

# Starknet contract address (Mainnet)
STARKNET_CORE_ADDRESS=0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4
```

## Running

### Prerequisites

- Rust 1.75+ (edition 2024)
- Access to an Ethereum WebSocket RPC endpoint

### Build & Run

```bash
# Build
cargo build --release

# Run
cargo run --release
```

The server starts on `http://0.0.0.0:8080`.

## Project Structure

```
rollup-proof-status/
├── src/
│   ├── main.rs              # Entry point, HTTP server, routes
│   ├── types.rs             # Shared types (RollupEvent, RollupStatus)
│   ├── arbitrum/
│   │   └── mod.rs           # Arbitrum event watchers
│   └── starknet/
│       └── mod.rs           # Starknet event watchers
├── abi/
│   ├── arbitrum_sequencer_inbox.json
│   ├── arbitrum_rollup_core.json
│   └── starknet_core_contract.json
├── Cargo.toml
└── .env
```

## Health Monitoring (Future)

The assessment layer will implement health rules:

- **Lag Rule**: Alert if L2 head - L1 committed head > threshold
- **Cadence Rule**: Alert if no new L1 event in X minutes
- **Time Rule**: Alert if last commitment timestamp exceeds allowed window
- **Proof Rule**: Alert if ZK proofs stop verifying

## Supported Rollups

| Rollup   | Status      | Events Tracked                           |
|----------|-------------|------------------------------------------|
| Arbitrum | Implemented | BatchDelivered, AssertionCreated/Confirmed |
| Starknet | Implemented | LogStateUpdate, LogMessageToL2           |
| Optimism | Planned     | OutputProposed, OutputFinalized          |
| zkSync   | Planned     | BlockCommit, BlockVerification           |

## License

MIT
