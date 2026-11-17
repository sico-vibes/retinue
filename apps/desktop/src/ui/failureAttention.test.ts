import { describe, expect, it } from 'vitest'
import type { Agent, Session } from '../types'
import { needsYouItems } from './NeedsYou'
import { unseenFailures } from './teamSelectors'

// Owner report, 17 November 2026: a failed Ada lane showed no error on Ada's row and nothing in the bell,
// because a newer conversation of hers had finished and the bell only listed requests.
const ada: Agent = { id: 'ada', name: 'Ada', description: '', instructions: '', color: 'mint', runtimeId: 'rt', model: null, thinking: null, serviceTier: null, customArgs: [], customEnv: {}, maxConcurrency: 1, defaultProject: null, sortOrder: 0, archived: false, createdAt: '2026-11-16T00:00:00Z', updatedAt: '2026-11-16T00:00:00Z' }
const session = (id: string, extra: Partial<Session> = {}): Session => ({ id, runtimeId: 'rt', agentId: 'ada', title: `Lane ${id}`, updatedAt: '2026-11-17T00:16:00Z', ...extra })

const failedOld = session('a', { state: 'error', lastMessageAt: '2026-11-17T00:16:00Z' })
const newerDone = session('b', { state: 'completed', lastMessageAt: '2026-11-17T01:54:00Z', updatedAt: '2026-11-17T01:54:00Z' })
const failedSeen = session('c', { state: 'failed' })
const failedArchived = session('d', { state: 'error', archived: true })
const sessions = [failedOld, newerDone, failedSeen, failedArchived]
const unread = new Set(['a', 'b', 'd'])

describe('unopened failures', () => {
  it('finds a failure behind a newer conversation, and drops seen or archived ones', () => {
    expect(unseenFailures(sessions, 'ada', unread).map((item) => item.id)).toEqual(['a'])
    expect(unseenFailures(sessions, 'grey', unread)).toEqual([])
  })

  it('lists an unopened failure in the bell until it is opened', () => {
    const items = needsYouItems([], sessions, [ada], Date.parse('2026-11-17T08:00:00Z'), unread)
    expect(items).toHaveLength(1)
    expect(items[0]).toMatchObject({ id: 'failure:a', kind: 'failure', sessionId: 'a', summary: 'Lane a', permission: null })
    expect(items[0]!.agent?.name).toBe('Ada')
    expect(needsYouItems([], sessions, [ada], Date.now(), new Set())).toEqual([])
    expect(needsYouItems([], sessions, [ada], Date.now())).toEqual([])
  })
})
