// @vitest-environment happy-dom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Agent, ImportRow, Session } from '../types'

const h = vi.hoisted(() => ({ rpc: vi.fn(), listen: vi.fn() }))
vi.mock('../tauri', () => ({ rpc: h.rpc, listenForDaemonEvents: h.listen, inDesktop: true }))

import { groupByChoice, ImportDialog, outcomeSentence, projectStatus, rowMatches } from './ImportDialog'
import { continuingAgent, ImportedBar } from './ImportedBar'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const agent = (id: string, name: string, runtimeId: string, extra: Partial<Agent> = {}) => ({ id, name, runtimeId, color: 'sky', description: '', instructions: '', model: null, ...extra }) as unknown as Agent
const AGENTS = [agent('ceo', 'Mona', 'rt-claude', { systemRole: 'manager' } as Partial<Agent>), agent('ada', 'Ada', 'rt-codex'), agent('bo', 'Bo', 'rt-claude')]
const PROVIDERS: Record<string, string> = { 'rt-claude': 'claude', 'rt-codex': 'codex' }
const providerOf = (item: Agent) => PROVIDERS[item.runtimeId] ?? null
const row = (id: string, source: 'claude' | 'codex', extra: Partial<ImportRow> = {}): ImportRow => ({ conversationId: id, source, agent: source, agentLabel: source === 'claude' ? 'Claude Code' : 'Codex', title: `Title ${id}`, projectPath: 'C:/work/billing', firstUser: 'Fix the invoice parser', messageCount: 4, bytes: 2000, updatedAt: '2026-11-20T10:00:00Z', canResumeWith: null, alreadyImported: false, importedSessionId: null, error: null, ...extra })

let root: Root
let host: HTMLDivElement
beforeEach(() => { h.rpc.mockReset(); h.listen.mockReset().mockResolvedValue(() => undefined); host = document.createElement('div'); document.body.appendChild(host); root = createRoot(host) })
afterEach(() => { act(() => root.unmount()); host.remove() })
const settle = async () => { await act(async () => { for (let index = 0; index < 6; index += 1) await Promise.resolve() }) }
const button = (text: string) => [...host.querySelectorAll<HTMLButtonElement>('button')].find((node) => node.textContent?.trim() === text)

describe('import helpers', () => {
  it('names a folder honestly and searches titles, first messages and folders', () => {
    expect(projectStatus({ existingProjectId: 'p1', exists: true })).toBe('Already a project')
    expect(projectStatus({ existingProjectId: null, exists: true })).toBe('New project')
    expect(projectStatus({ existingProjectId: null, exists: false })).toBe('Folder missing')
    expect(rowMatches(row('a', 'claude'), 'INVOICE')).toBe(true)
    expect(rowMatches(row('a', 'claude'), 'billing')).toBe(true)
    expect(rowMatches(row('a', 'claude'), 'payroll')).toBe(false)
    expect(rowMatches(row('a', 'claude'), '  ')).toBe(true)
  })

  it('makes one import call per person, with the CEO as the empty choice', () => {
    const groups = groupByChoice([{ source: 'claude', conversationId: 'a' }, { source: 'codex', conversationId: 'b' }, { source: 'claude', conversationId: 'c' }], { 'claude:a': 'ada', 'claude:c': 'ada' })
    expect([...groups.entries()]).toEqual([['ada', [{ source: 'claude', conversationId: 'a' }, { source: 'claude', conversationId: 'c' }]], ['', [{ source: 'codex', conversationId: 'b' }]]])
    expect(outcomeSentence({ imported: 3, skipped: 1, failed: 0, sessions: [], to: { ada: 2, '': 1 } }, (id) => id ? 'Ada' : 'Mona')).toBe('Imported 3 conversations: 2 to Ada, 1 to Mona; 1 already here or skipped.')
  })
})

