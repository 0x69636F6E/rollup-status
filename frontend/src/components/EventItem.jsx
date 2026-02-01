export function EventItem({ event }) {
  const rollup = event.rollup?.toLowerCase()
  const accentColor = rollup === 'arbitrum' ? 'arbitrum' : rollup === 'base' ? 'base' : 'starknet'

  const getEventTypeConfig = (type) => {
    switch (type?.toLowerCase()) {
      case 'batch':
      case 'batchdelivered':
      case 'sequencerbatchdelivered':
        return { label: 'Batch', bg: 'bg-blue-500/20', text: 'text-blue-400' }
      case 'proof':
      case 'proofsubmitted':
      case 'sendroot':
      case 'disputegamecreated':
        return { label: 'Proof', bg: 'bg-purple-500/20', text: 'text-purple-400' }
      case 'finalized':
      case 'proofverified':
      case 'logstateupdate':
      case 'stateupdate':
      case 'withdrawalproven':
        return { label: 'Finalized', bg: 'bg-green-500/20', text: 'text-green-400' }
      case 'messagelog':
        return { label: 'Message', bg: 'bg-yellow-500/20', text: 'text-yellow-400' }
      default:
        return { label: type || 'Event', bg: 'bg-gray-500/20', text: 'text-gray-400' }
    }
  }

  const formatTxHash = (hash) => {
    if (!hash) return '—'
    return `${hash.slice(0, 10)}...${hash.slice(-8)}`
  }

  const formatRelativeTime = (timestamp) => {
    if (!timestamp) return 'just now'
    const now = Date.now()
    // Backend sends Unix timestamp in seconds, JS expects milliseconds
    const ts = typeof timestamp === 'number' ? timestamp * 1000 : new Date(timestamp).getTime()
    const diff = Math.floor((now - ts) / 1000)

    if (diff < 5) return 'just now'
    if (diff < 60) return `${diff}s ago`
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
    return `${Math.floor(diff / 86400)}d ago`
  }

  const eventType = getEventTypeConfig(event.event_type)
  const etherscanUrl = event.tx_hash
    ? `https://etherscan.io/tx/${event.tx_hash}`
    : null

  return (
    <div className="flex items-center gap-4 p-3 rounded-lg bg-bg-tertiary/50 hover:bg-bg-tertiary transition-colors">
      <div className={`w-1 h-12 rounded-full bg-${accentColor}`} />

      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-1">
          <span className={`text-sm font-medium text-${accentColor} capitalize`}>
            {event.rollup || 'Unknown'}
          </span>
          <span
            className={`px-2 py-0.5 rounded text-xs font-medium ${eventType.bg} ${eventType.text}`}
          >
            {eventType.label}
          </span>
        </div>

        <div className="flex items-center gap-4 text-sm">
          {event.block_number && (
            <span className="text-text-secondary">
              Block{' '}
              <span className="font-mono text-text-primary">
                {event.block_number.toLocaleString()}
              </span>
            </span>
          )}

          {event.tx_hash && etherscanUrl && (
            <a
              href={etherscanUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="font-mono text-arbitrum hover:underline"
            >
              {formatTxHash(event.tx_hash)}
            </a>
          )}
        </div>
      </div>

      <div className="text-xs text-text-secondary whitespace-nowrap">
        {formatRelativeTime(event.timestamp)}
      </div>
    </div>
  )
}
