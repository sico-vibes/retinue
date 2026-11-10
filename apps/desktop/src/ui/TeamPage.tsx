// The Team page (owner, 8 November 2026): bots are employees. "Your team" lists
// every employee with the projects they are hired into; "Hire" is the preset
// catalogue by department, plus "Create your own". Hiring goes through the
// daemon's `project.hire`; one employee can be hired into many projects.
import { useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Anchor, ArrowLeft, ChevronLeft, ChevronRight, GraduationCap, Hammer, Heart, ImageUp, Microscope, Minus, PartyPopper, Plus, Route, Search, Sparkles, X, Zap, type LucideIcon } from 'lucide-react'
import type { Agent, Project, Runtime, Session } from '../types'
import { rpc } from '../tauri'
import { BlobCanvas } from '../blob/BlobCanvas'
import { agentLook } from '../blob/look'
import { DEPARTMENTS, PRESETS, presetLook, presetMatches, presetsByDepartment, type Department, type Personality, type Preset, type SkillCard } from '../team/presets'
import personalityCatalogue from '../team/personalities.json'
import { PROFILE_PHOTOS, asPhoto, isUploadedPhoto, photoFromFile } from '../blob/photos'
import { usePhotoOwners } from './photoOwners'
import { roleIcon } from './roleIcon'
import { Select } from './Select'
import { labelizeEffort } from './effortLabels'
import { preferredConcreteModel } from './modelPreferences'
import { parseModelCatalog, type CatalogModel } from '../executionContract'
import { agentColorHex } from './agentColor'
import { ProjectIcon } from './ProjectIcon'
import { announceMembersChanged, useProjectMembers } from './projectMembers'
import { providerBrand } from './providerBrand'
import { runtimeUsable } from './rosterSelectors'
import { isManager, sessionIsBusy } from './teamSelectors'
import './teamPage.css'

export type TeamTab = 'team' | 'hire'

type HireTarget = { preset: Preset; agent?: undefined } | { agent: Agent; preset?: undefined }

type Catalogue = { departments: readonly Department[]; presets: readonly Preset[]; personalities: readonly Personality[] }
const LOCAL_PERSONALITIES: readonly Personality[] = personalityCatalogue.personalities

