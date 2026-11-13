import { describe, expect, it } from 'vitest'
import type { Session } from '../types'
import { parseScopes } from './ChangesView'
import { conversationStatus } from './ProjectsSidebar'
import { attentionOrder, attentionRank } from './sidebarOrder'

describe('review scopes', () => {
  it('reads the scopes the daemon offers and ignores ones it does not know', () => {
    const scopes = parseScopes({ scopes: [
      { scope: 'uncommitted', available: true, reason: null },
      { scope: 'turn', available: false, reason: 'No changes were recorded yet.', lastTurnId: null },
      { scope: 'turn-ish', available: true },
      { scope: 'branch', available: true, lastTurnId: 7 },
    ] })
    expect(scopes).toEqual([
      { scope: 'uncommitted', available: true, reason: null, lastTurnId: null },
      { scope: 'turn', available: false, reason: 'No changes were recorded yet.', lastTurnId: null },
      { scope: 'branch', available: true, reason: null, lastTurnId: null },
    ])
    // An older daemon that does not know the call offers no switch.
    expect(parseScopes(null)).toEqual([])
  })
})

describe('conversation status and order', () => {
  const session = (partial: Partial<Session>) => ({ id: 's', state: 'idle', ...partial }) as Session

  it('says when a conversation is snoozed, beside what it is doing', () => {
    expect(conversationStatus(session({ state: 'working', snoozed: true }), false)).toEqual({ label: 'Working · snoozed', tone: 'idle' })
    expect(conversationStatus(session({ snoozed: true }), false).label).toBe('Idle · snoozed')
    expect(conversationStatus(session({ state: 'working' }), false)).toEqual({ label: 'Working', tone: 'working' })
    // One that needs the owner is never shown as snoozed: the daemon has woken it.
    expect(conversationStatus(session({ state: 'waiting_permission', snoozed: true }), false).label).toBe('Needs you')
  })

  it('puts what needs the owner first, then what is working, and keeps time order inside each', () => {
    const list = [session({ id: 'idle-new' }), session({ id: 'working', state: 'working' }), session({ id: 'needs', state: 'waiting_permission' }), session({ id: 'idle-old' }), session({ id: 'flagged' })]
    const ordered = attentionOrder(list, (item) => attentionRank(item, item.id === 'flagged'))
    expect(ordered.map((item) => item.id)).toEqual(['needs', 'flagged', 'working', 'idle-new', 'idle-old'])
  })
})
