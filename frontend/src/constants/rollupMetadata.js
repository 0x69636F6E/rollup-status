// Per-rollup metadata: type, events, L1 contracts, health thresholds
// Thresholds mirror src/health.rs lines 46-77

export const rollupMetadata = {
  arbitrum: {
    type: 'Optimistic Rollup',
    events: ['BatchDelivered', 'ProofSubmitted', 'ProofVerified'],
    contracts: [
      {
        label: 'Sequencer Inbox',
        address: '0x1c479675ad559DC151F6Ec7ed3FbF8ceE79582B6',
      },
      {
        label: 'Rollup Core',
        address: '0x4Dbd4fc535Ac27206064B68FfCf827b0A60BAB3f',
      },
    ],
    thresholds: {
      batchCadenceSecs: 300,     // 5 minutes
      proofCadenceSecs: 3600,    // 1 hour
      delayedSecs: 600,          // 10 minutes
      haltedSecs: 1800,          // 30 minutes
    },
  },
  starknet: {
    type: 'ZK Rollup',
    events: ['StateUpdate'],
    contracts: [
      {
        label: 'Core Contract',
        address: '0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4',
      },
    ],
    thresholds: {
      batchCadenceSecs: 3600,    // 1 hour
      proofCadenceSecs: 7200,    // 2 hours
      delayedSecs: 7200,         // 2 hours
      haltedSecs: 14400,         // 4 hours
    },
  },
  base: {
    type: 'OP Stack Rollup',
    events: ['DisputeGameCreated', 'WithdrawalProven'],
    contracts: [
      {
        label: 'Dispute Game Factory',
        address: '0x43edB88C4B80fDD2AdFF2412A7BebF9dF42cB40e',
      },
      {
        label: 'Optimism Portal',
        address: '0x49048044D57e1C92A77f79988d21Fa8fAF74E97e',
      },
    ],
    thresholds: {
      batchCadenceSecs: 1800,    // 30 minutes
      proofCadenceSecs: 3600,    // 1 hour
      delayedSecs: 3600,         // 1 hour
      haltedSecs: 7200,          // 2 hours
    },
  },
  optimism: {
    type: 'OP Stack Rollup',
    events: ['DisputeGameCreated', 'WithdrawalProven'],
    contracts: [
      {
        label: 'Dispute Game Factory',
        address: '0xe5965Ab5962eDc7477C8520243A95517CD252fA9',
      },
      {
        label: 'Optimism Portal',
        address: '0xbEb5Fc579115071764c7423A4f12eDde41f106Ed',
      },
    ],
    thresholds: {
      batchCadenceSecs: 1800,    // 30 minutes
      proofCadenceSecs: 3600,    // 1 hour
      delayedSecs: 3600,         // 1 hour
      haltedSecs: 7200,          // 2 hours
    },
  },
  zksync: {
    type: 'ZK Rollup',
    events: ['BlockCommit', 'BlocksVerification', 'BlockExecution'],
    contracts: [
      {
        label: 'Diamond Proxy',
        address: '0x32400084C286CF3E17e7B677ea9583e60a000324',
      },
    ],
    thresholds: {
      batchCadenceSecs: 1800,    // 30 minutes
      proofCadenceSecs: 3600,    // 1 hour
      delayedSecs: 7200,         // 2 hours
      haltedSecs: 14400,         // 4 hours
    },
  },
}

// Human-readable duration formatter
export function formatDuration(seconds) {
  if (seconds == null) return '—'
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`
  if (seconds < 86400) {
    const h = Math.floor(seconds / 3600)
    const m = Math.floor((seconds % 3600) / 60)
    return m > 0 ? `${h}h ${m}m` : `${h}h`
  }
  return `${Math.floor(seconds / 86400)}d`
}

// Compact threshold label formatter
export function formatThreshold(seconds) {
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) return `${seconds / 60}min`
  return `${seconds / 3600}hr`
}