function usePresetCatalogue(refreshKey: unknown): Catalogue {
  const [value, setValue] = useState<Catalogue>({ departments: DEPARTMENTS, presets: PRESETS, personalities: LOCAL_PERSONALITIES })
  useEffect(() => {
    let live = true
    rpc<{ departments?: Department[]; presets?: Preset[]; personalities?: Personality[] }>('preset.list').then((result) => {
      if (!live || !result?.presets?.length) return
      // The daemon's list carries who is hired; the instructions stay local.
      const local = new Map(PRESETS.map((preset) => [preset.id, preset]))
      setValue({ personalities: result.personalities?.length ? result.personalities : LOCAL_PERSONALITIES, departments: result.departments?.length ? result.departments : DEPARTMENTS, presets: result.presets.map((preset) => ({ ...local.get(preset.id), ...preset, instructions: local.get(preset.id)?.instructions ?? [] })) })
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
  const { departments, presets, personalities } = usePresetCatalogue(`${agents.length}:${version}`)
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
            const RoleIcon = roleIcon(preset.role)
            return <article key={preset.id} className="team-card hire-card" style={{ ['--card-tint' as string]: agentColorHex(preset.color) }}>
              <span className="hire-card-art" aria-hidden="true"><RoleIcon strokeWidth={1.1} /></span>
              <div className="team-card-art"><BlobCanvas color={agentColorHex(preset.color)} size={76} look={presetLook(preset)} label={preset.nickname} decorative /></div>
              <div className="team-card-copy"><strong>{preset.nickname}</strong><span className="team-card-role">{preset.role}</span><p>{preset.description}</p></div>
              <div className="team-card-tags">{preset.tags.slice(0, 3).map((tag) => <span key={tag}>{tag}</span>)}</div>
              <div className="team-card-actions">
                {hired && <span className="team-card-hired">On your team</span>}
                <button type="button" className={hired ? 'secondary-button small' : 'primary-button small'} disabled={!connected || (!!hired && projects.length === 0)} title={hired && !projects.length ? 'Create a project first' : undefined} onClick={() => setHiring(hired ? { agent: hired } : { preset })}>{hired ? 'Hire into…' : 'Hire'}</button>
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
    {hiring && <HireDialog target={hiring} personalities={personalities} projects={projects} runtimes={runtimes} alreadyIn={hiring.agent ? new Set((projectsOf.get(hiring.agent.id) ?? []).map((project) => project.id)) : new Set()} initialProjectId={initialProjectId} onClose={() => setHiring(null)} onDone={() => { setHiring(null); announceMembersChanged(); setVersion((value) => value + 1); onChanged?.(); setTab('team') }} onError={onError} />}
  </div>
}

/** A personality's icon (`personalities.json` `icon`). */
const PERSONALITY_ICONS: Record<string, LucideIcon> = { anchor: Anchor, heart: Heart, zap: Zap, 'party-popper': PartyPopper, 'graduation-cap': GraduationCap, microscope: Microscope, sparkles: Sparkles, hammer: Hammer, route: Route, minus: Minus }
const LOCAL_BY_ID = new Map(LOCAL_PERSONALITIES.map((item) => [item.id, item]))

/** On/off switch, the app's own control (never a checkbox). */
function Toggle({ on, label, onChange, disabled = false }: { on: boolean; label: string; onChange: () => void; disabled?: boolean }) {
  return <button type="button" className={`toggle ${on ? 'on' : ''}`} role="switch" aria-checked={on} aria-label={label} disabled={disabled} onClick={onChange}><i /></button>
}

/**
 * Hiring (owner, 9 November 2026): a preset can be renamed, re-photographed, given a personality
 * (its prompt editable, with an example of how it talks), a coding agent with its model and effort,
 * and a set of skills, before anything is made. Hiring an existing employee only picks projects.
 */
function HireDialog({ target, personalities, projects, runtimes, alreadyIn, initialProjectId, onClose, onDone, onError }: {
  target: HireTarget
  personalities: readonly Personality[]
  projects: readonly Project[]
  runtimes: readonly Runtime[]
  alreadyIn: ReadonlySet<string>
  initialProjectId: string | null
  onClose: () => void
  onDone: () => void
  onError: (message: string) => void
}) {
  const preset = target.preset
  const name = preset?.nickname ?? target.agent?.name ?? 'this employee'
  const usable = runtimes.filter((runtime) => runtimeUsable(runtime))
  const [chosen, setChosen] = useState<ReadonlySet<string>>(() => new Set(initialProjectId && !alreadyIn.has(initialProjectId) ? [initialProjectId] : []))
  const [busy, setBusy] = useState(false)
  const dialog = useRef<HTMLElement>(null)
  useEffect(() => {
    dialog.current?.querySelector<HTMLElement>('input, button')?.focus()
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape' && !document.querySelector('.select-menu')) { event.preventDefault(); onClose() } }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [onClose])
  const toggleProject = (id: string) => setChosen((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next })

  // Who they are.
  const owners = usePhotoOwners()
  const [nickname, setNickname] = useState(preset?.nickname ?? '')
  const [photo, setPhoto] = useState<string>(preset && !owners.has(preset.photo) ? preset.photo : '')
  // How they talk. The daemon's list has no icon or example, so the shipped file fills them in.
  const cards = personalities.map((item) => ({ ...LOCAL_BY_ID.get(item.id), ...item, icon: item.icon ?? LOCAL_BY_ID.get(item.id)?.icon, example: item.example ?? LOCAL_BY_ID.get(item.id)?.example }))
  const [personalityId, setPersonalityId] = useState(preset?.personality ?? '')
  const shipped = cards.find((item) => item.id === personalityId)
  const [prompt, setPrompt] = useState(shipped?.prompt ?? '')
  const pickPersonality = (id: string) => { setPersonalityId(id); const next = cards.find((item) => item.id === id); if (next) setPrompt(next.prompt) }
  const edited = !!shipped && prompt.trim() !== shipped.prompt

  // What they run on: the coding agent, its model and effort, from the agent's verified model list.
  const [runtimeId, setRuntimeId] = useState(usable[0]?.id ?? '')
  const runtime = usable.find((item) => item.id === runtimeId) ?? null
  const [models, setModels] = useState<CatalogModel[]>([])
  const [modelsState, setModelsState] = useState<'idle' | 'loading' | 'error'>('idle')
  const [model, setModel] = useState<string>('')
  const [thinking, setThinking] = useState<string>('')
  useEffect(() => {
    if (!preset || !runtime) { setModels([]); return }
    let live = true
    setModelsState('loading'); setModels([]); setModel(''); setThinking('')
    rpc('runtime.models', { runtimeId: runtime.id }).then((raw) => {
      if (!live) return
      const catalog = parseModelCatalog(raw)
      if (!catalog || !catalog.models.length) { setModelsState('error'); return }
      setModels(catalog.models)
      setModel(preferredConcreteModel(runtime.provider ?? '', catalog.models)?.id ?? '')
      setModelsState('idle')
    }, () => { if (live) setModelsState('error') })
    return () => { live = false }
  }, [preset, runtime?.id, runtime?.provider])
  const selectedModel = models.find((item) => item.id === model) ?? null
  const efforts = selectedModel?.supportedThinking ?? []

  // What they know.
  const skillCards = (list: unknown[] | undefined): SkillCard[] => (list ?? []).map((item) => typeof item === 'string' ? { id: item, name: item.replace(/-/g, ' ').replace(/^./, (letter) => letter.toUpperCase()), description: '' } : item as SkillCard)
  const defaults = skillCards(preset?.skills?.default as unknown[])
  const optional = skillCards(preset?.skills?.optional as unknown[])
  const [skills, setSkills] = useState<ReadonlySet<string>>(() => new Set(defaults.map((skill) => skill.id)))
  const toggleSkill = (id: string) => setSkills((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next })

  const hire = async () => {
    setBusy(true)
    try {
      let agentId = target.agent?.id ?? null
      // A preset can be hired into no project; it then waits on the team.
      const targets: Array<string | null> = chosen.size ? [...chosen] : preset ? [null] : []
      for (const projectId of targets) {
        const options = preset && !agentId ? {
          name: nickname.trim() || preset.nickname,
          photo,
          personality: personalityId ? { id: edited ? 'custom' : personalityId, prompt: prompt.trim() } : null,
          skills: [...skills],
          ...(runtimeId ? { runtimeId } : {}),
          ...(model ? { model } : {}),
          ...(thinking ? { thinking } : {}),
        } : {}
        const params: Record<string, unknown> = agentId ? { projectId, agentId } : { projectId, presetId: preset!.id, ...options }
        const result = await rpc<{ agentId?: string }>('project.hire', params)
        agentId = result?.agentId ?? agentId
      }
      onDone()
    } catch (reason) {
      onError(`Couldn't hire ${nickname.trim() || name}: ${reason instanceof Error ? reason.message : String(reason)}`)
      setBusy(false)
    }
  }

  const tint = agentColorHex(preset?.color ?? target.agent?.color ?? 'mint')
  const projectList = <section className="hire-section">
    <h4>{preset ? 'Projects' : `Hire ${name} into`}{preset && <small>Optional</small>}</h4>
    {projects.length === 0 && <p className="hire-hint">No projects yet. {name} joins the team and can be added to a project later.</p>}
    <div className="hire-rows">{projects.map((project) => {
      const inside = alreadyIn.has(project.id)
      return <div key={project.id} className={`hire-row${inside ? ' inside' : ''}`}><ProjectIcon project={project} size={15} /><span className="hire-row-copy"><strong>{project.name}</strong>{inside && <small>Already here</small>}</span><Toggle on={inside || chosen.has(project.id)} disabled={inside} label={`Hire into ${project.name}`} onChange={() => toggleProject(project.id)} /></div>
    })}</div>
  </section>

  // Into the page root: the Team page sits in its own layer, which the sidebars would otherwise draw over.
  return createPortal(<div className="team-dialog-backdrop" onPointerDown={(event) => { if (event.target === event.currentTarget) onClose() }}>
    <section ref={dialog} className={`team-dialog${preset ? ' hire-dialog' : ''}`} role="dialog" aria-modal="true" aria-label={`Hire ${nickname.trim() || name}`} style={{ ['--hire-tint' as string]: tint }}>
      <header className="hire-head">
        {preset ? <BlobCanvas color={tint} size={72} look={presetLook({ photo })} label={name} decorative /> : <BlobCanvas decorative color={tint} size={56} look={agentLook(target.agent!)} label={name} />}
        <div className="hire-head-copy"><span className="hire-eyebrow">{preset ? 'Hire' : 'Add to projects'}</span><strong>{nickname.trim() || name}</strong><span>{preset?.role ?? target.agent?.role ?? ''}</span></div>
        <button type="button" className="icon-button tiny" aria-label="Close" onClick={onClose}><X size={15} /></button>
      </header>
      <div className="hire-body">
        {preset && <>
          <section className="hire-section">
            <h4>Name and photo</h4>
            <input className="text-input hire-name" aria-label="Name" value={nickname} maxLength={40} onChange={(event) => setNickname(event.target.value)} />
            <HirePhotoPicker value={photo} color={tint} onChange={setPhoto} />
          </section>
          <section className="hire-section">
            <h4>Personality</h4>
            <div className="hire-personalities" role="radiogroup" aria-label="Personality">{cards.map((item) => {
              const Icon = PERSONALITY_ICONS[item.icon ?? ''] ?? Sparkles
              const on = personalityId === item.id
              return <button key={item.id} type="button" role="radio" aria-checked={on} className={`hire-personality${on ? ' on' : ''}`} title={item.summary} onClick={() => pickPersonality(item.id)}><Icon size={20} strokeWidth={1.8} aria-hidden="true" /><span>{item.name}</span></button>
            })}</div>
            {shipped && <div className="hire-example" aria-label={`How ${nickname.trim() || name} would reply`}>
              <p className="hire-example-question">{personalityCatalogue.exampleQuestion}</p>
              <div className="hire-example-reply"><BlobCanvas decorative color={tint} size={28} look={presetLook({ photo })} label={name} /><p>{shipped.example}</p></div>
            </div>}
            <label className="hire-prompt-label"><span>How they communicate{edited && <em> · edited, saved as custom</em>}</span>
              <textarea className="hire-prompt" rows={4} maxLength={2000} value={prompt} onChange={(event) => setPrompt(event.target.value)} />
            </label>
          </section>
          <section className="hire-section">
            <h4>Coding agent</h4>
            <div className="hire-brain">
              <label><span>Agent</span><Select ariaLabel="Coding agent" variant="field" align="left" value={runtimeId} disabled={usable.length === 0} placeholder="No coding agent" onChange={setRuntimeId} options={usable.map((item) => ({ value: item.id, label: providerBrand(item.provider ?? '').name, provider: item.provider }))} /></label>
              <label><span>Model</span>{modelsState === 'loading'
                ? <div className="hire-loading" role="status">Loading models…</div>
                : <Select ariaLabel="Model" variant="field" align="left" searchable={8} searchPlaceholder="Search models" placeholder={modelsState === 'error' ? 'Default model' : 'Choose a model'} value={model} disabled={models.length === 0} onChange={(value) => { setModel(value); setThinking('') }} options={models.map((item, index) => ({ value: item.id, label: item.displayName, ...(item.group && item.group !== models[index - 1]?.group ? { group: item.group } : {}) }))} />}</label>
              <label><span>Effort</span><Select ariaLabel="Effort" variant="field" align="left" value={thinking} disabled={efforts.length === 0} onChange={setThinking} options={[{ value: '', label: selectedModel?.defaultThinking ? `Default (${labelizeEffort(selectedModel.defaultThinking, runtime?.provider ?? '')})` : 'Default' }, ...efforts.map((level) => ({ value: level, label: labelizeEffort(level, runtime?.provider ?? '') }))]} /></label>
            </div>
            {modelsState === 'error' && <p className="hire-hint">The model list couldn't be checked, so {nickname.trim() || name} starts on the agent's default model.</p>}
          </section>
          {(defaults.length > 0 || optional.length > 0) && <section className="hire-section">
            <h4>Skills <small>{skills.size} on</small></h4>
            <div className="hire-rows">{[...defaults.map((skill) => ({ skill, suggested: false })), ...optional.map((skill) => ({ skill, suggested: true }))].map(({ skill, suggested }) => <div key={skill.id} className="hire-row">
              <span className="hire-row-copy"><strong>{skill.name}{suggested && <small className="hire-optional">Optional</small>}</strong>{skill.description && <em>{skill.description}</em>}</span>
              <Toggle on={skills.has(skill.id)} label={`Use the ${skill.name} skill`} onChange={() => toggleSkill(skill.id)} />
            </div>)}</div>
          </section>}
        </>}
        {projectList}
        {preset && <p className="hire-hint">{nickname.trim() || name} starts with Ask before acting. You can change access and instructions later in their settings.</p>}
      </div>
      <footer className="hire-foot">
        <button type="button" className="ghost-button small" onClick={onClose}>Cancel</button>
        <button type="button" className="primary-button small" disabled={busy || (!preset && chosen.size === 0) || (!!preset && !nickname.trim())} onClick={() => void hire()}>{busy ? 'Hiring…' : chosen.size > 1 ? `Hire into ${chosen.size} projects` : `Hire ${nickname.trim() || name}`}</button>
      </footer>
    </section>
  </div>, document.body)
}

/** The hire dialog's photo choice: Upload first, then the portraits nobody wears, a page at a time. */
function HirePhotoPicker({ value, color, onChange }: { value: string; color: string; onChange: (photo: string) => void }) {
  const owners = usePhotoOwners()
  const upload = useRef<HTMLInputElement>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const free = PROFILE_PHOTOS.filter((photo) => !owners.has(photo.id))
  const PER = 12
  const cells: Array<'upload' | 'none' | (typeof free)[number]> = ['upload', 'none', ...free]
  const pages = Math.max(1, Math.ceil(cells.length / PER))
  const [page, setPage] = useState(() => Math.max(0, Math.floor(Math.max(0, cells.findIndex((cell) => typeof cell === 'object' && cell.id === value)) / PER)))
  const shown = cells.slice(page * PER, page * PER + PER)
  const uploaded = isUploadedPhoto(asPhoto(value)) ? value : null
  return <div className="hire-photos-wrap">
    <div className="hire-photos" role="radiogroup" aria-label="Photo">
      {shown.map((cell) => cell === 'upload'
        ? <button key="upload" type="button" className={`hire-photo upload${uploaded ? ' on' : ''}`} aria-label="Upload your own photo" title="Upload your own photo" onClick={() => upload.current?.click()}>{uploaded ? <img src={uploaded} alt="" /> : <ImageUp size={20} aria-hidden="true" />}</button>
        : cell === 'none'
          ? <button key="none" type="button" role="radio" aria-checked={!value} className={`hire-photo${!value ? ' on' : ''}`} aria-label="No photo" title="No photo" onClick={() => onChange('')}><BlobCanvas decorative color={color} size={52} look={{ shape: 'circle' }} label="No photo" /></button>
          : <button key={cell.id} type="button" role="radio" aria-checked={value === cell.id} className={`hire-photo${value === cell.id ? ' on' : ''}`} aria-label={`Photo ${cell.id.replace('photo-', '')}`} onClick={() => onChange(cell.id)}><img src={cell.url} alt="" /></button>)}
    </div>
    {pages > 1 && <div className="profile-photo-pager"><button type="button" className="icon-button tiny" aria-label="Previous photos" disabled={page === 0} onClick={() => setPage(page - 1)}><ChevronLeft size={14} /></button><span className="profile-photo-dots">{Array.from({ length: pages }, (_, index) => <button key={index} type="button" aria-label={`Photos page ${index + 1}`} aria-current={index === page ? 'page' : undefined} className={index === page ? 'on' : undefined} onClick={() => setPage(index)} />)}</span><button type="button" className="icon-button tiny" aria-label="More photos" disabled={page >= pages - 1} onClick={() => setPage(page + 1)}><ChevronRight size={14} /></button></div>}
    <input ref={upload} type="file" accept="image/png,image/jpeg,image/webp,image/gif" hidden onChange={(event) => { const file = event.target.files?.[0]; event.target.value = ''; if (!file) return; setProblem(null); void photoFromFile(file).then((next) => onChange(String(next)), (error: unknown) => setProblem(error instanceof Error ? error.message : 'That picture could not be read.')) }} />
    {problem && <p className="hire-hint" role="alert">{problem}</p>}
  </div>
}
