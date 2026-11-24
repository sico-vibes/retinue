import { describe, expect, it } from 'vitest'
import { AUTO_SETTLE_AFTER_MS, isSettled } from './settledStore'
import type { Session } from '../types'

const DAY = 24 * 60 * 60 * 1000
const now = Date.parse('2026-11-24T12:00:00Z')
const session = (over: Partial<Session> = {}): Session => ({ id: 's1', state: 'idle', createdAt: new Date(now - 10 * DAY).toISOString(), lastMessageAt: new Date(now - 8 * DAY).toISOString(), ...over }) as Session

describe('settled conversations', () => {
  it('settle after seven idle days and not before', () => {
    expect(AUTO_SETTLE_AFTER_MS).toBe(7 * DAY)
    expect(isSettled(session(), now, {})).toBe(true)
    expect(isSettled(session({ lastMessageAt: new Date(now - 6 * DAY).toISOString() }), now, {})).toBe(false)
  })
  it('never settle while working, waiting for the owner or stopped with an error', () => {
    for (const state of ['working', 'waiting_permission', 'error', 'failed']) expect(isSettled(session({ state } as Partial<Session>), now, { s1: 'settled' })).toBe(false)
  })
  it('follow the owner: settle by hand, or keep one active', () => {
    expect(isSettled(session({ lastMessageAt: new Date(now - DAY).toISOString() }), now, { s1: 'settled' })).toBe(true)
    expect(isSettled(session(), now, { s1: 'active' })).toBe(false)
  })
})
