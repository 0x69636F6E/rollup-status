import { useState } from 'react'
import { StatusBadge } from './StatusBadge'
import { CopyableValue } from './CopyableValue'
import { RollupDetails } from './RollupDetails'
import arbitrumLogo from '../assets/arbitrum.png'
import starknetLogo from '../assets/starknet.png'
import baseLogo from '../assets/base.png'
import optimismLogo from '../assets/optimism.svg'
import zksyncLogo from '../assets/zksync.svg'

const logos = {
  arbitrum: arbitrumLogo,
  starknet: starknetLogo,
  base: baseLogo,
  optimism: optimismLogo,
  zksync: zksyncLogo,
}

export function RollupCard({ rollup, status, loading, health }) {
  const [expanded, setExpanded] = useState(false)

  const accentColors = { arbitrum: 'arbitrum', starknet: 'starknet', base: 'base', optimism: 'optimism', zksync: 'zksync' }
  const accentColor = accentColors[rollup] || 'arbitrum'

  const rollupTypes = {
    arbitrum: 'Optimistic Rollup',
    starknet: 'ZK Rollup',
    base: 'OP Stack Rollup',
    optimism: 'OP Stack Rollup',
    zksync: 'ZK Rollup',
  }

  const getEtherscanTxUrl = (txHash) => {
    if (!txHash || txHash === '—') return null
    return `https://etherscan.io/tx/${txHash}`
  }

  const formatTimestamp = (ts) => {
    if (!ts) return '—'
    // Backend sends Unix timestamp in seconds, JS expects milliseconds
    const date = new Date(typeof ts === 'number' ? ts * 1000 : ts)
    return date.toLocaleTimeString()
  }

  const getHealthStatus = () => {
    if (loading || !status) return 'unknown'
    if (status.error) return 'error'
    if (health?.status) {
      switch (health.status) {
        case 'Delayed': return 'warning'
        case 'Halted':
        case 'Disconnected': return 'error'
        case 'Healthy': return 'healthy'
        default: return 'unknown'
      }
    }
    return 'healthy'
  }

  return (
    <div className="bg-bg-secondary border border-border rounded-lg overflow-hidden">
      <div className={`h-1 bg-${accentColor}`} />
      <div className="p-6">
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-lg overflow-hidden flex items-center justify-center">
              <img
                src={logos[rollup]}
                alt={`${rollup} logo`}
                className="w-10 h-10 object-contain"
              />
            </div>
            <div>
              <h3 className="text-lg font-semibold text-text-primary capitalize">
                {rollup}
              </h3>
              <p className="text-xs text-text-secondary">
                {rollupTypes[rollup] || 'Rollup'}
              </p>
            </div>
          </div>
          <StatusBadge status={getHealthStatus()} />
        </div>

        {loading ? (
          <div className="space-y-3">
            {[1, 2, 3].map((i) => (
              <div key={i} className="animate-pulse">
                <div className="h-4 bg-bg-tertiary rounded w-1/3 mb-1" />
                <div className="h-6 bg-bg-tertiary rounded w-2/3" />
              </div>
            ))}
          </div>
        ) : status?.error ? (
          <div className="text-error text-sm">{status.error}</div>
        ) : (
          <div className="space-y-3">
            <div>
              <p className="text-xs text-text-secondary uppercase tracking-wide">
                Latest Batch
              </p>
              <CopyableValue
                value={status?.latest_batch}
                isHash={String(status?.latest_batch || '').startsWith('0x')}
                etherscanUrl={getEtherscanTxUrl(status?.latest_batch_tx)}
              />
            </div>
            <div>
              <p className="text-xs text-text-secondary uppercase tracking-wide">
                Latest Proof
              </p>
              <CopyableValue
                value={status?.latest_proof}
                isHash={true}
                etherscanUrl={getEtherscanTxUrl(status?.latest_proof_tx)}
              />
            </div>
            <div>
              <p className="text-xs text-text-secondary uppercase tracking-wide">
                Latest Finalized
              </p>
              <CopyableValue
                value={status?.latest_finalized}
                isHash={true}
                etherscanUrl={getEtherscanTxUrl(status?.latest_finalized_tx)}
              />
            </div>
            <div className="pt-2 border-t border-border">
              <p className="text-xs text-text-secondary">
                Last updated: {formatTimestamp(status?.last_updated)}
              </p>
            </div>

            <button
              onClick={() => setExpanded((v) => !v)}
              className="flex items-center gap-1 text-xs text-text-secondary hover:text-text-primary transition-colors mt-1"
            >
              <svg
                className={`w-3.5 h-3.5 transition-transform ${expanded ? 'rotate-180' : ''}`}
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
              </svg>
              Details
            </button>

            {expanded && (
              <div className="border-t border-border mt-2">
                <RollupDetails rollup={rollup} health={health} />
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
