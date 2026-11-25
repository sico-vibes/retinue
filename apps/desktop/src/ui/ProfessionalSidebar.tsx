// The sidebar of Professional mode: every conversation with its provider logo, title, status, project and where it lives
// (this computer, a remote PC, a VPS, a phone), grouped by project (or one flat list), working and waiting ones on top.
// Other devices keep their own groups below; every settled conversation, from any device, sits in one shelf docked at the
// bottom. See workMode.ts.
import { useEffect, useMemo, useState, type DragEvent, type ReactNode } from 'react'
import { CheckCheck, ChevronRight, FolderOpen, GitBranch, ListFilter, MoreHorizontal, Plus, Search } from 'lucide-react'
import type { Agent, Project, Runtime, Session } from '../types'
import { RuntimeMark, providerBrand } from './providerBrand'
import { ProBadge, ProStatusIcon, proBadgeFor, type ProTone } from './ProBadge'
import { ProjectIcon } from './ProjectIcon'
import { Collapse } from './Collapse'
import { TeamMenu, type MenuItem } from './TeamSidebar'
import { attentionOrder, attentionRank } from './sidebarOrder'
import { isSettled, useSettledChoices } from './settledStore'
import { activityAt, isEmptyConversation } from './teamSelectors'
import { shortTime } from './rosterSelectors'
import { formatElapsed } from './CompanionPanels'
import { LOCAL_WHERE, DeviceMenu, WhereIcon, useDevicePrefs, useDeviceRows, whereKindOf, type Where } from './ProfessionalDevices'
import { rememberRemoteTab } from './ProfessionalTabs'
import type { Machine, MachineBot, MachineSessionRow } from './machines'
import './professional.css'

const FLAT_KEY = 'retinue.pro.flat'
const FOLDED_KEY = 'retinue.pro.folded'
const DEVICES_FOLDED_KEY = 'retinue.pro.machines.folded'
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

/** How long a working chat has been at it, ticking each second on its own so the list does not re-render. */
function WorkTimer({ since }: { since: string }) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => { const id = window.setInterval(() => setNow(Date.now()), 1000); return () => window.clearInterval(id) }, [])
  const start = Date.parse(since)
  return <time className="pro-row-time pro-row-timer" dateTime={since} aria-label="Working for">{Number.isFinite(start) ? formatElapsed(Math.max(0, now - start)) : '—'}</time>
}

/** One chat: provider logo, title and the time of its last message (or, while it works, how long it has been working) on the
 *  first line; its project, status and place on the second. The project starts under the title; the status sits under the time. */
export function ProRow({ title, provider, badge, project, projectIcon, stamp, since, where, selected, dimmed = false, onOpen }: { title: string; provider: string | null | undefined; badge: { tone: ProTone; label: string }; project?: string | null; projectIcon?: ReactNode; stamp: string; since?: string | null; where?: Where; selected: boolean; dimmed?: boolean; onOpen: () => void }) {
  const working = badge.tone === 'working' && !!since
  return <li role="none"><button type="button" role="treeitem" aria-current={selected ? 'true' : undefined} className={`pro-row${selected ? ' selected' : ''}${dimmed ? ' dimmed' : ''}`} onClick={onOpen} title={title}>
    <span className="pro-row-line"><span className="pro-row-logo"><RuntimeMark provider={provider} size={16} /></span><span className="pro-row-title">{title}</span>{working ? <WorkTimer since={since!} /> : <time className="pro-row-time" dateTime={stamp}>{shortTime(stamp)}</time>}</span>
    <span className="pro-row-foot">{project !== undefined && <span className="pro-row-project">{projectIcon}<span className="pro-row-project-name">{project || 'No project'}</span></span>}<ProStatusIcon {...badge} />{where && <span className="pro-row-where"><WhereIcon where={where} size={13} /></span>}</span>
  </button></li>
}

/** When the work under way began: the current turn if the record has it, else the message that started it. */
const workingSince = (session: Session): string | null => {
  const turn = session.turns?.at(-1) as { createdAt?: unknown; startedAt?: unknown } | undefined
  const value = turn?.startedAt ?? turn?.createdAt ?? session.lastMessageAt ?? session.updatedAt
  return typeof value === 'string' ? value : null
}

/** A row in any list, wherever the chat lives: this computer or another device. */
type Item = { key: string; title: string; provider: string | null | undefined; badge: { tone: ProTone; label: string }; project: string | null; projectNode: ReactNode; stamp: string; since: string | null; where: Where; selected: boolean; dimmed: boolean; onOpen: () => void }
const renderItem = (item: Item) => <ProRow key={item.key} title={item.title} provider={item.provider} badge={item.badge} project={item.project} projectIcon={item.projectNode} stamp={item.stamp} since={item.since} where={item.where} selected={item.selected} dimmed={item.dimmed} onOpen={item.onOpen} />

