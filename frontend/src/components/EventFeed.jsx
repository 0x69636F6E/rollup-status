import { useState } from 'react'
import { EventItem } from './EventItem'

export function EventFeed({ events, onClear }) {
  const [filter, setFilter] = useState('all')

  const filteredEvents = events.filter((event) => {
    if (filter === 'all') return true
    return event.rollup?.toLowerCase() === filter
  })

  return (
    <div className="bg-bg-secondary border border-border rounded-lg overflow-hidden">
      <div className="p-4 border-b border-border">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <h2 className="text-lg font-semibold text-text-primary">
              Live Events
            </h2>
            <span className="px-2 py-0.5 rounded-full bg-bg-tertiary text-xs text-text-secondary">
              {filteredEvents.length} events
            </span>
          </div>

          <div className="flex items-center gap-2">
            <select
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              className="bg-bg-tertiary border border-border rounded px-2 py-1 text-sm text-text-primary focus:outline-none focus:ring-1 focus:ring-arbitrum"
            >
              <option value="all">All Rollups</option>
              <option value="arbitrum">Arbitrum</option>
              <option value="starknet">Starknet</option>
              <option value="base">Base</option>
            </select>

            <button
              onClick={onClear}
              className="px-3 py-1 text-sm text-text-secondary hover:text-text-primary transition-colors"
            >
              Clear
            </button>
          </div>
        </div>
      </div>

      <div className="p-4 max-h-[600px] overflow-y-auto">
        {filteredEvents.length === 0 ? (
          <div className="text-center py-12">
            <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-bg-tertiary flex items-center justify-center">
              <svg
                className="w-8 h-8 text-text-secondary"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M13 10V3L4 14h7v7l9-11h-7z"
                />
              </svg>
            </div>
            <p className="text-text-secondary">Waiting for events...</p>
            <p className="text-sm text-text-secondary/70 mt-1">
              Events will appear here as they are posted to L1
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {filteredEvents.map((event, index) => (
              <EventItem key={`${event.tx_hash}-${index}`} event={event} />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
