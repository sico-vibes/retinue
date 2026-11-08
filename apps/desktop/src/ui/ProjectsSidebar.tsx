// The sidebar's Projects tree, the main way in since 8 November 2026 (owner:
// "the project tab to be the primary tab"). Projects on top; under each, the
// employees hired into it (or, with an older daemon, the bots that worked
// there); under the selected employee its newest conversations as two-line
// cards: the title, then when it last changed and what it is doing now. A
// selected name or title longer than its row scrolls (Marquee). Folding is
// animated (Collapse). Spacing follows the density tokens in density.css.
import { useMemo, useRef, useState } from 'react'
import { BellRing, ChevronRight, CircleAlert, Plus } from 'lucide-react'
import type { Agent, Project, Session } from '../types'
import { BlobCanvas } from '../blob/BlobCanvas'
import { agentLook } from '../blob/look'
import { agentColorHex } from './agentColor'
import { Collapse } from './Collapse'
import { Marquee } from './Marquee'
import { ProjectIcon } from './ProjectIcon'
import type { MembersByProject } from './projectMembers'
import { shortTime } from './rosterSelectors'
import { isManager, isSideSession, projectOfSession, sessionIsBusy } from './teamSelectors'
import { TeamMenu, type MenuItem } from './TeamSidebar'
import './projectsSidebar.css'

const FOLDED_KEY = 'bitbots.sidebar.foldedProjects'
const RECENT_PER_BOT = 4

const stampOf = (session: Session) => String(session.updatedAt ?? session.createdAt ?? '')

type BotInProject = { agent: Agent; sessions: Session[] }
type ProjectBlock = { key: string; project: Project | null; bots: BotInProject[]; latest: string }

export type ConversationStatus = { label: string; tone: 'working' | 'needs' | 'limited' | 'bad' | 'idle' }

/** What a conversation is doing now, in words for the card's second line. */
export function conversationStatus(session: Session, needsYou: boolean): ConversationStatus {
  const state = String(session.state ?? '').toLowerCase()
  if (needsYou || state === 'waiting_permission') return { label: 'Needs you', tone: 'needs' }
  if (sessionIsBusy(session)) return { label: 'Working', tone: 'working' }
  if (state === 'limited' || state === 'rate_limited') return { label: 'Limited', tone: 'limited' }
  if (state === 'error' || state === 'failed') return { label: 'Stopped with an error', tone: 'bad' }
  return { label: 'Idle', tone: 'idle' }
}

/**
 * Projects newest first; in each, its employees newest first. With `members`
 * (the daemon's hiring table) an employee shows even before its first
 * conversation there; without it, membership is worked out from conversations
 * and default projects, as before hiring existed.
 */
export function projectBlocks(agents: readonly Agent[], projects: readonly Project[], sessions: readonly Session[], members: MembersByProject | null = null): ProjectBlock[] {
  const byId = new Map(agents.map((agent) => [agent.id, agent]))
  const cells = new Map<string, Map<string, Session[]>>()
  const put = (key: string, agentId: string, session?: Session) => {
    const row = cells.get(key) ?? new Map<string, Session[]>()
    const list = row.get(agentId) ?? []
    if (session) list.push(session)
    row.set(agentId, list)
    cells.set(key, row)
  }
  for (const session of sessions) {
    if (session.archived || isSideSession(session) || !session.agentId || !byId.has(session.agentId)) continue
    put(projectOfSession(session, projects)?.id ?? 'none', session.agentId, session)
  }
  for (const agent of agents) if (agent.projectId && projects.some((project) => project.id === agent.projectId)) put(agent.projectId, agent.id)
  if (members) for (const [projectId, list] of members) for (const member of list) if (byId.has(member.agentId) && projects.some((project) => project.id === projectId)) put(projectId, member.agentId)
  const blocks: ProjectBlock[] = [...projects.map((project) => ({ project, key: project.id })), { project: null, key: 'none' }].map(({ project, key }) => {
    const bots = [...(cells.get(key) ?? new Map<string, Session[]>())].map(([agentId, list]): BotInProject => ({ agent: byId.get(agentId)!, sessions: list.sort((a, b) => stampOf(b).localeCompare(stampOf(a))) }))
    bots.sort((a, b) => stampOf(b.sessions[0] ?? ({} as Session)).localeCompare(stampOf(a.sessions[0] ?? ({} as Session))) || a.agent.name.localeCompare(b.agent.name))
    return { key, project, bots, latest: bots.reduce((best, bot) => { const at = bot.sessions[0] ? stampOf(bot.sessions[0]) : ''; return at > best ? at : best }, '') }
  })
  // The "No project" block only shows when something is in it.
  return blocks.filter((block) => block.project || block.bots.length).sort((a, b) => (a.project ? 0 : 1) - (b.project ? 0 : 1) || b.latest.localeCompare(a.latest) || (a.project?.name ?? '').localeCompare(b.project?.name ?? ''))
}

