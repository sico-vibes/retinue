import { describe, expect, it } from 'vitest'
import type { Agent, PermissionRequest, Project, Session } from '../types'
import { marqueeTiming } from './Marquee'
import { conversationStatus, projectBlocks } from './ProjectsSidebar'
import { hireSiblings } from './proposalGroups'

const agent = (id: string, extra: Partial<Agent> = {}) => ({ id, name: id, description: '', color: 'blue', ...extra }) as Agent
const project = (id: string) => ({ id, name: id, path: `C:/${id}` }) as Project
const session = (id: string, agentId: string, projectId: string, updatedAt: string, extra: Partial<Session> = {}) => ({ id, agentId, projectId, projectPath: `C:/${projectId}`, updatedAt, state: 'idle', ...extra }) as unknown as Session

describe('projects tree', () => {
  it('shows a hired employee before its first conversation there', () => {
    const agents = [agent('ada'), agent('wu')]
    const projects = [project('shop')]
    const without = projectBlocks(agents, projects, [])
    expect(without.find((block) => block.key === 'shop')?.bots ?? []).toEqual([])
    const withMembers = projectBlocks(agents, projects, [], new Map([['shop', [{ agentId: 'ada' }]]]))
    expect(withMembers.find((block) => block.key === 'shop')!.bots.map((bot) => bot.agent.id)).toEqual(['ada'])
  })

  it('ignores members of a project that is gone or an agent that is not listed', () => {
    const blocks = projectBlocks([agent('ada')], [project('shop')], [], new Map([['gone', [{ agentId: 'ada' }]], ['shop', [{ agentId: 'ghost' }]]]))
    expect(blocks.flatMap((block) => block.bots)).toEqual([])
  })

  it('orders an employee\'s conversations newest first', () => {
    const blocks = projectBlocks([agent('ada')], [project('shop')], [session('a', 'ada', 'shop', '2026-11-01T10:00:00Z'), session('b', 'ada', 'shop', '2026-11-02T10:00:00Z')])
    expect(blocks[0].bots[0].sessions.map((item) => item.id)).toEqual(['b', 'a'])
  })

  it('says what a conversation is doing in words', () => {
    const base = session('a', 'ada', 'shop', '2026-11-01T10:00:00Z')
    expect(conversationStatus(base, false)).toEqual({ label: 'Idle', tone: 'idle' })
    expect(conversationStatus({ ...base, state: 'working' } as Session, false).label).toBe('Working')
    expect(conversationStatus({ ...base, state: 'working' } as Session, true).label).toBe('Needs you')
    expect(conversationStatus({ ...base, state: 'limited' } as Session, false).label).toBe('Limited')
    expect(conversationStatus({ ...base, state: 'error' } as Session, false).tone).toBe('bad')
  })
})

describe('marquee timing', () => {
  it('grows with the distance and keeps the pauses', () => {
    expect(marqueeTiming(0)).toBeCloseTo(3.2)
    expect(marqueeTiming(56)).toBeGreaterThan(marqueeTiming(28))
  })
})

describe('hire proposals', () => {
  const hire = (id: string, groupId: string | null) => ({ id, kind: 'proposal', proposal: { id, kind: 'hire', payload: groupId ? { groupId } : {} } }) as unknown as PermissionRequest
  it('finds the other candidates from the same call only', () => {
    const one = hire('1', 'g')
    const list = [one, hire('2', 'g'), hire('3', 'other'), hire('4', null)]
    expect(hireSiblings(list, one).map((item) => item.id)).toEqual(['2'])
    expect(hireSiblings(list, hire('4', null))).toEqual([])
  })
})
