// @vitest-environment happy-dom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { describe, expect, it, vi } from 'vitest'
import type { Agent, Project, Session } from '../types'
import { ProjectsSidebar } from './ProjectsSidebar'

const agent = { id: 'codex', name: 'Codex', description: '', color: 'blue', role: 'CTO' } as Agent
const project = { id: 'board', name: 'Board', path: 'C:/board' } as Project
const session = (id: string, at: string, state = 'idle') => ({ id, agentId: 'codex', projectId: 'board', projectPath: 'C:/board', title: id, updatedAt: at, state }) as unknown as Session

describe('ProjectsSidebar', () => {
  it('folds an employee\'s conversations even while one of them is selected', async () => {
    const host = document.createElement('div'); document.body.append(host)
    const root = createRoot(host)
    const sessions = [session('first', '2026-11-08T11:43:00Z'), session('second', '2026-11-08T11:42:00Z', 'working')]
    try {
      await act(async () => root.render(<ProjectsSidebar agents={[agent]} projects={[project]} sessions={sessions} selectedSessionId="second" canCreate onOpenConversation={vi.fn()} onStartInProject={vi.fn()} onCreateProject={vi.fn()} />))
      const toggle = () => host.querySelector<HTMLButtonElement>('.ps-more')!
      expect(toggle().getAttribute('aria-expanded')).toBe('true')
      await act(async () => toggle().click())
      expect(toggle().getAttribute('aria-expanded')).toBe('false')
      await act(async () => toggle().click())
      expect(toggle().getAttribute('aria-expanded')).toBe('true')
      expect(host.querySelector('.ps-convo .ps-shimmer')?.textContent).toBe('Working')
    } finally { act(() => root.unmount()); host.remove() }
  })

  it('marks Needs you with an icon', async () => {
    const host = document.createElement('div'); document.body.append(host)
    const root = createRoot(host)
    try {
      await act(async () => root.render(<ProjectsSidebar agents={[agent]} projects={[project]} sessions={[session('only', '2026-11-08T11:43:00Z')]} approvalSessionIds={new Set(['only'])} selectedSessionId="only" canCreate onOpenConversation={vi.fn()} onStartInProject={vi.fn()} onCreateProject={vi.fn()} />))
      const needs = host.querySelector('.ps-state.needs')!
      expect(needs.textContent).toBe('Needs you')
      expect(needs.querySelector('svg')).not.toBeNull()
    } finally { act(() => root.unmount()); host.remove() }
  })
})
