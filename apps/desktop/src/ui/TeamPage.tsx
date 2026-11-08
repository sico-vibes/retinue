// The Team page (owner, 8 November 2026): bots are employees. "Your team" lists
// every employee with the projects they are hired into; "Hire" is the preset
// catalogue by department, plus "Create your own". Hiring goes through the
// daemon's `project.hire`; one employee can be hired into many projects.
import { useEffect, useMemo, useRef, useState } from 'react'
import { ArrowLeft, Plus, Search, X } from 'lucide-react'
import type { Agent, Project, Runtime, Session } from '../types'
import { rpc } from '../tauri'
import { BlobCanvas } from '../blob/BlobCanvas'
import { HazeMark } from '../blob/haze/HazeMark'
import type { HazeIcon } from '../blob/haze/icons'
import { agentLook } from '../blob/look'
import { DEPARTMENTS, PRESETS, presetMatches, presetsByDepartment, type Department, type Preset } from '../team/presets'
import { agentColorHex } from './agentColor'
import { ProjectIcon } from './ProjectIcon'
import { announceMembersChanged, useProjectMembers } from './projectMembers'
import { providerBrand } from './providerBrand'
import { runtimeUsable } from './rosterSelectors'
import { isManager, sessionIsBusy } from './teamSelectors'
import './teamPage.css'

export type TeamTab = 'team' | 'hire'

type HireTarget = { preset: Preset; agent?: undefined } | { agent: Agent; preset?: undefined }

function usePresetCatalogue(refreshKey: unknown): { departments: readonly Department[]; presets: readonly Preset[] } {
  const [value, setValue] = useState<{ departments: readonly Department[]; presets: readonly Preset[] }>({ departments: DEPARTMENTS, presets: PRESETS })
  useEffect(() => {
    let live = true
    rpc<{ departments?: Department[]; presets?: Preset[] }>('preset.list').then((result) => {
      if (!live || !result?.presets?.length) return
      // The daemon's list carries who is hired; the instructions stay local.
      const local = new Map(PRESETS.map((preset) => [preset.id, preset]))
      setValue({ departments: result.departments?.length ? result.departments : DEPARTMENTS, presets: result.presets.map((preset) => ({ ...local.get(preset.id), ...preset, instructions: local.get(preset.id)?.instructions ?? [] })) })
    }, () => undefined)
    return () => { live = false }
  }, [refreshKey])
  return value
}

