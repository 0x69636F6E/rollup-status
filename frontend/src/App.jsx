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
  const [loading, setLoading] = useState(true)

  const wsEndpoint = config.wsUrl ? `${config.wsUrl}/rollups/stream` : '/rollups/stream'
  const { events, status: wsStatus, clearEvents } = useWebSocket(wsEndpoint)

  useEffect(() => {
    const fetchStatus = async () => {
      try {
        const [arbRes, starkRes, baseRes] = await Promise.allSettled([
          fetch(`${config.apiUrl}/rollups/arbitrum/status`),
          fetch(`${config.apiUrl}/rollups/starknet/status`),
          fetch(`${config.apiUrl}/rollups/base/status`),
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
      } catch (err) {
        console.error('Failed to fetch rollup status:', err)
        setArbitrumStatus({ error: 'Connection failed' })
        setStarknetStatus({ error: 'Connection failed' })
        setBaseStatus({ error: 'Connection failed' })
      } finally {
        setLoading(false)
      }
    }

    fetchStatus()
    const interval = setInterval(fetchStatus, 30000)
    return () => clearInterval(interval)
  }, [])

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
        // Base dispute games update batch and proof
        updated.latest_batch = latestEvent.batch_number
        updated.latest_batch_tx = latestEvent.tx_hash
        updated.latest_proof = latestEvent.batch_number
        updated.latest_proof_tx = latestEvent.tx_hash
      } else if (eventType === 'WithdrawalProven') {
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
            />
            <RollupCard
              rollup="starknet"
              status={starknetStatus}
              loading={loading}
            />
            <RollupCard
              rollup="base"
              status={baseStatus}
              loading={loading}
            />
          </div>

          <div className="lg:col-span-2">
            <EventFeed events={events} onClear={clearEvents} />
          </div>
        </div>
      </main>

      <footer className="border-t border-border mt-auto">
        <div className="max-w-7xl mx-auto px-4 py-4 sm:px-6 lg:px-8">
          <p className="text-sm text-text-secondary text-center">
            Rollup Proof Status Monitor — Tracking L2 → L1 events on Ethereum
          </p>
        </div>
      </footer>
    </div>
  )
}

export default App
