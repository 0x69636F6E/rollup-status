import { useState, useEffect, useRef, useCallback } from 'react'

export function useWebSocket(url) {
  const [events, setEvents] = useState([])
  const [initialData, setInitialData] = useState(null)
  const [status, setStatus] = useState('disconnected')
  const wsRef = useRef(null)
  const reconnectTimeoutRef = useRef(null)
  const reconnectAttempts = useRef(0)
  const maxReconnectAttempts = 10
  const baseDelay = 1000

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      return
    }

    setStatus('connecting')

    // Use absolute URL if provided, otherwise construct from current host
    let wsUrl
    if (url.startsWith('ws://') || url.startsWith('wss://')) {
      wsUrl = url
    } else {
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
      wsUrl = `${protocol}//${window.location.host}${url}`
    }

    const ws = new WebSocket(wsUrl)
    wsRef.current = ws

    ws.onopen = () => {
      setStatus('connected')
      reconnectAttempts.current = 0
    }

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        // Handle initial status message separately from events
        if (data.type === 'initial') {
          setInitialData(data)
        } else if (data.rollup) {
          // Only add actual rollup events to the events array
          setEvents((prev) => [data, ...prev].slice(0, 100))
        }
      } catch (e) {
        console.error('Failed to parse WebSocket message:', e)
      }
    }

    ws.onclose = () => {
      setStatus('disconnected')
      wsRef.current = null

      if (reconnectAttempts.current < maxReconnectAttempts) {
        const delay = baseDelay * Math.pow(2, reconnectAttempts.current)
        reconnectAttempts.current += 1
        setStatus('reconnecting')
        reconnectTimeoutRef.current = setTimeout(connect, delay)
      }
    }

    ws.onerror = (error) => {
      console.error('WebSocket error:', error)
    }
  }, [url])

  const disconnect = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current)
      reconnectTimeoutRef.current = null
    }
    if (wsRef.current) {
      wsRef.current.close()
      wsRef.current = null
    }
    reconnectAttempts.current = maxReconnectAttempts
    setStatus('disconnected')
  }, [])

  const clearEvents = useCallback(() => {
    setEvents([])
  }, [])

  useEffect(() => {
    connect()
    return () => {
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current)
      }
      if (wsRef.current) {
        wsRef.current.close()
      }
    }
  }, [connect])

  return { events, initialData, status, connect, disconnect, clearEvents }
}
