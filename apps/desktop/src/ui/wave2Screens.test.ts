// Wave 2 (17 November 2026): the screen helpers for reported cost, check-backs,
// the bot's own "now" note and the model menu cues. Money is only what a
// provider reported: an unknown amount never reads as $0.
import { describe, expect, it } from 'vitest'
import { formatUsd, parseSpendCheck, runCostText, runReasonText, spendLine } from './automationSchedule'
import { checkBackLabel } from './taskModel'
import { doingNowFor } from './doingNow'
import { modelCue } from '../executionContract'
import { applyEvent, type Snapshot } from '../types'

describe('schedule cost', () => {
  it('projects only from reported runs and never shows $0 for unknown', () => {
    expect(spendLine({ basis: 'recent_runs', projection: '18.4', sampledRuns: 6 })).toBe('About $18.40 a month, from 6 runs')
    expect(spendLine({ basis: 'recent_runs', projection: '0.004', sampledRuns: 1 })).toBe('About <$0.01 a month, from 1 run')
    expect(spendLine({ basis: 'no_priced_runs', projection: null, sampledRuns: 0 })).toBe('No cost reported yet')
    expect(spendLine({ basis: 'one_off', projection: null, sampledRuns: 0 })).toBe('One-time run')
    expect(spendLine({ basis: 'no_upcoming_runs', projection: null, sampledRuns: 3 })).toBeNull()
    expect(spendLine(null)).toBeNull()
  })

  it('shows a run cost only when the provider reported one', () => {
    expect(runCostText({ amount: '0.0015894', unreported: 0 })).toBe('<$0.01')
    expect(runCostText({ amount: '1.2', unreported: 0 })).toBe('$1.20')
    expect(runCostText({ amount: '1.2', unreported: 2 })).toBe('$1.20 reported, some not')
    expect(runCostText({ amount: null, unreported: 3 })).toBeNull()
    expect(runCostText(undefined)).toBeNull()
    expect(formatUsd('-1')).toBeNull()
  })

  it('reads the check-in the owner typed', () => {
    expect(parseSpendCheck('')).toEqual({ ok: true, value: null })
    expect(parseSpendCheck(' $2.50 ')).toEqual({ ok: true, value: '2.50' })
    expect(parseSpendCheck('10000')).toEqual({ ok: true, value: '10000' })
    expect(parseSpendCheck('10000.01')).toEqual({ ok: false })
    expect(parseSpendCheck('2.505')).toEqual({ ok: false })
    expect(parseSpendCheck('two')).toEqual({ ok: false })
  })

  it('names the two new run reasons without calling either a failure', () => {
    expect(runReasonText('unchanged')).toBe('The project had not changed since the last good run.')
    expect(runReasonText('spend_check')).toBe('Stopped at your spend check-in.')
  })
})

describe('task check-back', () => {
  const now = Date.parse('2026-11-17T12:00:00Z')
  const describe = () => 'in 3 h'
  it('shows a future check on an idle task only', () => {
    expect(checkBackLabel({ state: 'done', nextCheckAt: '2026-11-17T15:00:00Z' }, now, describe)).toBe('Checking back in 3 h')
    expect(checkBackLabel({ state: 'running', nextCheckAt: '2026-11-17T15:00:00Z' }, now, describe)).toBeNull()
    expect(checkBackLabel({ state: 'queued', nextCheckAt: '2026-11-17T15:00:00Z' }, now, describe)).toBeNull()
    expect(checkBackLabel({ state: 'done', nextCheckAt: '2026-11-17T11:00:00Z' }, now, describe)).toBeNull()
    expect(checkBackLabel({ state: 'done', nextCheckAt: null }, now, describe)).toBeNull()
  })
})

describe('the bot\'s own now line', () => {
  const snapshot = { sequence: 1, sessions: [{ id: 's1', runtimeId: 'r', state: 'working', tools: [{ id: 't', state: 'running', title: 'Bash', detail: { label: 'Ran npm test' } }] }] } as unknown as Snapshot
  it('is set by session.note, cleared by an empty note, and wins over the tool label', () => {
    expect(doingNowFor(snapshot.sessions![0])).toBe('Running npm test')
    const noted = applyEvent(snapshot, { type: 'session.note', sequence: 2, sessionId: 's1', payload: { sessionId: 's1', turnId: 'u1', text: 'Checking the float handling in the parser' } } as never)
    expect(noted.sessions![0].workingNote).toEqual({ turnId: 'u1', text: 'Checking the float handling in the parser' })
    expect(doingNowFor(noted.sessions![0])).toBe('Checking the float handling in the parser')
    const cleared = applyEvent(noted, { type: 'session.note', sequence: 3, sessionId: 's1', payload: { sessionId: 's1', turnId: 'u1', text: '' } } as never)
    expect(cleared.sessions![0].workingNote).toBeNull()
    expect(doingNowFor(cleared.sessions![0])).toBe('Running npm test')
  })
})

describe('model menu cues', () => {
  const now = Date.parse('2026-11-17T12:00:00Z')
  it('shows a published price, a live pause first, and nothing for an unknown price', () => {
    expect(modelCue({ price: { inputPerMTok: '3.00', outputPerMTok: '15.00', currency: 'USD', source: null, checkedAt: null }, health: null }, now).price).toBe('$3 in · $15 out per million tokens')
    expect(modelCue({ price: { inputPerMTok: '0.25', outputPerMTok: '1.25', currency: 'USD', source: null, checkedAt: null }, health: null }, now).price).toBe('$0.25 in · $1.25 out per million tokens')
    expect(modelCue({ price: null, health: null }, now)).toEqual({ price: null, paused: null })
    expect(modelCue({ price: null, health: { failures: 2, pausedUntil: '2026-11-17T12:30:00Z', reason: null } }, now).paused).toMatch(/^Paused for unattended work until /)
    expect(modelCue({ price: null, health: { failures: 2, pausedUntil: '2026-11-17T11:30:00Z', reason: null } }, now).paused).toBeNull()
  })
})
