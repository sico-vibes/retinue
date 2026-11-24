// @vitest-environment happy-dom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { describe, expect, it, vi } from 'vitest'
import type { Agent, Project, Runtime, Session } from '../types'
import { ProfessionalSidebar, agentForProvider } from './ProfessionalSidebar'

const agent = { id: 'codex', name: 'Codex', runtimeId: 'rt-codex', sortOrder: 0, description: '', color: 'blue' } as Agent
const board = { id: 'board', name: 'Board', path: 'C:\\work\\Board' } as Project
const runtime = { id: 'rt-codex', provider: 'codex' } as Runtime
const ago = (hours: number) => new Date(Date.now() - hours * 3_600_000).toISOString()
const chat = (id: string, hours: number, extra: Partial<Session> = {}) => ({ id, agentId: 'codex', runtimeId: 'rt-codex', title: id, state: 'idle', createdAt: ago(hours), updatedAt: ago(hours), lastMessageAt: ago(hours), messages: [{ id: `${id}-m`, role: 'user', content: 'hi' }], ...extra }) as unknown as Session

function mount(sessions: Session[], onOpen = vi.fn(), approvals = new Set<string>()) {
  const host = document.createElement('div'); document.body.append(host)
  const root = createRoot(host)
  act(() => root.render(<ProfessionalSidebar agents={[agent]} projects={[board]} sessions={sessions} runtimes={[runtime]} selectedSessionId={null} approvalSessionIds={approvals} canCreate onOpenConversation={onOpen} onStartInProject={vi.fn()} onCreateProject={vi.fn()} />))
  return { host, done: () => { act(() => root.unmount()); host.remove() } }
}

describe('Professional mode sidebar', () => {
  it('groups chats by project (by id or by folder), with the rest under No project', () => {
    const { host, done } = mount([chat('a', 2, { projectId: 'board' } as Partial<Session>), chat('b', 3, { projectPath: 'c:/work/board/' } as Partial<Session>), chat('c', 4)])
    try {
      expect([...host.querySelectorAll('.pro-group-name')].map((node) => node.textContent)).toEqual(['Board', 'No project'])
      expect([...host.querySelectorAll('.pro-group:first-of-type .pro-row-title')].map((node) => node.textContent)).toEqual(['a', 'b'])
    } finally { done() }
  })
  it('puts a chat that needs you first and shows its provider and status', () => {
    const { host, done } = mount([chat('old', 5), chat('new', 1), chat('ask', 9)], vi.fn(), new Set(['ask']))
    try {
      expect([...host.querySelectorAll('.pro-row-title')].map((node) => node.textContent)).toEqual(['ask', 'new', 'old'])
      expect(host.querySelector('.pro-row .ps-state.needs')).not.toBeNull()
    } finally { done() }
  })
  it('files a chat idle for over a week under Settled, and opens a row with its employee underneath', () => {
    const onOpen = vi.fn()
    const { host, done } = mount([chat('fresh', 1), chat('stale', 24 * 9)], onOpen)
    try {
      expect(host.querySelector('.pro-settled-head')?.textContent).toBe('Settled1')
      act(() => host.querySelector<HTMLButtonElement>('.pro-list:not(.pro-list-settled) .pro-row')!.click())
      expect(onOpen).toHaveBeenCalledWith(agent, expect.objectContaining({ id: 'fresh' }))
    } finally { done() }
  })
  it('sends a new chat to the provider\'s own employee', () => {
    expect(agentForProvider([{ ...agent, archived: true }, { ...agent, id: 'second', sortOrder: 5 }, agent], 'rt-codex')?.id).toBe('codex')
    expect(agentForProvider([agent], 'rt-none')).toBeUndefined()
  })
})
