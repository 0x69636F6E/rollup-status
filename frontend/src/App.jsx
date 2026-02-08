import { useState, useEffect } from 'react'
import { Header } from './components/Header'
import { RollupCard } from './components/RollupCard'
import { EventFeed } from './components/EventFeed'
import { useWebSocket } from './hooks/useWebSocket'
import { config } from './config'

function App() {
  const [arbitrumStatus, setArbitrumStatus] = useState(null)
  const [starknetStatus, setStarknetStatus] = useState(null)
  const [baseStatus, setBaseStatus] = useState(null)
  const [optimismStatus, setOptimismStatus] = useState(null)
  const [zksyncStatus, setZksyncStatus] = useState(null)
  const [healthData, setHealthData] = useState({})
  const [sequencerData, setSequencerData] = useState({})
  const [loading, setLoading] = useState(true)

  const wsEndpoint = config.wsUrl ? `${config.wsUrl}/rollups/stream` : '/rollups/stream'
  const { events, initialData, status: wsStatus, clearEvents } = useWebSocket(wsEndpoint)

  useEffect(() => {
    const fetchStatus = async () => {
      try {
        const [arbRes, starkRes, baseRes, opRes, zkRes, healthRes, seqRes] = await Promise.allSettled([
          fetch(`${config.apiUrl}/rollups/arbitrum/status`),
          fetch(`${config.apiUrl}/rollups/starknet/status`),
          fetch(`${config.apiUrl}/rollups/base/status`),
          fetch(`${config.apiUrl}/rollups/optimism/status`),
          fetch(`${config.apiUrl}/rollups/zksync/status`),
          fetch(`${config.apiUrl}/rollups/health`),
          fetch(`${config.apiUrl}/rollups/sequencer`),
        ])

        if (arbRes.status === 'fulfilled' && arbRes.value.ok) {
          const data = await arbRes.value.json()
          setArbitrumStatus(data)
        } else {
          setArbitrumStatus({ error: 'Failed to fetch status' })
        }

        if (starkRes.status === 'fulfilled' && starkRes.value.ok) {
          const data = await starkRes.value.json()
          setStarknetStatus(data)
        } else {
          setStarknetStatus({ error: 'Failed to fetch status' })
        }

        if (baseRes.status === 'fulfilled' && baseRes.value.ok) {
          const data = await baseRes.value.json()
          setBaseStatus(data)
        } else {
          setBaseStatus({ error: 'Failed to fetch status' })
        }

        if (opRes.status === 'fulfilled' && opRes.value.ok) {
          const data = await opRes.value.json()
          setOptimismStatus(data)
        } else {
          setOptimismStatus({ error: 'Failed to fetch status' })
        }

        if (zkRes.status === 'fulfilled' && zkRes.value.ok) {
          const data = await zkRes.value.json()
          setZksyncStatus(data)
        } else {
          setZksyncStatus({ error: 'Failed to fetch status' })
        }

        if (healthRes.status === 'fulfilled' && healthRes.value.ok) {
          const data = await healthRes.value.json()
          const byRollup = {}
          for (const entry of data.rollups || []) {
            byRollup[entry.rollup] = entry
          }
          setHealthData(byRollup)
        }

        if (seqRes.status === 'fulfilled' && seqRes.value.ok) {
          const data = await seqRes.value.json()
          setSequencerData(data.sequencer || {})
        }
      } catch (err) {
        console.error('Failed to fetch rollup status:', err)
        setArbitrumStatus({ error: 'Connection failed' })
        setStarknetStatus({ error: 'Connection failed' })
        setBaseStatus({ error: 'Connection failed' })
        setOptimismStatus({ error: 'Connection failed' })
        setZksyncStatus({ error: 'Connection failed' })
      } finally {
        setLoading(false)
      }
    }

    fetchStatus()
    const interval = setInterval(fetchStatus, 30000)
    return () => clearInterval(interval)
  }, [])

  // Handle WebSocket initial payload (includes sequencer data)
  useEffect(() => {
    if (!initialData) return
    if (initialData.sequencer) {
      setSequencerData(initialData.sequencer)
    }
  }, [initialData])

  // Update status from WebSocket events
  useEffect(() => {
    if (events.length === 0) return

    const latestEvent = events[0]
    if (!latestEvent?.rollup) return

    const rollup = latestEvent.rollup.toLowerCase()
    const eventType = latestEvent.event_type

    const updateStatus = (prev) => {
      const updated = {
        ...prev,
        last_updated: latestEvent.timestamp || Date.now() / 1000,
      }

      // Update the appropriate field based on event type
      if (eventType === 'BatchDelivered') {
        updated.latest_batch = latestEvent.batch_number
        updated.latest_batch_tx = latestEvent.tx_hash
      } else if (eventType === 'ProofSubmitted') {
        updated.latest_proof = latestEvent.batch_number
        updated.latest_proof_tx = latestEvent.tx_hash
      } else if (eventType === 'ProofVerified') {
        updated.latest_finalized = latestEvent.batch_number
        updated.latest_finalized_tx = latestEvent.tx_hash
      } else if (eventType === 'StateUpdate') {
        // Starknet state updates include all three
        updated.latest_batch = latestEvent.batch_number
        updated.latest_batch_tx = latestEvent.tx_hash
        updated.latest_proof = latestEvent.batch_number
        updated.latest_proof_tx = latestEvent.tx_hash
        updated.latest_finalized = latestEvent.batch_number
        updated.latest_finalized_tx = latestEvent.tx_hash
      } else if (eventType === 'DisputeGameCreated') {
        // OP Stack dispute games update batch and proof
        updated.latest_batch = latestEvent.batch_number
        updated.latest_batch_tx = latestEvent.tx_hash
        updated.latest_proof = latestEvent.batch_number
        updated.latest_proof_tx = latestEvent.tx_hash
      } else if (eventType === 'WithdrawalProven') {
        updated.latest_finalized = latestEvent.batch_number
        updated.latest_finalized_tx = latestEvent.tx_hash
      } else if (eventType === 'BlockCommit') {
        updated.latest_batch = latestEvent.batch_number
        updated.latest_batch_tx = latestEvent.tx_hash
      } else if (eventType === 'BlocksVerification') {
        updated.latest_proof = latestEvent.batch_number
        updated.latest_proof_tx = latestEvent.tx_hash
      } else if (eventType === 'BlockExecution') {
        updated.latest_finalized = latestEvent.batch_number
        updated.latest_finalized_tx = latestEvent.tx_hash
      }

      return updated
    }

    if (rollup === 'arbitrum') {
      setArbitrumStatus(updateStatus)
    } else if (rollup === 'starknet') {
      setStarknetStatus(updateStatus)
    } else if (rollup === 'base') {
      setBaseStatus(updateStatus)
    } else if (rollup === 'optimism') {
      setOptimismStatus(updateStatus)
    } else if (rollup === 'zksync') {
      setZksyncStatus(updateStatus)
    }
  }, [events])

  return (
    <div className="min-h-screen bg-bg-primary">
      <Header connectionStatus={wsStatus} />

      <main className="max-w-7xl mx-auto px-4 py-8 sm:px-6 lg:px-8">
        <div className="grid gap-6 lg:grid-cols-3">
          <div className="lg:col-span-1 space-y-6">
            <RollupCard
              rollup="arbitrum"
              status={arbitrumStatus}
              loading={loading}
              health={healthData.arbitrum}
              sequencer={sequencerData.arbitrum}
            />
            <RollupCard
              rollup="starknet"
              status={starknetStatus}
              loading={loading}
              health={healthData.starknet}
            />
            <RollupCard
              rollup="base"
              status={baseStatus}
              loading={loading}
              health={healthData.base}
              sequencer={sequencerData.base}
            />
            <RollupCard
              rollup="optimism"
              status={optimismStatus}
              loading={loading}
              health={healthData.optimism}
            />
            <RollupCard
              rollup="zksync"
              status={zksyncStatus}
              loading={loading}
              health={healthData.zksync}
            />
          </div>

          <div className="lg:col-span-2">
            <EventFeed events={events} onClear={clearEvents} />
          </div>
        </div>
      </main>

      <footer className="border-t border-border mt-auto">
        <div className="max-w-7xl mx-auto px-4 py-4 sm:px-6 lg:px-8 flex items-center justify-center gap-3">
          <p className="text-sm text-text-secondary">
            built with ❤️ by damola
          </p>
          <a
            href="https://github.com/iconthegreat"
            target="_blank"
            rel="noopener noreferrer"
            className="text-text-secondary hover:text-text-primary transition-colors"
            title="GitHub"
          >
            <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" />
            </svg>
          </a>
        </div>
      </footer>
    </div>
  )
}

export default App
