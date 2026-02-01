import { StatusBadge } from './StatusBadge'

export function RollupCard({ rollup, status, loading }) {
  const isArbitrum = rollup === 'arbitrum'
  const isStarknet = rollup === 'starknet'
  const isBase = rollup === 'base'
  const accentColor = isArbitrum ? 'arbitrum' : isBase ? 'base' : 'starknet'

  const formatBlockNumber = (num) => {
    if (num === null || num === undefined) return '—'
    return num.toLocaleString()
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
            <div
              className={`w-10 h-10 rounded-lg bg-${accentColor}/20 flex items-center justify-center`}
            >
              {isArbitrum ? (
                <svg className={`w-6 h-6 text-${accentColor}`} viewBox="0 0 24 24" fill="currentColor">
                  <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
                </svg>
              ) : isBase ? (
                <svg className={`w-6 h-6 text-${accentColor}`} viewBox="0 0 24 24" fill="currentColor">
                  <circle cx="12" cy="12" r="10" />
                </svg>
              ) : (
                <svg className={`w-6 h-6 text-${accentColor}`} viewBox="0 0 24 24" fill="currentColor">
                  <polygon points="12,2 22,20 2,20" />
                </svg>
              )}
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
              <p className="text-lg font-mono text-text-primary">
                {formatBlockNumber(status?.latest_batch)}
              </p>
            </div>
            <div>
              <p className="text-xs text-text-secondary uppercase tracking-wide">
                Latest Proof
              </p>
              <p className="text-lg font-mono text-text-primary">
                {formatBlockNumber(status?.latest_proof)}
              </p>
            </div>
            <div>
              <p className="text-xs text-text-secondary uppercase tracking-wide">
                Latest Finalized
              </p>
              <p className="text-lg font-mono text-text-primary">
                {formatBlockNumber(status?.latest_finalized)}
              </p>
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
