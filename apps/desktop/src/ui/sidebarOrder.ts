// Phase 6: how the sidebar orders bots, per computer. "Mine" keeps the order the owner dragged.
import type { Agent, Project, Session } from '../types'
import { mainSessionFor, sessionIsBusy } from './teamSelectors'

export type SidebarSort = 'manual' | 'recent' | 'name' | 'status'
const SORT_KEY = 'retinue.sidebar.sort'
const GROUP_KEY = 'retinue.sidebar.groupByProject'

export const SIDEBAR_SORTS: Array<{ id: SidebarSort; label: string }> = [
  { id: 'manual', label: 'My order' },
  { id: 'recent', label: 'Most recent' },
  { id: 'name', label: 'Name' },
  { id: 'status', label: 'Status' },
]

export function storedSidebarSort(): SidebarSort {
  try { const value = localStorage.getItem(SORT_KEY); return SIDEBAR_SORTS.some((item) => item.id === value) ? value as SidebarSort : 'manual' } catch { return 'manual' }
}
export function storeSidebarSort(sort: SidebarSort) { try { localStorage.setItem(SORT_KEY, sort) } catch { /* private mode */ } }
const VIEW_KEY = 'retinue.sidebar.view'
export type SidebarView = 'bots' | 'projects'
// 8 November 2026: Projects is the default; the bot roster is the Team view.
export function storedSidebarView(): SidebarView { try { return localStorage.getItem(VIEW_KEY) === 'bots' ? 'bots' : 'projects' } catch { return 'projects' } }
export function storeSidebarView(view: SidebarView) { try { localStorage.setItem(VIEW_KEY, view) } catch { /* private mode */ } }
export function storedSidebarGrouping(): boolean { try { return localStorage.getItem(GROUP_KEY) === '1' } catch { return false } }
export function storeSidebarGrouping(on: boolean) { try { localStorage.setItem(GROUP_KEY, on ? '1' : '0') } catch { /* private mode */ } }

const latest = (agent: Agent, sessions: readonly Session[]) => String(mainSessionFor(sessions as Session[], agent.id)?.updatedAt ?? '')

/** Needs you first, then working, then the rest. */
function urgency(agent: Agent, sessions: readonly Session[]): number {
  const session = mainSessionFor(sessions as Session[], agent.id)
  if (String(session?.state ?? '') === 'waiting_permission') return 0
  return sessionIsBusy(session) ? 1 : 2
}

export function sortBots(agents: readonly Agent[], sessions: readonly Session[], sort: SidebarSort): Agent[] {
  const list = [...agents]
  if (sort === 'name') return list.sort((a, b) => a.name.localeCompare(b.name))
  if (sort === 'recent') return list.sort((a, b) => latest(b, sessions).localeCompare(latest(a, sessions)))
  if (sort === 'status') return list.sort((a, b) => urgency(a, sessions) - urgency(b, sessions) || latest(b, sessions).localeCompare(latest(a, sessions)))
  return list
}

/** Bots under their default project, in the project order, and those with none last. */
export function groupBotsByProject(agents: readonly Agent[], projects: readonly Project[]): Array<{ id: string; name: string; agents: Agent[] }> {
  const groups = [...projects].sort((a, b) => a.sortOrder - b.sortOrder).map((project) => ({ id: project.id, name: project.name, agents: agents.filter((agent) => agent.projectId === project.id) }))
  const known = new Set(projects.map((project) => project.id))
  const rest = agents.filter((agent) => !agent.projectId || !known.has(agent.projectId))
  return [...groups, { id: 'no-project', name: 'No project', agents: rest }].filter((group) => group.agents.length > 0)
}

/** How much a conversation wants the owner: needs an answer (0), working (1), everything else (2). */
export function attentionRank(session: Session, needsYou = false): number {
  if (needsYou || String(session.state ?? '').toLowerCase() === 'waiting_permission') return 0
  return sessionIsBusy(session) ? 1 : 2
}

/** Orders by attention and keeps the given order inside each rank, so a list sorted by time stays so. */
export function attentionOrder<T>(items: readonly T[], rank: (item: T) => number): T[] {
  return items.map((item, index) => ({ item, index, rank: rank(item) })).sort((a, b) => a.rank - b.rank || a.index - b.index).map((entry) => entry.item)
}

/** A conversation settles once it's finished and an hour old: anything live or recent stays on top. */
export const SETTLE_AFTER_MS = 60 * 60 * 1000

export function hasSettled(session: Session, now: number, unread: boolean): boolean {
  if (unread || sessionIsBusy(session)) return false
  const state = String(session.state ?? '').toLowerCase()
  if (state === 'waiting_permission' || state === 'error' || state === 'failed') return false
  const at = Date.parse(String(session.updatedAt ?? session.createdAt ?? ''))
  return Number.isFinite(at) && now - at > SETTLE_AFTER_MS
}

/** Keeps each group's order, with the settled ones after the rest. */
export function settleOrder<T>(items: readonly T[], settled: (item: T) => boolean): T[] {
  return [...items.filter((item) => !settled(item)), ...items.filter(settled)]
}
