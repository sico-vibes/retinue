// Settled conversations (owner, 24 November 2026, after T3 Code): a conversation that has been idle for a week, or one
// the owner settles by hand, moves into a collapsed "Settled" section so an employee's list stays short. It never
// settles while it is working or waiting for the owner. "Keep active" stops the automatic rule for one conversation.
// The choice is kept on this computer (like which sidebar groups are folded); the daemon does not know about it.
import { useSyncExternalStore } from 'react'
import type { Session } from '../types'
import { activityAt, sessionIsBusy } from './teamSelectors'

/** T3 Code settles after seven days without activity; so does Retinue. */
export const AUTO_SETTLE_AFTER_MS = 7 * 24 * 60 * 60 * 1000
export type SettledChoice = 'settled' | 'active'
const KEY = 'retinue.conversations.settled'

let cache: Record<string, SettledChoice> | null = null
const listeners = new Set<() => void>()
function read(): Record<string, SettledChoice> {
  if (cache) return cache
  try {
    const raw = JSON.parse(window.localStorage.getItem(KEY) ?? '{}') as Record<string, unknown>
    cache = Object.fromEntries(Object.entries(raw).filter(([, value]) => value === 'settled' || value === 'active')) as Record<string, SettledChoice>
  } catch { cache = {} }
  return cache
}
export function setSettledChoice(id: string, choice: SettledChoice | null) {
  const next = { ...read() }
  if (choice) next[id] = choice
  else delete next[id]
  cache = next
  try { window.localStorage.setItem(KEY, JSON.stringify(next)) } catch { /* the choice lasts until the window closes */ }
  for (const listener of listeners) listener()
}
export function useSettledChoices(): Readonly<Record<string, SettledChoice>> {
  return useSyncExternalStore((listener) => { listeners.add(listener); return () => { listeners.delete(listener) } }, read, read)
}
/** Test hook: forget what was read, so the next read comes from storage again. */
export function resetSettledChoices() { cache = null }

/**
 * Whether a conversation sits in the Settled section. Work in progress, a question for the owner and a stop with an
 * error are never settled, whatever was chosen: they have to stay in sight.
 */
export function isSettled(session: Session, now: number, choices: Readonly<Record<string, SettledChoice>>): boolean {
  if (sessionIsBusy(session)) return false
  const state = String(session.state ?? '').toLowerCase()
  if (state === 'waiting_permission' || state === 'error' || state === 'failed') return false
  const choice = choices[session.id]
  if (choice === 'settled') return true
  if (choice === 'active') return false
  const at = Date.parse(activityAt(session))
  return Number.isFinite(at) && now - at > AUTO_SETTLE_AFTER_MS
}