export type ProDevices = {
  machines: readonly Machine[]
  selected: { machineId: string; botId: string; sessionId?: string } | null
  onOpen: (machine: Machine, bot: MachineBot, sessionId: string) => void
  /** Reloads the machine list after a rename. */
  onReload: () => void
  /** Opens the folder or clone dialog for that machine. */
  onNewProject: (machine: Machine, mode: 'folder' | 'clone') => void
  /** Starts a chat on that machine with that bot and opens it as a tab. */
  onStartChat: (machine: Machine, bot: MachineBot) => void
}

export function ProfessionalSidebar({ agents, projects, sessions, runtimes, selectedSessionId, approvalSessionIds, canCreate, onOpenConversation, onStartInProject, onCreateProject, devices }: {
  agents: readonly Agent[]; projects: readonly Project[]; sessions: readonly Session[]; runtimes: readonly Runtime[]
  selectedSessionId: string | null; approvalSessionIds?: ReadonlySet<string>; canCreate: boolean
  onOpenConversation: (agent: Agent, session: Session) => void
  onStartInProject: (agent: Agent, projectId: string | null) => void
  onCreateProject: () => void
  devices?: ProDevices
}) {
  const choices = useSettledChoices()
  const [flat, setFlat] = useState(() => { try { return window.localStorage.getItem(FLAT_KEY) === '1' } catch { return false } })
  const [folded, setFolded] = useState(() => readSet(FOLDED_KEY))
  const [devicesFolded, setDevicesFolded] = useState(() => readSet(DEVICES_FOLDED_KEY))
  const [settledOpen, setSettledOpen] = useState(false)
  const [settledQuery, setSettledQuery] = useState('')
  const [menu, setMenu] = useState<{ x: number; y: number; label: string; items: MenuItem[] } | null>(null)
  const [deviceMenu, setDeviceMenu] = useState<{ machine: Machine; x: number; y: number } | null>(null)
  const [dragId, setDragId] = useState<string | null>(null)
  const [dropId, setDropId] = useState<string | null>(null)
  const now = Date.now()
  const machines = devices?.machines ?? []
  const deviceRows = useDeviceRows(machines)
  const prefs = useDevicePrefs(machines)
  const projectById = useMemo(() => new Map(projects.map((project) => [project.id, project])), [projects])
  const runtimeById = useMemo(() => new Map(runtimes.map((runtime) => [runtime.id, runtime])), [runtimes])
  const projectByPath = useMemo(() => new Map(projects.filter((project) => project.path).map((project) => [normalPath(project.path!), project.id])), [projects])
  const agentById = useMemo(() => new Map(agents.map((agent) => [agent.id, agent])), [agents])

  const rows = useMemo(() => {
    const recent = sessions.filter(listedConversation).sort((a, b) => Date.parse(activityAt(b)) - Date.parse(activityAt(a)))
    // Waiting for you first, then working, then the rest by recency.
    return attentionOrder(recent, (session) => attentionRank(session, approvalSessionIds?.has(session.id) ?? false))
  }, [sessions, approvalSessionIds])

  const localItem = (session: Session): Item => {
    const agent = agentById.get(session.agentId ?? '')
    const runtime = runtimeById.get(session.runtimeId ?? agent?.runtimeId ?? '')
    const project = projectById.get(projectKeyOf(session, projectById, projectByPath)) ?? null
    return { key: session.id, title: session.title?.trim() || 'New chat', provider: runtime?.provider, badge: proBadgeFor(session, approvalSessionIds?.has(session.id) ?? false), project: project?.name ?? null, projectNode: <ProjectIcon project={project} size={13} />, stamp: activityAt(session), since: workingSince(session), where: LOCAL_WHERE, selected: session.id === selectedSessionId && !devices?.selected?.sessionId, dimmed: false, onOpen: () => { if (agent) onOpenConversation(agent, session) } }
  }
  const deviceItem = (machine: Machine, row: MachineSessionRow): Item => {
    const bot = row.agentId ? machine.bots.find((item) => item.id === row.agentId) : undefined
    const title = row.title?.trim() || 'New chat'
    return { key: `${machine.id}:${row.id}`, title, provider: bot?.provider, badge: proBadgeFor({ state: row.state ?? 'idle' }, row.state === 'waiting_permission'), project: row.projectName ?? null, projectNode: <ProjectIcon project={null} size={13} />, stamp: row.updatedAt ?? '', since: row.updatedAt ?? null, where: { kind: whereKindOf(machine), name: machine.name, color: prefs.colorOf(machine.id) }, selected: devices?.selected?.machineId === machine.id && devices.selected.sessionId === row.id, dimmed: machine.state !== 'online', onOpen: () => { if (bot && devices) { rememberRemoteTab(machine.id, bot.id, row.id, { title, provider: bot.provider ?? null, machineName: machine.name }); devices.onOpen(machine, bot, row.id) } } }
  }
  // A row's age for the settle rule: the few fields it reads, from a device's record.
  const asSession = (row: MachineSessionRow) => ({ id: row.id, state: row.state ?? 'idle', createdAt: row.updatedAt ?? '', updatedAt: row.updatedAt ?? '', lastMessageAt: row.updatedAt ?? undefined, messages: [] }) as unknown as Session

  const groups = useMemo(() => {
    const live = rows.filter((session) => !isSettled(session, now, choices))
    if (flat) return [{ key: 'all', project: null as Project | null, label: '', sessions: live }]
    const byProject = new Map<string, Session[]>()
    for (const session of live) { const key = projectKeyOf(session, projectById, projectByPath); byProject.set(key, [...(byProject.get(key) ?? []), session]) }
    return [...byProject.entries()].map(([key, list]) => ({ key, project: key === 'none' ? null : projectById.get(key) ?? null, label: key === 'none' ? 'No project' : projectById.get(key)?.name ?? 'Project', sessions: list }))
      .sort((a, b) => (a.key === 'none' ? 1 : 0) - (b.key === 'none' ? 1 : 0))
  }, [rows, flat, projectById, projectByPath, choices, now])

  const settledItems: Item[] = [
    ...rows.filter((session) => isSettled(session, now, choices)).map(localItem),
    ...machines.flatMap((machine) => (deviceRows[machine.id] ?? []).filter((row) => isSettled(asSession(row), now, choices)).map((row) => deviceItem(machine, row))),
  ].sort((a, b) => Date.parse(b.stamp) - Date.parse(a.stamp))
  const shelfShown = settledOpen || settledItems.some((item) => item.selected)
  // Search the settled chats by title, project or device; every word must match somewhere.
  const words = settledQuery.trim().toLowerCase().split(/\s+/).filter(Boolean)
  const settledShown = words.length ? settledItems.filter((item) => { const hay = `${item.title} ${item.project ?? 'no project'} ${item.where.name ?? ''}`.toLowerCase(); return words.every((word) => hay.includes(word)) }) : settledItems

  // Where a chat or a project can start (T3 Code asks for the environment first): this computer, or any device that is online.
  const botName = (bot: MachineBot) => (bot.provider ? providerBrand(bot.provider).name : bot.name)
  const offlineNote = (machine: Machine) => (machine.state === 'online' ? '' : ' (offline)')
  const localChatItems = (projectId: string | null): MenuItem[] => runtimes.filter((runtime) => agentForProvider(agents, runtime.id)).map((runtime) => ({
    label: `New ${providerBrand(runtime.provider).name} chat`, icon: <RuntimeMark provider={runtime.provider} size={15} />, disabled: !canCreate,
    onSelect: () => { const agent = agentForProvider(agents, runtime.id); if (agent) onStartInProject(agent, projectId) },
  }))
  const deviceChatItems = (machine: Machine): MenuItem[] => machine.bots.map((bot) => ({
    label: `New ${botName(bot)} chat`, icon: <RuntimeMark provider={bot.provider} size={15} />, disabled: machine.state !== 'online',
    onSelect: () => devices?.onStartChat(machine, bot),
  }))
  const deviceProjectItems = (machine: Machine): MenuItem[] => [
    { label: 'Open a folder…', icon: <FolderOpen size={15} aria-hidden="true" />, disabled: machine.state !== 'online', onSelect: () => devices?.onNewProject(machine, 'folder') },
    { label: 'Clone a repository…', icon: <GitBranch size={15} aria-hidden="true" />, disabled: machine.state !== 'online', onSelect: () => devices?.onNewProject(machine, 'clone') },
  ]
  const whereOf = (machine: Machine): Where => ({ kind: whereKindOf(machine), name: machine.name, color: prefs.colorOf(machine.id) })
  const anchor = (event: React.MouseEvent<HTMLElement>) => { const rect = event.currentTarget.getBoundingClientRect(); return { x: rect.left, y: rect.bottom + 4 } }
  const openNew = (event: React.MouseEvent<HTMLElement>, projectId: string | null, withDevices = false) => {
    const items: MenuItem[] = [...localChatItems(projectId)]
    if (withDevices && machines.length) items.push('separator', ...prefs.ordered.map((machine): MenuItem => ({ label: `On ${machine.name}${offlineNote(machine)}`, icon: <WhereIcon where={whereOf(machine)} size={14} />, disabled: machine.state !== 'online', submenu: deviceChatItems(machine) })))
    setMenu({ ...anchor(event), label: 'Start a chat', items })
  }
  const openProjectMenu = (event: React.MouseEvent<HTMLElement>) => {
    if (!machines.length) { onCreateProject(); return }
    setMenu({ ...anchor(event), label: 'Add a project', items: [
      { label: 'Open a folder on this computer…', icon: <WhereIcon where={LOCAL_WHERE} size={14} />, onSelect: onCreateProject },
      'separator',
      ...prefs.ordered.map((machine): MenuItem => ({ label: `On ${machine.name}${offlineNote(machine)}`, icon: <WhereIcon where={whereOf(machine)} size={14} />, disabled: machine.state !== 'online', submenu: deviceProjectItems(machine) })),
    ] })
  }
  const openDeviceNew = (event: React.MouseEvent<HTMLElement>, machine: Machine) => setMenu({ ...anchor(event), label: `On ${machine.name}`, items: [...deviceChatItems(machine), 'separator', ...deviceProjectItems(machine)] })
  const toggleFold = (key: string) => setFolded((current) => { const next = new Set(current); if (next.has(key)) next.delete(key); else next.add(key); write(FOLDED_KEY, JSON.stringify([...next])); return next })
  const toggleDevice = (id: string) => setDevicesFolded((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); write(DEVICES_FOLDED_KEY, JSON.stringify([...next])); return next })
  const dragProps = (id: string) => ({
    draggable: true,
    onDragStart: (event: DragEvent) => { setDragId(id); event.dataTransfer.effectAllowed = 'move'; event.dataTransfer.setData('text/plain', id) },
    onDragOver: (event: DragEvent) => { if (dragId && dragId !== id) { event.preventDefault(); setDropId(id) } },
    onDrop: (event: DragEvent) => { event.preventDefault(); if (dragId) prefs.move(dragId, id); setDragId(null); setDropId(null) },
    onDragEnd: () => { setDragId(null); setDropId(null) },
  })
  const anyRows = rows.length > 0 || machines.some((machine) => (deviceRows[machine.id] ?? []).length > 0)

  return <div className="pro-sidebar">
    <div className="pro-body">
      <div className="pro-head">
        <span>Chats</span>
        <span className="pro-head-tools">
          <button type="button" className="icon-button tiny" aria-pressed={flat} aria-label={flat ? 'Group chats by project' : 'Show one flat list'} title={flat ? 'Group by project' : 'One flat list'} onClick={() => { setFlat(!flat); write(FLAT_KEY, flat ? '0' : '1') }}><ListFilter size={14} aria-hidden="true" /></button>
          <button type="button" className="icon-button tiny" aria-haspopup={machines.length ? 'menu' : undefined} aria-label="New project" title={machines.length ? 'Add a project here or on another device' : 'New project'} onClick={openProjectMenu}><FolderOpen size={14} aria-hidden="true" /></button>
          <button type="button" className="icon-button tiny" aria-haspopup="menu" aria-label="New chat" title="New chat" onClick={(event) => openNew(event, null, true)}><Plus size={14} aria-hidden="true" /></button>
        </span>
      </div>
      {groups.filter((group) => group.sessions.length > 0).map((group) => {
        const isFolded = folded.has(group.key)
        return <section key={group.key} className="pro-group" aria-label={group.label || 'Chats'}>
          {!flat && <div className="pro-group-head">
            <button type="button" className="pro-group-fold" aria-expanded={!isFolded} onClick={() => toggleFold(group.key)}>
              <ChevronRight size={12} aria-hidden="true" className={`fold-chevron${isFolded ? '' : ' open'}`} />
              <ProjectIcon project={group.project} size={16} /><span className="pro-group-name">{group.label}</span>{isFolded && <span className="pro-count">{group.sessions.length}</span>}
            </button>
            <button type="button" className="icon-button tiny pro-group-add" aria-haspopup="menu" aria-label={`New chat in ${group.label}`} title="New chat here" onClick={(event) => openNew(event, group.project?.id ?? null)}><Plus size={13} aria-hidden="true" /></button>
          </div>}
          <Collapse open={flat || !isFolded}><ul role="tree" className="pro-list">{group.sessions.map((session) => renderItem(localItem(session)))}</ul></Collapse>
        </section>
      })}
      {!anyRows && <p className="pro-empty">No chats yet. Start one with the plus.</p>}

      {machines.length > 0 && <div className="pro-machines">
        <div className="pro-head pro-machines-label"><span>Other devices</span></div>
        {prefs.ordered.map((machine) => {
          const list = deviceRows[machine.id] ?? []
          const live = list.filter((row) => !isSettled(asSession(row), now, choices))
          const isFolded = devicesFolded.has(machine.id)
          const offline = machine.state !== 'online'
          const where: Where = { kind: whereKindOf(machine), name: machine.name, color: prefs.colorOf(machine.id) }
          return <section key={machine.id} className={`pro-group pro-device${dragId === machine.id ? ' dragging' : ''}${dropId === machine.id ? ' drop-target' : ''}`} aria-label={machine.name} {...dragProps(machine.id)}>
            <div className="pro-group-head">
              <button type="button" className="pro-group-fold" aria-expanded={!isFolded} onClick={() => toggleDevice(machine.id)} title={offline ? 'Shown as it was when last reached. Drag to reorder.' : machine.live ? 'Live. Drag to reorder.' : 'Checked every 20 seconds. Drag to reorder.'}>
                <ChevronRight size={12} aria-hidden="true" className={`fold-chevron${isFolded ? '' : ' open'}`} />
                <WhereIcon where={where} size={14} />
                <ProBadge tone={offline ? 'offline' : 'online'} label={offline ? 'Offline' : 'Online'} /><span className="pro-group-name pro-device-name">{machine.name}</span>
                <span className="pro-count">{live.length}</span>
              </button>
              <button type="button" className="icon-button tiny pro-group-add" aria-haspopup="menu" aria-label={`New chat or project on ${machine.name}`} title="New chat or project here" onClick={(event) => openDeviceNew(event, machine)}><Plus size={13} aria-hidden="true" /></button>
              <button type="button" className="icon-button tiny pro-group-add" aria-haspopup="dialog" aria-label={`Rename or colour ${machine.name}`} title="Rename or colour" onClick={(event) => { const rect = event.currentTarget.getBoundingClientRect(); setDeviceMenu({ machine, x: rect.left - 180, y: rect.bottom + 4 }) }}><MoreHorizontal size={13} aria-hidden="true" /></button>
            </div>
            <Collapse open={!isFolded}>
              <ul role="tree" className="pro-list">{live.map((row) => renderItem(deviceItem(machine, row)))}</ul>
              {live.length === 0 && <p className="pro-empty">{list.length > 0 ? 'Everything here is settled.' : offline ? 'Not reachable right now.' : 'No chats yet.'}</p>}
            </Collapse>
          </section>
        })}
      </div>}
    </div>

    {settledItems.length > 0 && <div className={`pro-settled pro-shelf${shelfShown ? ' open' : ''}`}>
      <button type="button" className="pro-settled-head" aria-expanded={shelfShown} onClick={() => setSettledOpen(!shelfShown)}>
        <span className="pro-shelf-icon" aria-hidden="true"><CheckCheck size={13} /></span><span className="pro-shelf-title">Settled</span><span className="pro-shelf-count">{settledItems.length}</span>
        <ChevronRight size={13} aria-hidden="true" className={`pro-shelf-chevron${shelfShown ? ' open' : ''}`} />
      </button>
      <Collapse open={shelfShown}>
        <label className="pro-shelf-search"><Search size={13} aria-hidden="true" /><input type="search" value={settledQuery} onChange={(event) => setSettledQuery(event.target.value)} onKeyDown={(event) => { if (event.key === 'Escape' && settledQuery) { event.stopPropagation(); setSettledQuery('') } }} placeholder="Search settled chats" aria-label="Search settled chats" spellCheck={false} /></label>
        <ul role="tree" className="pro-list pro-list-settled">{settledShown.map(renderItem)}</ul>
        {settledShown.length === 0 && <p className="pro-empty">No settled chat matches “{settledQuery.trim()}”.</p>}
      </Collapse>
    </div>}
    {menu && <TeamMenu label={menu.label} position={menu} items={menu.items} onClose={() => setMenu(null)} />}
    {deviceMenu && <DeviceMenu machine={deviceMenu.machine} color={prefs.colorOf(deviceMenu.machine.id)} position={deviceMenu} onColor={(color) => prefs.setColor(deviceMenu.machine.id, color)} onRenamed={() => devices?.onReload()} onClose={() => setDeviceMenu(null)} />}
  </div>
}
