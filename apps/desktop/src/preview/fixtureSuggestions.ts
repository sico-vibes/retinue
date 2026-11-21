import type { DaemonEvent, Session } from '../types'
import type { FollowUpSuggestion } from '../ui/sessionSuggestions'

type Emit = (type: string, payload: Record<string, unknown>) => void

const suggestions = new Map<string, FollowUpSuggestion>()
const turns = new Map<string, string>()
let listening = false

export function fixtureSuggestionRpc(
  method: string,
  params: Record<string, unknown>,
  settings: Record<string, unknown>,
  listeners: Set<(event: DaemonEvent) => void>,
  emit: Emit,
) {
  if (typeof params.sessionId !== 'string') return
  const sessionId = params.sessionId

  const clear = (id: string) => {
    if (suggestions.delete(id)) emit('session.suggestion.cleared', { sessionId: id })
  }

  if (method === 'session.suggestion.get') return settings['chat.followUpSuggestions'] !== false ? suggestions.get(sessionId) ?? null : null
  if (method === 'session.suggestion.dismiss') {
    clear(sessionId)
    return { ok: true }
  }

  if (method !== 'session.prompt' || listening) return
  listening = true
  listeners.add((event) => {
    if (event.type !== 'session.changed') return
    const session = event.payload?.session as Session | undefined
    const message = session?.messages?.at(-1)
    if (!session || typeof message?.turnId !== 'string' || typeof message.id !== 'string') return
    const { id } = session
    const { turnId } = message

    // The existing echo fixture publishes the user message and reply separately.
    if (message.id.startsWith('preview-user-') && turns.get(id) !== turnId) {
      turns.set(id, turnId)
      queueMicrotask(() => clear(id))
    } else if (message.id.startsWith('preview-reply-') && turns.get(id) === turnId) {
      turns.delete(id)
      if (settings['chat.followUpSuggestions'] === false || session.modelLock?.planMode === true) return
      const suggestion = { turnId, text: 'What should we check next?' }
      queueMicrotask(() => {
        if (turns.has(id)) return
        suggestions.set(id, suggestion)
        emit('session.suggestion', { sessionId: id, ...suggestion })
      })
    }
  })
}