/** A status in words: Needs you carries a bell, Working a light sweeping over the word. */
export function StatusLabel({ tone, label }: ConversationStatus) {
  return <span className={`ps-state ${tone}`}>
    {tone === 'needs' && <BellRing size={11} strokeWidth={2.2} aria-hidden="true" />}
    {tone === 'bad' && <CircleAlert size={11} strokeWidth={2.2} aria-hidden="true" />}
    {tone === 'working' ? <span className="ps-shimmer">{label}</span> : label}
  </span>
}

function storedFolds(): Set<string> { try { return new Set<string>(JSON.parse(localStorage.getItem(FOLDED_KEY) ?? '[]')) } catch { return new Set<string>() } }

export function ProjectsSidebar({ agents, projects, sessions, members = null, selectedSessionId, approvalSessionIds, canCreate, onOpenConversation, onStartInProject, onCreateProject, onHire, onAskCeo }: {
  agents: readonly Agent[]
  projects: readonly Project[]
  sessions: readonly Session[]
  members?: MembersByProject | null
  selectedSessionId: string | null
  approvalSessionIds?: ReadonlySet<string>
  canCreate: boolean
  /** Opens a conversation of a bot, selecting the bot first. */
  onOpenConversation: (agent: Agent, session: Session) => void
  /** A new conversation with this bot in this project (null for no project). */
  onStartInProject: (agent: Agent, projectId: string | null) => void
  onCreateProject: () => void
  /** Opens the Team page's hiring list for this project. */
  onHire?: (projectId: string) => void
  /** Opens a conversation with the CEO about staffing this project. */
  onAskCeo?: (projectId: string) => void
}) {
  const bots = useMemo(() => agents.filter((agent) => !agent.archived && !agent.hidden && !agent.parentAgentId && !isManager(agent)), [agents])
  const blocks = useMemo(() => projectBlocks(bots, projects, sessions, members), [bots, projects, sessions, members])
  const [folded, setFolded] = useState(storedFolds)
  // An employee's cards follow the selection (open while it holds the selected
  // conversation) until the owner folds or unfolds them; that choice then wins.
  const [folds, setFolds] = useState<ReadonlyMap<string, boolean>>(new Map())
  const [menu, setMenu] = useState<{ items: MenuItem[]; label: string; x: number; y: number } | null>(null)
  const trigger = useRef<HTMLElement | null>(null)
  const unfold = (key: string) => setFolded((current) => { const next = new Set(current); next.delete(key); return next })
  const toggleFold = (key: string) => setFolded((current) => { const next = new Set(current); if (next.has(key)) next.delete(key); else next.add(key); try { localStorage.setItem(FOLDED_KEY, JSON.stringify([...next])) } catch { /* a remembered fold is a convenience only */ } return next })
  const setOpenFor = (id: string, value: boolean) => setFolds((current) => new Map(current).set(id, value))
  const projectMenu = (button: HTMLElement, block: ProjectBlock) => {
    trigger.current = button
    const rect = button.getBoundingClientRect()
    const name = block.project?.name ?? 'no project'
    const projectId = block.project?.id ?? null
    const people: MenuItem[] = block.bots.length
      ? block.bots.map(({ agent }): MenuItem => ({ label: `New conversation with ${agent.name}`, disabled: !canCreate, onSelect: () => { unfold(block.key); onStartInProject(agent, projectId) } }))
      : bots.map((agent): MenuItem => ({ label: `New conversation with ${agent.name}`, disabled: !canCreate, onSelect: () => { unfold(block.key); onStartInProject(agent, projectId) } }))
    const staffing: MenuItem[] = projectId ? [
      ...(onHire ? [{ label: 'Hire an employee…', onSelect: () => onHire(projectId) } as MenuItem] : []),
      ...(onAskCeo ? [{ label: 'Ask the CEO who to hire', disabled: !canCreate, onSelect: () => onAskCeo(projectId) } as MenuItem] : []),
    ] : []
    const items: MenuItem[] = [...staffing, ...(staffing.length && people.length ? ['separator' as const] : []), ...people]
    setMenu({ label: `Actions for ${name}`, x: rect.left, y: rect.bottom + 4, items: items.length ? items : [{ label: 'No employees yet', disabled: true }] })
  }
  const closeMenu = () => { setMenu(null); requestAnimationFrame(() => trigger.current?.focus()) }

  return <div className="projects-sidebar" role="tree" aria-label="Projects">
    {blocks.map((block) => {
      const name = block.project?.name ?? 'No project'
      const isFolded = folded.has(block.key)
      return <section key={block.key} className="ps-project" aria-label={name}>
        <div className="ps-project-head">
          <button type="button" className="ps-fold" aria-expanded={!isFolded} aria-label={`${isFolded ? 'Expand' : 'Collapse'} ${name}`} onClick={() => toggleFold(block.key)}>
            <ChevronRight size={12} aria-hidden="true" className={`fold-chevron${isFolded ? '' : ' open'}`} />
            <ProjectIcon project={block.project} size={14} />
            <span className="ps-project-name" title={block.project?.path ?? undefined}>{name}</span>
            {isFolded && block.bots.length > 0 && <span className="ps-count">{block.bots.length}</span>}
          </button>
          <button type="button" className="icon-button tiny ps-add" aria-label={`Actions for ${name}`} title="New conversation or hire" aria-haspopup="menu" onClick={(event) => projectMenu(event.currentTarget, block)}><Plus size={13} aria-hidden="true" /></button>
        </div>
        <Collapse open={!isFolded}>
          <ul className="ps-bots" role="group">
            {block.bots.length === 0 && <li className="ps-empty">{block.project && onHire ? <button type="button" className="ps-empty-hire" onClick={() => onHire(block.project!.id)}>No one hired yet. Hire an employee</button> : 'No one here yet.'}</li>}
            {block.bots.map(({ agent, sessions: list }) => {
              const latest = list[0]
              const recent = list.slice(0, RECENT_PER_BOT)
              const rowKey = `${block.key}:${agent.id}`
              const holdsSelection = recent.some((session) => session.id === selectedSessionId)
              const expanded = recent.length > 0 && (folds.get(rowKey) ?? holdsSelection)
              const working = list.some((session) => sessionIsBusy(session))
              const needsYou = list.some((session) => approvalSessionIds?.has(session.id))
              return <li key={agent.id} role="none" className="ps-bot-item">
                <div className={`ps-bot marquee-host${holdsSelection ? ' selected' : ''}`}>
                  <button type="button" role="treeitem" aria-expanded={recent.length ? expanded : undefined} className="ps-bot-main" onClick={() => { if (!latest) { onStartInProject(agent, block.project?.id ?? null); return } if (!expanded) setOpenFor(rowKey, true); onOpenConversation(agent, latest) }} title={latest ? undefined : `Start a conversation with ${agent.name}`}>
                    <BlobCanvas decorative color={agentColorHex(agent.color)} size={28} mini mood={working ? 'working' : 'idle'} look={agentLook(agent)} label={agent.name} />
                    <span className="ps-bot-copy">
                      <span className="ps-bot-line">
                        <Marquee className="ps-bot-name" text={agent.name} active={holdsSelection} />
                        {needsYou ? <StatusLabel tone="needs" label="Needs you" /> : working ? <StatusLabel tone="working" label="Working" /> : latest ? <time dateTime={stampOf(latest)}>{shortTime(stampOf(latest))}</time> : null}
                      </span>
                      <span className="ps-bot-sub">{agent.role?.trim() || (latest ? (latest.title?.trim() || 'New chat') : 'No conversations yet')}</span>
                    </span>
                  </button>
                  {recent.length > 1 && <button type="button" className="icon-button tiny ps-more" aria-label={`${expanded ? 'Hide' : 'Show'} ${agent.name}'s conversations in ${name}`} aria-expanded={expanded} onClick={() => setOpenFor(rowKey, !expanded)}><ChevronRight size={12} aria-hidden="true" className={`fold-chevron${expanded ? ' open' : ''}`} /></button>}
                </div>
                <Collapse open={expanded}>
                  <ul className="ps-convos" role="group">{recent.map((session) => {
                    const selected = session.id === selectedSessionId
                    const status = conversationStatus(session, approvalSessionIds?.has(session.id) ?? false)
                    return <li key={session.id} role="none"><button type="button" role="treeitem" aria-current={selected ? 'true' : undefined} className={`ps-convo marquee-host${selected ? ' selected' : ''}`} onClick={() => onOpenConversation(agent, session)}>
                      <Marquee className="ps-convo-title" text={session.title?.trim() || 'New chat'} active={selected} />
                      <span className="ps-convo-meta"><time dateTime={stampOf(session)}>{shortTime(stampOf(session))}</time><span aria-hidden="true">·</span><StatusLabel tone={status.tone} label={status.label} /></span>
                    </button></li>
                  })}</ul>
                </Collapse>
              </li>
            })}
          </ul>
        </Collapse>
      </section>
    })}
    {blocks.length === 0 && <p className="team-group-empty">No projects yet.</p>}
    <button type="button" className="team-hidden-toggle" onClick={onCreateProject}><Plus size={13} aria-hidden="true" />New project</button>
    {menu && <TeamMenu label={menu.label} position={menu} items={menu.items} onClose={closeMenu} />}
  </div>
}