export function TeamPage({ agents, projects, sessions, runtimes, connected, initialTab = 'team', initialProjectId = null, onBack, onOpenAgent, onEditAgent, onCreateOwn, onError, onChanged }: {
  agents: readonly Agent[]
  projects: readonly Project[]
  sessions: readonly Session[]
  runtimes: readonly Runtime[]
  connected: boolean
  initialTab?: TeamTab
  /** Hiring opened from a project preselects it. */
  initialProjectId?: string | null
  onBack: () => void
  onOpenAgent: (agent: Agent) => void
  onEditAgent: (agent: Agent) => void
  onCreateOwn: (projectId: string | null) => void
  onError: (message: string) => void
  /** After a hire or release: reload agents, so a new employee appears. */
  onChanged?: () => void
}) {
  const [tab, setTab] = useState<TeamTab>(initialTab)
  const [query, setQuery] = useState('')
  const [hiring, setHiring] = useState<HireTarget | null>(null)
  const [version, setVersion] = useState(0)
  useEffect(() => setTab(initialTab), [initialTab])
  const projectIds = useMemo(() => projects.map((project) => project.id), [projects])
  const members = useProjectMembers(projectIds, `${agents.length}:${version}`)
  const { departments, presets } = usePresetCatalogue(`${agents.length}:${version}`)
  const employees = useMemo(() => agents.filter((agent) => !agent.archived && !agent.parentAgentId && !isManager(agent)).sort((a, b) => a.name.localeCompare(b.name)), [agents])
  const projectsOf = useMemo(() => {
    const map = new Map<string, Project[]>()
    for (const project of projects) for (const member of members?.get(project.id) ?? []) map.set(member.agentId, [...(map.get(member.agentId) ?? []), project])
    if (!members) for (const agent of employees) { const own = projects.find((project) => project.id === agent.projectId); if (own) map.set(agent.id, [own]) }
    return map
  }, [members, projects, employees])
  const hiredPreset = (preset: Preset) => preset.hiredAgentId ? agents.find((agent) => agent.id === preset.hiredAgentId && !agent.archived) ?? null : agents.find((agent) => agent.presetId === preset.id && !agent.archived) ?? null
  const shownEmployees = employees.filter((agent) => !query.trim() || [agent.name, agent.role ?? '', agent.description ?? ''].some((value) => String(value).toLowerCase().includes(query.trim().toLowerCase())))
  const groups = presetsByDepartment(presets.filter((preset) => presetMatches(preset, query, departments)), departments)
  const backRef = useRef<HTMLButtonElement>(null)
  useEffect(() => { backRef.current?.focus({ focusVisible: false } as FocusOptions) }, [])
  const release = async (agent: Agent, project: Project) => {
    try { await rpc('project.release', { projectId: project.id, agentId: agent.id }); announceMembersChanged(); setVersion((value) => value + 1); onChanged?.() } catch (reason) { onError(`Couldn't release ${agent.name} from ${project.name}: ${reason instanceof Error ? reason.message : String(reason)}`) }
  }

  return <div className="team-page" data-team-tab={tab}>
    <header className="blob-page-header">
      <button ref={backRef} type="button" className="analytics-icon-button" aria-label="Back" title="Back" onClick={onBack}><ArrowLeft size={16} /></button>
      <strong className="blob-page-heading">Team</strong>
      <div className="segmented team-tabs" role="tablist" aria-label="Team">
        <button type="button" role="tab" aria-selected={tab === 'team'} className={tab === 'team' ? 'active' : ''} onClick={() => setTab('team')}>Your team<span className="team-tab-count">{employees.length}</span></button>
        <button type="button" role="tab" aria-selected={tab === 'hire'} className={tab === 'hire' ? 'active' : ''} onClick={() => setTab('hire')}>Hire</button>
      </div>
    </header>
    <div className="team-scroll"><div className="team-column">
      <label className="team-search"><Search size={14} aria-hidden="true" /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={tab === 'hire' ? 'Search roles, skills or names' : 'Search your team'} aria-label={tab === 'hire' ? 'Search presets' : 'Search your team'} />{query && <button type="button" className="icon-button tiny" aria-label="Clear search" onClick={() => setQuery('')}><X size={13} /></button>}</label>

      {tab === 'team' && <section className="team-roster" aria-label="Your team">
        {shownEmployees.length === 0 && <div className="team-empty"><p>{employees.length ? 'No one matches that search.' : 'No one on your team yet.'}</p>{!employees.length && <button type="button" className="primary-button small" onClick={() => setTab('hire')}>Hire your first employee</button>}</div>}
        {shownEmployees.map((agent) => {
          const working = sessions.some((session) => session.agentId === agent.id && sessionIsBusy(session))
          const inProjects = projectsOf.get(agent.id) ?? []
          return <article key={agent.id} className="team-person">
            <button type="button" className="team-person-main" onClick={() => onOpenAgent(agent)} title={`Open ${agent.name}`}>
              <BlobCanvas decorative color={agentColorHex(agent.color)} size={44} mini mood={working ? 'working' : 'idle'} look={agentLook(agent)} label={agent.name} />
              <span className="team-person-copy"><strong>{agent.name}</strong><span>{agent.role?.trim() || agent.description?.trim() || 'Employee'}</span></span>
              <span className={`team-person-state${working ? ' working' : ''}`}>{working ? 'Working' : 'Idle'}</span>
            </button>
            <div className="team-person-projects">
              {inProjects.map((project) => <span key={project.id} className="team-chip"><ProjectIcon project={project} size={12} />{project.name}<button type="button" aria-label={`Release ${agent.name} from ${project.name}`} title="Release from this project" disabled={!connected} onClick={() => void release(agent, project)}><X size={11} /></button></span>)}
              <button type="button" className="team-chip add" disabled={!connected || projects.length === 0} onClick={() => setHiring({ agent })}><Plus size={12} aria-hidden="true" />Hire into a project</button>
              <button type="button" className="ghost-button small team-person-settings" onClick={() => onEditAgent(agent)}>Settings</button>
            </div>
          </article>
        })}
      </section>}

      {tab === 'hire' && <section className="team-catalogue" aria-label="Hire">
        {groups.map(({ department, presets: list }) => <section key={department.id} className="team-department" aria-label={department.name}>
          <h3>{department.name}</h3>
          <div className="team-cards">{list.map((preset) => {
            const hired = hiredPreset(preset)
            return <article key={preset.id} className="team-card">
              <div className="team-card-art"><HazeMark color={agentColorHex(preset.color)} size={56} look={{ shape: 'circle', icon: preset.portrait as HazeIcon }} label={preset.nickname} decorative /></div>
              <div className="team-card-copy"><strong>{preset.nickname}</strong><span className="team-card-role">{preset.role}</span><p>{preset.description}</p></div>
              <div className="team-card-tags">{preset.tags.slice(0, 3).map((tag) => <span key={tag}>{tag}</span>)}</div>
              <div className="team-card-actions">
                {hired && <span className="team-card-hired">On your team</span>}
                <button type="button" className={hired ? 'secondary-button small' : 'primary-button small'} disabled={!connected || projects.length === 0} title={projects.length ? undefined : 'Create a project first'} onClick={() => setHiring(hired ? { agent: hired } : { preset })}>{hired ? 'Hire into…' : 'Hire'}</button>
              </div>
            </article>
          })}</div>
        </section>)}
        <section className="team-department" aria-label="Your own">
          <h3>Someone else</h3>
          <div className="team-cards"><button type="button" className="team-card team-card-create" disabled={!connected} onClick={() => onCreateOwn(initialProjectId)}><Plus size={22} aria-hidden="true" /><strong>Create your own</strong><span>Give a new employee a name, a portrait, a role and instructions.</span></button></div>
        </section>
      </section>}
    </div></div>
    {hiring && <HireDialog target={hiring} projects={projects} runtimes={runtimes} alreadyIn={hiring.agent ? new Set((projectsOf.get(hiring.agent.id) ?? []).map((project) => project.id)) : new Set()} initialProjectId={initialProjectId} onClose={() => setHiring(null)} onDone={() => { setHiring(null); announceMembersChanged(); setVersion((value) => value + 1); onChanged?.(); setTab('team') }} onError={onError} />}
  </div>
}