describe('import dialog', () => {
  it('suggests who gets each conversation and writes nothing until the owner imports', async () => {
    h.rpc.mockImplementation(async (method: string, params: Record<string, unknown>) => {
      if (method === 'import.scan') return { sinceDays: 30, scannedAt: '', sources: [{ source: 'claude', label: 'Claude Code', folder: null, conversations: 3, recent: 2, newest: null, oldest: null, bytes: 1, error: null }] }
      if (method === 'import.projects') return { projects: [{ key: 'c:/work/billing', path: 'C:/work/billing', name: 'billing', exists: true, existingProjectId: null, total: 2, shown: 2, sources: ['claude'], bytes: 4000, newest: null, conversations: [row('a', 'claude'), row('b', 'claude')] }], older: 0, truncated: false, errors: [], sinceDays: 30, scannedAt: '' }
      if (method === 'import.distribute') return { suggestions: (params.items as { conversationId: string }[]).map((item, index) => ({ conversationId: item.conversationId, sessionId: null, title: `Title ${item.conversationId}`, source: 'claude', agentId: index === 0 ? 'bo' : null, reason: 'agent', detail: 'uses Claude Code' })) }
      if (method === 'import.apply') return { importId: 'i', imported: (params.items as unknown[]).length, skipped: 0, failed: 0, sessions: ['s1'] }
      throw new Error(method)
    })
    await act(async () => { root.render(<ImportDialog agents={AGENTS} ceo={AGENTS[0]!} onClose={vi.fn()} />) })
    await settle()
    await act(async () => { button('Next')!.click() })
    await settle()
    expect(host.querySelector('.import-project')?.textContent).toContain('New project')
    await act(async () => { button('Choose all listed')!.click() })
    await act(async () => { button('Next · 2 chosen')!.click() })
    await act(async () => { button('Next')!.click() })
    await settle()
    expect(h.rpc.mock.calls.some(([method]) => method === 'import.apply')).toBe(false)
    expect(h.rpc).toHaveBeenCalledWith('import.distribute', { items: [{ source: 'claude', conversationId: 'a' }, { source: 'claude', conversationId: 'b' }] })
    await act(async () => { button('Import 2')!.click() })
    await settle()
    expect(h.rpc).toHaveBeenCalledWith('import.apply', { items: [{ source: 'claude', conversationId: 'a' }], assign: { agentId: 'bo' }, sinceDays: 30 })
    expect(h.rpc).toHaveBeenCalledWith('import.apply', { items: [{ source: 'claude', conversationId: 'b' }], assign: { mode: 'ceo' }, sinceDays: 30 })
    expect(host.querySelector('.import-done')?.textContent).toBe('Imported 2 conversations: 1 to Bo, 1 to Mona.')
  })
})

describe('imported conversation bar', () => {
  const imported = { id: 's1', runtimeId: 'rt-claude', agentId: 'ada', imported: { source: 'claude', conversationId: 'c1', canResumeWith: null, readOnly: true } } as unknown as Session

  it('offers the employee who speaks the source first, then anyone on that agent', () => {
    expect(continuingAgent(AGENTS, imported, providerOf)?.id).toBe('ceo')
    expect(continuingAgent(AGENTS, { ...imported, agentId: 'bo' }, providerOf)?.id).toBe('bo')
    expect(continuingAgent([AGENTS[1]!], imported, providerOf)).toBeNull()
  })

  it('hands the conversation over through the daemon and reports what it did', async () => {
    const onContinued = vi.fn()
    h.rpc.mockResolvedValue({ mode: 'continue', session: { id: 's1' }, draft: null, importedSessionId: 's1' })
    await act(async () => { root.render(<ImportedBar session={imported} agents={AGENTS} providerOf={providerOf} onContinued={onContinued} />) })
    expect(host.textContent).toContain('Imported from Claude Code. This is read-only history.')
    await act(async () => { button('Continue with Mona')!.click() })
    await settle()
    expect(h.rpc).toHaveBeenCalledWith('import.continue', { sessionId: 's1', agentId: 'ceo' })
    expect(onContinued).toHaveBeenCalledWith(expect.objectContaining({ mode: 'continue' }), AGENTS[0])
  })

  it('has no Continue button when no employee uses the source coding agent', async () => {
    await act(async () => { root.render(<ImportedBar session={imported} agents={[AGENTS[1]!]} providerOf={providerOf} onContinued={vi.fn()} />) })
    expect(host.textContent).not.toContain('Continue with')
    expect(button('Give this to another employee…')).toBeTruthy()
  })
})
