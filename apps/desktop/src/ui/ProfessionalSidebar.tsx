// The sidebar of Professional mode: every conversation with its provider logo, title, project and status, grouped by
// project (or one flat list), working and waiting ones on top, old ones on a collapsed Settled shelf. Other machines
// keep their own groups below. See workMode.ts.
import { useMemo, useState } from 'react'
import { ChevronRight, FolderOpen, ListFilter, Plus } from 'lucide-react'
import type { Agent, Project, Runtime, Session } from '../types'
import { RuntimeMark, providerBrand } from './providerBrand'
import { StatusLabel, conversationStatus } from './ProjectsSidebar'
import { ProjectIcon } from './ProjectIcon'
import { Collapse } from './Collapse'
import { TeamMenu, type MenuItem } from './TeamSidebar'
import { attentionOrder, attentionRank } from './sidebarOrder'
import { isSettled, useSettledChoices } from './settledStore'
import { activityAt, isEmptyConversation } from './teamSelectors'
import { shortTime } from './rosterSelectors'
import './professional.css'

const FLAT_KEY = 'retinue.pro.flat'
const FOLDED_KEY = 'retinue.pro.folded'
const readSet = (key: string) => { try { return new Set<string>(JSON.parse(window.localStorage.getItem(key) ?? '[]')) } catch { return new Set<string>() } }
const write = (key: string, value: string) => { try { window.localStorage.setItem(key, value) } catch { /* kept until the window closes */ } }

/** The employee a provider's new chats go to: the first live one on that provider (the default agent it was created with). */
export function agentForProvider(agents: readonly Agent[], runtimeId: string): Agent | undefined {
  return agents.filter((agent) => !agent.archived && agent.runtimeId === runtimeId).sort((a, b) => (a.sortOrder ?? 0) - (b.sortOrder ?? 0))[0]
}
/** Conversations Professional mode lists: the owner's own, not a teammate's side conversation or an automation's. */
export const listedConversation = (session: Session) => !session.archived && session.link?.kind !== 'side' && session.link?.kind !== 'automation' && !isEmptyConversation(session)

const normalPath = (value: string) => value.split('\\').join('/').replace(/\/+$/, '').toLowerCase()
/** A conversation belongs to its project by id, or by the folder it works in. */
function projectKeyOf(session: Session, byId: ReadonlyMap<string, Project>, byPath: ReadonlyMap<string, string>): string {
  if (session.projectId && byId.has(session.projectId)) return session.projectId
  const path = typeof session.projectPath === 'string' ? byPath.get(normalPath(session.projectPath)) : undefined
  return path ?? 'none'
}

function ProRow({ session, project, runtime, selected, status, showProject, onOpen }: { session: Session; project: Project | null | undefined; runtime: Runtime | undefined; selected: boolean; status: ReturnType<typeof conversationStatus>; showProject: boolean; onOpen: () => void }) {
  const title = session.title?.trim() || 'New chat'
  return <li role="none"><button type="button" role="treeitem" aria-current={selected ? 'true' : undefined} className={`pro-row${selected ? ' selected' : ''}`} onClick={onOpen} title={title}>
    <span className="pro-row-logo"><RuntimeMark provider={runtime?.provider} size={16} /></span>
    <span className="pro-row-copy">
      <span className="pro-row-title">{title}</span>
      <span className="pro-row-meta">{showProject && <span className="pro-row-project">{project?.name ?? 'No project'}</span>}{showProject && <span aria-hidden="true">·</span>}<StatusLabel {...status} /></span>
    </span>
    <time className="pro-row-time" dateTime={activityAt(session)}>{shortTime(activityAt(session))}</time>
  </button></li>
}