function HireDialog({ target, projects, runtimes, alreadyIn, initialProjectId, onClose, onDone, onError }: {
  target: HireTarget
  projects: readonly Project[]
  runtimes: readonly Runtime[]
  alreadyIn: ReadonlySet<string>
  initialProjectId: string | null
  onClose: () => void
  onDone: () => void
  onError: (message: string) => void
}) {
  const name = target.preset?.nickname ?? target.agent?.name ?? 'this employee'
  const usable = runtimes.filter((runtime) => runtimeUsable(runtime))
  const [chosen, setChosen] = useState<ReadonlySet<string>>(() => new Set(initialProjectId && !alreadyIn.has(initialProjectId) ? [initialProjectId] : []))
  const [runtimeId, setRuntimeId] = useState(usable[0]?.id ?? '')
  const [busy, setBusy] = useState(false)
  const dialog = useRef<HTMLElement>(null)
  useEffect(() => {
    dialog.current?.querySelector<HTMLElement>('input, select, button')?.focus()
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') { event.preventDefault(); onClose() } }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [onClose])
  const toggle = (id: string) => setChosen((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next })
  const hire = async () => {
    setBusy(true)
    try {
      let agentId = target.agent?.id ?? null
      for (const projectId of chosen) {
        const params: Record<string, unknown> = agentId ? { projectId, agentId } : { projectId, presetId: target.preset!.id, ...(runtimeId ? { runtimeId } : {}) }
        const result = await rpc<{ agentId?: string }>('project.hire', params)
        agentId = result?.agentId ?? agentId
      }
      onDone()
    } catch (reason) {
      onError(`Couldn't hire ${name}: ${reason instanceof Error ? reason.message : String(reason)}`)
      setBusy(false)
    }
  }
  return <div className="team-dialog-backdrop" onPointerDown={(event) => { if (event.target === event.currentTarget) onClose() }}>
    <section ref={dialog} className="team-dialog" role="dialog" aria-modal="true" aria-label={`Hire ${name}`}>
      <header className="team-dialog-head">
        {target.preset ? <HazeMark color={agentColorHex(target.preset.color)} size={40} look={{ shape: 'circle', icon: target.preset.portrait as HazeIcon }} label={name} decorative /> : <BlobCanvas decorative color={agentColorHex(target.agent!.color)} size={40} mini look={agentLook(target.agent!)} label={name} />}
        <div><strong>Hire {name}</strong><span>{target.preset?.role ?? target.agent?.role ?? ''}</span></div>
        <button type="button" className="icon-button tiny" aria-label="Close" onClick={onClose}><X size={15} /></button>
      </header>
      <fieldset className="team-dialog-projects">
        <legend>Into which projects?</legend>
        {projects.map((project) => {
          const inside = alreadyIn.has(project.id)
          return <label key={project.id} className={inside ? 'inside' : ''}><input type="checkbox" checked={inside || chosen.has(project.id)} disabled={inside} onChange={() => toggle(project.id)} /><ProjectIcon project={project} size={14} /><span>{project.name}</span>{inside && <small>Already here</small>}</label>
        })}
      </fieldset>
      {target.preset && usable.length > 1 && <label className="team-dialog-field"><span>Coding agent</span><select value={runtimeId} onChange={(event) => setRuntimeId(event.target.value)}>{usable.map((runtime) => <option key={runtime.id} value={runtime.id}>{providerBrand(runtime.provider ?? '').name}</option>)}</select></label>}
      {target.preset && <p className="team-dialog-note">{name} starts with Ask before acting. You can change the model, access and instructions in their settings.</p>}
      <footer className="team-dialog-actions">
        <button type="button" className="ghost-button small" onClick={onClose}>Cancel</button>
        <button type="button" className="primary-button small" disabled={busy || chosen.size === 0} onClick={() => void hire()}>{busy ? 'Hiring…' : chosen.size > 1 ? `Hire into ${chosen.size} projects` : 'Hire'}</button>
      </footer>
    </section>
  </div>
}
