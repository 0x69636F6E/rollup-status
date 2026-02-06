import { useState } from 'react'
import { StatusBadge } from './StatusBadge'
import arbitrumLogo from '../assets/arbitrum.png'
import starknetLogo from '../assets/starknet.png'
import baseLogo from '../assets/base.png'

const logos = {
  arbitrum: arbitrumLogo,
  starknet: starknetLogo,
  base: baseLogo,
}

// Copyable value component with Etherscan link for hashes
function CopyableValue({ value, isHash = false, etherscanUrl = null }) {
  const [copied, setCopied] = useState(false)

  if (!value || value === '—') {
    return <span className="text-lg font-mono text-text-primary">—</span>
  }

  const handleCopy = async (e) => {
    e.preventDefault()
    e.stopPropagation()
    await navigator.clipboard.writeText(value)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const stringValue = String(value)
  const displayValue = stringValue.length > 16
    ? `${stringValue.slice(0, 8)}...${stringValue.slice(-6)}`
    : typeof value === 'number' ? value.toLocaleString() : value

  const content = (
    <span
      onClick={handleCopy}
      className="text-lg font-mono text-text-primary cursor-pointer hover:text-arbitrum transition-colors inline-flex items-center gap-2 group"
      title={copied ? 'Copied!' : `Click to copy: ${value}`}
    >
      {displayValue}
      <svg
        className={`w-4 h-4 opacity-0 group-hover:opacity-100 transition-opacity ${copied ? 'text-success' : 'text-text-secondary'}`}
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        {copied ? (
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
        ) : (
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
        )}
      </svg>
    </span>
  )

  if (etherscanUrl) {
    return (
      <div className="flex items-center gap-2">
        {content}
        <a
          href={etherscanUrl}
          target="_blank"
          rel="noopener noreferrer"
          className="text-text-secondary hover:text-arbitrum transition-colors"
          title="View on Etherscan"
          onClick={(e) => e.stopPropagation()}
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
          </svg>
        </a>
      </div>
    )
  }

  return content
}

export function RollupCard({ rollup, status, loading }) {
  const isArbitrum = rollup === 'arbitrum'
  const isStarknet = rollup === 'starknet'
  const isBase = rollup === 'base'
  const accentColor = isArbitrum ? 'arbitrum' : isBase ? 'base' : 'starknet'

  const getEtherscanUrl = (hash) => {
    if (!hash || hash === '—') return null
    return `https://etherscan.io/search?q=${hash}`
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
                {isArbitrum ? 'Optimistic Rollup' : isBase ? 'OP Stack Rollup' : 'ZK Rollup'}
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
                etherscanUrl={getEtherscanUrl(status?.latest_batch)}
              />
            </div>
            <div>
              <p className="text-xs text-text-secondary uppercase tracking-wide">
                Latest Proof
              </p>
              <CopyableValue
                value={status?.latest_proof}
                isHash={true}
                etherscanUrl={getEtherscanUrl(status?.latest_proof)}
              />
            </div>
            <div>
              <p className="text-xs text-text-secondary uppercase tracking-wide">
                Latest Finalized
              </p>
              <CopyableValue
                value={status?.latest_finalized}
                isHash={true}
                etherscanUrl={getEtherscanUrl(status?.latest_finalized)}
              />
            </div>
            <div className="pt-2 border-t border-border">
              <p className="text-xs text-text-secondary">
                Last updated: {formatTimestamp(status?.last_updated)}
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