export function ProfessionalSidebar({ agents, projects, sessions, runtimes, selectedSessionId, approvalSessionIds, canCreate, onOpenConversation, onStartInProject, onCreateProject, machines }: {
  agents: readonly Agent[]; projects: readonly Project[]; sessions: readonly Session[]; runtimes: readonly Runtime[]
  selectedSessionId: string | null; approvalSessionIds?: ReadonlySet<string>; canCreate: boolean
  onOpenConversation: (agent: Agent, session: Session) => void
  onStartInProject: (agent: Agent, projectId: string | null) => void
  onCreateProject: () => void
  machines?: React.ReactNode
}) {
  const choices = useSettledChoices()
  const [flat, setFlat] = useState(() => { try { return window.localStorage.getItem(FLAT_KEY) === '1' } catch { return false } })
  const [folded, setFolded] = useState(() => readSet(FOLDED_KEY))
  const [settledOpen, setSettledOpen] = useState<ReadonlySet<string>>(() => new Set())
  const [menu, setMenu] = useState<{ x: number; y: number; projectId: string | null } | null>(null)
  const now = Date.now()
  const projectById = useMemo(() => new Map(projects.map((project) => [project.id, project])), [projects])
  const runtimeById = useMemo(() => new Map(runtimes.map((runtime) => [runtime.id, runtime])), [runtimes])
  const projectByPath = useMemo(() => new Map(projects.filter((project) => project.path).map((project) => [normalPath(project.path!), project.id])), [projects])
  const agentById = useMemo(() => new Map(agents.map((agent) => [agent.id, agent])), [agents])

  const rows = useMemo(() => {
    const recent = sessions.filter(listedConversation).sort((a, b) => Date.parse(activityAt(b)) - Date.parse(activityAt(a)))
    // Waiting for you first, then working, then the rest by recency.
    return attentionOrder(recent, (session) => attentionRank(session, approvalSessionIds?.has(session.id) ?? false))
  }, [sessions, approvalSessionIds])

  const groups = useMemo(() => {
    if (flat) return [{ key: 'all', project: null as Project | null, label: '', sessions: rows }]
    const byProject = new Map<string, Session[]>()
    for (const session of rows) { const key = projectKeyOf(session, projectById, projectByPath); byProject.set(key, [...(byProject.get(key) ?? []), session]) }
    return [...byProject.entries()].map(([key, list]) => ({ key, project: key === 'none' ? null : projectById.get(key) ?? null, label: key === 'none' ? 'No project' : projectById.get(key)?.name ?? 'Project', sessions: list }))
      .sort((a, b) => (a.key === 'none' ? 1 : 0) - (b.key === 'none' ? 1 : 0))
  }, [rows, flat, projectById])

  const openNew = (event: React.MouseEvent<HTMLElement>, projectId: string | null) => { const rect = event.currentTarget.getBoundingClientRect(); setMenu({ x: rect.left, y: rect.bottom + 4, projectId }) }
  const items: MenuItem[] = (menu ? runtimes.filter((runtime) => agentForProvider(agents, runtime.id)) : []).map((runtime) => ({
    label: `New ${providerBrand(runtime.provider).name} chat`, icon: <RuntimeMark provider={runtime.provider} size={15} />, disabled: !canCreate,
    onSelect: () => { const agent = agentForProvider(agents, runtime.id); if (agent) onStartInProject(agent, menu!.projectId) },
  }))
  const toggleFold = (key: string) => setFolded((current) => { const next = new Set(current); if (next.has(key)) next.delete(key); else next.add(key); write(FOLDED_KEY, JSON.stringify([...next])); return next })

  const renderRow = (session: Session, showProject: boolean) => {
    const agent = agentById.get(session.agentId ?? '')
    const status = conversationStatus(session, approvalSessionIds?.has(session.id) ?? false)
    return <ProRow key={session.id} session={session} project={projectById.get(projectKeyOf(session, projectById, projectByPath))} runtime={runtimeById.get(session.runtimeId ?? agent?.runtimeId ?? '')} selected={session.id === selectedSessionId} status={status} showProject={showProject} onOpen={() => { if (agent) onOpenConversation(agent, session) }} />
  }

  return <div className="pro-sidebar">
    <div className="pro-head">
      <span>Chats</span>
      <span className="pro-head-tools">
        <button type="button" className="icon-button tiny" aria-pressed={flat} aria-label={flat ? 'Group chats by project' : 'Show one flat list'} title={flat ? 'Group by project' : 'One flat list'} onClick={() => { setFlat(!flat); write(FLAT_KEY, flat ? '0' : '1') }}><ListFilter size={14} aria-hidden="true" /></button>
        <button type="button" className="icon-button tiny" aria-label="New project" title="New project" onClick={onCreateProject}><FolderOpen size={14} aria-hidden="true" /></button>
        <button type="button" className="icon-button tiny" aria-haspopup="menu" aria-label="New chat" title="New chat" onClick={(event) => openNew(event, null)}><Plus size={14} aria-hidden="true" /></button>
      </span>
    </div>
    {groups.map((group) => {
      const live = group.sessions.filter((session) => !isSettled(session, now, choices))
      const settled = group.sessions.filter((session) => isSettled(session, now, choices))
      const isFolded = folded.has(group.key)
      const settledShown = settledOpen.has(group.key) || settled.some((session) => session.id === selectedSessionId)
      return <section key={group.key} className="pro-group" aria-label={group.label || 'Chats'}>
        {!flat && <div className="pro-group-head">
          <button type="button" className="pro-group-fold" aria-expanded={!isFolded} onClick={() => toggleFold(group.key)}>
            <ChevronRight size={12} aria-hidden="true" className={`fold-chevron${isFolded ? '' : ' open'}`} />
            {group.project ? <ProjectIcon project={group.project} size={14} /> : null}<span className="pro-group-name">{group.label}</span>{isFolded && <span className="pro-count">{group.sessions.length}</span>}
          </button>
          <button type="button" className="icon-button tiny pro-group-add" aria-haspopup="menu" aria-label={`New chat in ${group.label}`} title="New chat here" onClick={(event) => openNew(event, group.project?.id ?? null)}><Plus size={13} aria-hidden="true" /></button>
        </div>}
        <Collapse open={flat || !isFolded}>
          <ul role="tree" className="pro-list">{live.map((session) => renderRow(session, flat))}</ul>
          {settled.length > 0 && <div className="pro-settled">
            <button type="button" className="pro-settled-head" aria-expanded={settledShown} onClick={() => setSettledOpen((current) => { const next = new Set(current); if (settledShown) next.delete(group.key); else next.add(group.key); return next })}>
              <ChevronRight size={12} aria-hidden="true" className={`fold-chevron${settledShown ? ' open' : ''}`} /><span>Settled</span><span className="pro-count">{settled.length}</span>
            </button>
            <Collapse open={settledShown}><ul role="tree" className="pro-list pro-list-settled">{settled.map((session) => renderRow(session, flat))}</ul></Collapse>
          </div>}
        </Collapse>
      </section>
    })}
    {rows.length === 0 && <p className="pro-empty">No chats yet. Start one with the plus.</p>}
    {machines}
    {menu && <TeamMenu label="Start a chat with" position={menu} items={items} onClose={() => setMenu(null)} />}
  </div>
}
