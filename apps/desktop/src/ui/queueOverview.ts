// How many messages wait in each conversation's queue, and whether that queue is
// paused, for the lists that show it outside the conversation (22 November 2026).
// A paused queue in an old conversation was invisible: the owner's messages
// waited for days and nothing said so. One shared store reads
// `session.queue.overview` once and follows `queue.changed`.
import { useSyncExternalStore } from 'react'
import * as transport from '../tauri'

export type QueueWaiting = { count: number; paused: boolean }

let waiting = new Map<string, QueueWaiting>()
let started = false
const listeners = new Set<() => void>()

const publish = (next: Map<string, QueueWaiting>) => { waiting = next; listeners.forEach((listener) => listener()) }

/** Counts the messages that still wait to be sent: queued, sent by a person (not a retry). */
export function waitingCount(items: unknown): number {
  return Array.isArray(items) ? items.filter((item) => (item as { status?: string; kind?: string })?.status === 'queued' && ((item as { kind?: string }).kind ?? 'user') === 'user').length : 0
}

/** Folds one `queue.changed` event into the map. */
export function applyQueueChanged(current: ReadonlyMap<string, QueueWaiting>, payload: { sessionId?: unknown; items?: unknown; paused?: unknown }): Map<string, QueueWaiting> {
  const next = new Map(current)
  if (typeof payload.sessionId !== 'string') return next
  const count = waitingCount(payload.items)
  if (count > 0) next.set(payload.sessionId, { count, paused: payload.paused === true })
  else next.delete(payload.sessionId)
  return next
}

function start() {
  if (started) return
  started = true
  // A test or a preview without these calls simply has no overview.
  try {
    void transport.rpc<{ sessions?: Array<{ sessionId: string; count: number; paused: boolean }> }>('session.queue.overview', {})
      .then((reply) => publish(new Map((reply.sessions ?? []).filter((row) => row.count > 0).map((row) => [row.sessionId, { count: row.count, paused: row.paused === true }]))))
      .catch(() => undefined)
    void transport.listenForDaemonEvents((event) => {
      if (event.type !== 'queue.changed') return
      publish(applyQueueChanged(waiting, (event.payload ?? {}) as { sessionId?: unknown; items?: unknown; paused?: unknown }))
    }).catch(() => undefined)
  } catch { /* no transport */ }
}

function subscribe(listener: () => void) {
  start()
  listeners.add(listener)
  return () => { listeners.delete(listener) }
}

/** The waiting queues, keyed by conversation id. */
export function useQueueWaiting(): ReadonlyMap<string, QueueWaiting> {
  return useSyncExternalStore(subscribe, () => waiting, () => waiting)
}

/** The words for a waiting queue: "2 waiting · paused". */
export function waitingLabel(entry: QueueWaiting): string {
  return `${entry.count} waiting${entry.paused ? ' · paused' : ''}`
}
