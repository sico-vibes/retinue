import { useEffect, useRef, useState, type RefObject } from 'react'
import { rpc } from '../tauri'
import type { DaemonEvent } from '../types'

export const FOLLOW_UP_SETTING = 'chat.followUpSuggestions'

export interface FollowUpSuggestion {
  turnId: string
  text: string
}

interface SessionSuggestion extends FollowUpSuggestion {
  sessionId: string
}

function getSuggestion(sessionId: string) {
  return rpc<FollowUpSuggestion | null>('session.suggestion.get', { sessionId })
}

function dismissSuggestion(sessionId: string) {
  return rpc('session.suggestion.dismiss', { sessionId })
}

/** Ephemeral state for the conversation open in this window. */
export function useSessionSuggestion(sessionId: string | undefined, enabled: boolean, eventHandler: RefObject<((event: DaemonEvent) => void) | null>) {
  const [entry, setEntry] = useState<SessionSuggestion | null>(null)
  const revision = useRef(0)

  useEffect(() => {
    setEntry(null)
    if (!enabled || !sessionId) return
    let active = true

    const refresh = () => {
      const requestedAt = ++revision.current
      void getSuggestion(sessionId).then((suggestion) => {
        if (!active || revision.current !== requestedAt) return
        setEntry(typeof suggestion?.turnId === 'string' && typeof suggestion.text === 'string' ? { ...suggestion, sessionId } : null)
      }).catch(() => undefined)
    }

    eventHandler.current = (event) => {
      if (event.type === 'replay.gap') {
        setEntry(null)
        refresh()
        return
      }
      const payload = event.payload
      if (payload?.sessionId !== sessionId) return
      if (event.type === 'session.suggestion' && typeof payload.turnId === 'string' && typeof payload.text === 'string') {
        revision.current += 1
        setEntry({ sessionId, turnId: payload.turnId, text: payload.text })
      } else if (event.type === 'session.suggestion.cleared') {
        revision.current += 1
        setEntry(null)
      }
    }
    refresh()

    return () => { active = false; eventHandler.current = null }
  }, [sessionId, enabled, eventHandler])

  const suggestion = enabled && entry?.sessionId === sessionId ? entry : null
  return {
    suggestion,
    dismiss: () => {
      if (!suggestion) return
      revision.current += 1
      setEntry(null)
      void dismissSuggestion(suggestion.sessionId).catch(() => undefined)
    },
  }
}
