import type { DelegationRecord } from '../ui/delegations'
// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { applyEvent } from '../types'

async function openFixture(query = '') {
  vi.resetModules()
  history.replaceState(null, '', `/${query}`)
  vi.useFakeTimers()
  const bridge = await import('./fixtureBridge')
  const diagnostics = await import('./demoDiagnostics')
  return { ...bridge, ...diagnostics }
}

afterEach(() => {
  vi.useRealTimers()
  vi.restoreAllMocks()
  history.replaceState(null, '', '/')
})

describe('live preview demos', () => {
  it('stops a prompted fixture turn before its delayed reply and preserves the stopped snapshot', async () => {
    const fixture = await openFixture('?idle')
    const sessionId = 'session-codex'
    const { turnId } = await fixture.rpc<{ turnId: string }>('session.prompt', { sessionId, text: 'Run the tests.' })
    await expect(fixture.rpc('session.cancel', { sessionId })).resolves.toMatchObject({ cancellationRequested: true })
    await vi.advanceTimersByTimeAsync(5000)
    const session = (await fixture.fetchSnapshot()).sessions?.find((item) => item.id === sessionId)
    expect(session?.state).toBe('cancelled')
    expect(session?.messages?.filter((item) => item.turnId === turnId).map((item) => item.role)).toEqual(['user'])
  }, 20_000)

  it('delivers a live permission, streams the reply after approval and finishes through the reducer', async () => {
    const fixture = await openFixture('?idle&demo-live=approval')
    const sessionId = 'session-codex'
    let snapshot = await fixture.fetchSnapshot()
    const eventTypes: string[] = []
    const unlisten = await fixture.listenForDaemonEvents((event) => {
      eventTypes.push(event.type)
      snapshot = applyEvent(snapshot, event)
    })
    try {
      await vi.advanceTimersByTimeAsync(1400)
      const permission = snapshot.permissions?.find((item) => item.sessionId === sessionId)
      expect(permission?.id).toBe('preview-live-turn-permission')
      await fixture.rpc('permission.reply', { permissionId: permission?.id, choice: 'allow_once' })
      await vi.advanceTimersByTimeAsync(5000)
      const session = snapshot.sessions?.find((item) => item.id === sessionId)
      expect(eventTypes).toEqual(expect.arrayContaining(['permission.requested', 'permission.resolved', 'tool.changed', 'message.delta', 'turn.completed']))
      expect(session?.state).toBe('completed')
      expect(session?.messages?.at(-1)?.content).toBe('The invoice parser tests passed.')
      expect((await fixture.fetchSnapshot()).permissions).toEqual([])
    } finally { unlisten() }
  }, 20_000)

  it('emits the provider code on turn.error', async () => {
    const fixture = await openFixture('?idle&demo-live=error-context_exhausted')
    let snapshot = await fixture.fetchSnapshot()
    const unlisten = await fixture.listenForDaemonEvents((event) => { snapshot = applyEvent(snapshot, event) })
    try {
      await vi.advanceTimersByTimeAsync(2500)
      expect(snapshot.sessions?.find((item) => item.id === 'session-codex')?.turns?.at(-1))
        .toMatchObject({ state: 'error', failureClass: 'context_exhausted' })
    } finally { unlisten() }
  }, 20_000)

  it('publishes service loss and recovery through the native connection and supervisor listeners', async () => {
    const fixture = await openFixture('?demo-live=reconnect')
    const connections: boolean[] = []
    const states: string[] = []
    const stop = [
      await fixture.listenForDaemonConnection((online) => connections.push(online)),
      await fixture.listenForDaemonSupervisor((status) => states.push(status.state)),
      await fixture.listenForDaemonEvents(() => undefined),
    ]
    try {
      await vi.advanceTimersByTimeAsync(1400)
      expect(connections).toEqual([false])
      expect(states).toEqual(['restarting'])
      await vi.advanceTimersByTimeAsync(1100)
      expect(connections).toEqual([false, true])
      expect(states).toEqual(['restarting', 'running'])
    } finally { stop.forEach((unlisten) => unlisten()) }
  }, 20_000)

  it('stops a group of assignments while keeping terminal rows and notifying the UI', async () => {
    const fixture = await openFixture('?scene=team')
    const { delegations } = await fixture.rpc<{ delegations: DelegationRecord[] }>('delegation.list', {
      originSessionId: 'session-scene',
    })
    const running = delegations.find((row) => row.status === 'running')
    const completed = delegations.find((row) => row.status === 'completed')
    expect(running).toBeDefined()
    const events: string[] = []
    const stop = await fixture.listenForDaemonEvents((event) => events.push(event.type))
    try {
      const result = await fixture.rpc<{ delegations: DelegationRecord[] }>('delegation.cancelGroup', {
        originSessionId: running?.originSessionId, delegationIds: [running?.id, completed?.id],
      })
      expect(result.delegations.map((row) => row.status)).toEqual(['cancelled', 'completed'])
      expect(events).toContain('delegation.updated')
    } finally { stop() }
  }, 20_000)

  it('records unsupported RPCs once and retains the empty response', async () => {
    const fixture = await openFixture()
    const warning = vi.spyOn(console, 'warn').mockImplementation(() => undefined)
    await expect(fixture.rpc('demo.unknown')).resolves.toEqual({})
    await expect(fixture.rpc('demo.unknown')).resolves.toEqual({})
    expect(fixture.unknownRpcMethods()).toEqual(['demo.unknown'])
    expect(warning).toHaveBeenCalledExactlyOnceWith(expect.stringContaining('demo.unknown'))
  }, 20_000)
})

