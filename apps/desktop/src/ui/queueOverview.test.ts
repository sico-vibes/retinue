import { describe, expect, it } from 'vitest'
import { applyQueueChanged, waitingCount, waitingLabel } from './queueOverview'

describe('waiting queues outside their conversation', () => {
  it('counts only messages that still wait to be sent', () => {
    expect(waitingCount([{ status: 'queued', kind: 'user' }, { status: 'queued' }, { status: 'failed', kind: 'user' }, { status: 'queued', kind: 'retry' }, { status: 'sending', kind: 'user' }])).toBe(2)
    expect(waitingCount(null)).toBe(0)
  })

  it('follows queue.changed: adds, updates the pause and drops an emptied queue', () => {
    let map = applyQueueChanged(new Map(), { sessionId: 's-1', items: [{ status: 'queued', kind: 'user' }, { status: 'queued', kind: 'user' }], paused: true })
    expect(map.get('s-1')).toEqual({ count: 2, paused: true })
    map = applyQueueChanged(map, { sessionId: 's-1', items: [{ status: 'queued', kind: 'user' }], paused: false })
    expect(map.get('s-1')).toEqual({ count: 1, paused: false })
    map = applyQueueChanged(map, { sessionId: 's-1', items: [], paused: false })
    expect(map.has('s-1')).toBe(false)
    expect(applyQueueChanged(map, { items: [{ status: 'queued' }] }).size).toBe(0)
  })

  it('says how many wait and whether the queue is paused', () => {
    expect(waitingLabel({ count: 2, paused: true })).toBe('2 waiting · paused')
    expect(waitingLabel({ count: 1, paused: false })).toBe('1 waiting')
  })
})
