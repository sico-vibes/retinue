// Importing conversations from other coding agents (owner, 14 November 2026;
// project-first and suggestion-first, 22 November 2026). Where from, which
// projects and conversations, who gets each one, then the import itself.
// Design: docs/research/conversation-import.md, sections 4 and 5.3. The daemon
// reads the files; nothing private (credentials, thinking, environment) is copied,
// and nothing is written until the owner confirms who gets what.
import { useEffect, useMemo, useRef, useState } from 'react'
import { Check, ChevronDown, ChevronRight, Download, FolderOpen, Search, Sparkles, User, Users } from 'lucide-react'
import type { Agent, ImportApplyResult, ImportProgress, ImportProject, ImportProjects, ImportRow, ImportScan, ImportSourceId, ImportSuggestion } from '../types'
import { listenForDaemonEvents, rpc } from '../tauri'
import { useDialogAccessibility } from './dialogFocus'
import { ProviderLogo, providerBrand } from './providerBrand'
import { Select } from './Select'
import './importDialog.css'

type Step = 'sources' | 'projects' | 'who' | 'review' | 'importing' | 'done'
type Who = 'one' | 'ceo' | 'distribute'
type Outcome = { imported: number; skipped: number; failed: number; sessions: string[]; to: Record<string, number>; project?: string | null }
/** Conversations per `import.apply` call; the daemon refuses more (`MAX_CONVERSATIONS`). */
export const IMPORT_BATCH = 50
export const IMPORT_WINDOWS = [30, 90, 365] as const
const key = (row: Pick<ImportRow, 'source' | 'conversationId'>) => `${row.source}:${row.conversationId}`
const message = (error: unknown, fallback: string) => error instanceof Error && error.message ? error.message : fallback
const day = (value: string | null) => { const at = value ? Date.parse(value) : NaN; return Number.isFinite(at) ? new Date(at).toLocaleDateString([], { day: 'numeric', month: 'short' }) : '' }
const size = (bytes: number) => bytes >= 1_000_000 ? `${(bytes / 1_000_000).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1000))} KB`
const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? '' : 's'}`
const windowLabel = (days: number) => days >= 365 ? 'the last year' : `the last ${days} days`
const pickable = (row: ImportRow) => !row.alreadyImported && !row.error

/** What a project card says about the folder: already a project here, a new one, or gone from this computer. */
export function projectStatus(project: Pick<ImportProject, 'existingProjectId' | 'exists'>): string {
  if (project.existingProjectId) return 'Already a project'
  return project.exists ? 'New project' : 'Folder missing'
}

/** Whether a conversation matches the search: its title, its first message or its folder. */
export function rowMatches(row: Pick<ImportRow, 'title' | 'firstUser' | 'projectPath'>, query: string): boolean {
  const needle = query.trim().toLowerCase()
  if (!needle) return true
  return [row.title, row.firstUser, row.projectPath].some((text) => text?.toLowerCase().includes(needle))
}

/** The suggestion step's choices, grouped into one `import.apply` call per employee (the CEO is ''). */
export function groupByChoice(items: readonly { source: ImportSourceId; conversationId: string }[], choices: Readonly<Record<string, string>>): Map<string, { source: ImportSourceId; conversationId: string }[]> {
  const groups = new Map<string, { source: ImportSourceId; conversationId: string }[]>()
  for (const item of items) {
    const who = choices[key(item)] ?? ''
    const list = groups.get(who) ?? []
    list.push({ source: item.source, conversationId: item.conversationId })
    groups.set(who, list)
  }
  return groups
}

/** The closing sentence: how many came in, and who has them. */
export function outcomeSentence(outcome: Outcome, nameOf: (id: string) => string): string {
  const parts = [`Imported ${plural(outcome.imported, 'conversation')}`]
  const to = Object.entries(outcome.to).filter(([, count]) => count > 0)
  if (to.length) parts[0] += `: ${to.map(([id, count]) => `${count} to ${nameOf(id)}`).join(', ')}`
  if (outcome.skipped) parts.push(`${outcome.skipped} already here or skipped`)
  if (outcome.failed) parts.push(`${outcome.failed} could not be read`)
  return `${parts.join('; ')}.`
}

export function ImportDialog({ agents, ceo, initialAgentId, onClose, onOpen }: { agents: readonly Agent[]; ceo: Agent | null; initialAgentId?: string | null; onClose: () => void; onOpen?: (sessionId: string) => void }) {
  const { ref, close } = useDialogAccessibility(onClose)
  const [step, setStep] = useState<Step>('sources')
  const [since, setSince] = useState<number>(30)
  const [scan, setScan] = useState<ImportScan | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [sources, setSources] = useState<Set<ImportSourceId>>(new Set())
  const [projects, setProjects] = useState<ImportProjects | null>(null)
  const [open, setOpen] = useState<Set<string>>(new Set())
  const [query, setQuery] = useState('')
  const [picked, setPicked] = useState<Set<string>>(new Set())
  const [who, setWho] = useState<Who>(initialAgentId ? 'one' : 'distribute')
  const [agentId, setAgentId] = useState<string>(initialAgentId ?? agents.find((agent) => agent.id !== ceo?.id && !agent.archived && !agent.hidden)?.id ?? '')
  const [suggestions, setSuggestions] = useState<ImportSuggestion[]>([])
  const [choices, setChoices] = useState<Record<string, string>>({})
  const [progress, setProgress] = useState<{ done: number; total: number; last?: string }>({ done: 0, total: 0 })
  const [outcome, setOutcome] = useState<Outcome | null>(null)
  const [busy, setBusy] = useState(false)
  const base = useRef(0)
  const bots = useMemo(() => agents.filter((agent) => !agent.archived && !agent.hidden && agent.id !== ceo?.id), [agents, ceo])
  const ceoName = ceo?.name ?? 'The CEO'
  const nameOf = (id: string) => !id || id === ceo?.id ? ceoName : agents.find((agent) => agent.id === id)?.name ?? 'an employee'

  useEffect(() => {
    let live = true
    setScan(null); setError(null)
    void rpc<ImportScan>('import.scan', { sinceDays: since }).then((reply) => { if (!live) return; setScan(reply); setSources(new Set(reply.sources.filter((source) => !source.error && source.recent > 0).map((source) => source.source))) }).catch((reason) => { if (live) setError(message(reason, 'Retinue could not look for conversations.')) })
    return () => { live = false }
  }, [since])

  useEffect(() => {
    let stop: (() => void) | null = null
    let live = true
    void listenForDaemonEvents((event) => {
      if (event.type !== 'import.progress') return
      const payload = (event.payload ?? {}) as unknown as ImportProgress
      if (payload.state === 'started') return
      setProgress((current) => ({ ...current, done: Math.max(current.done, base.current + (payload.index ?? 0)), last: payload.title ?? current.last }))
    }).then((unlisten) => { if (live) stop = unlisten; else unlisten() }).catch(() => undefined)
    return () => { live = false; stop?.() }
  }, [])

  const rows = useMemo(() => projects?.projects.flatMap((project) => project.conversations) ?? [], [projects])
  const chosen = useMemo(() => rows.filter((row) => picked.has(key(row))), [rows, picked])
  const shown = useMemo(() => (projects?.projects ?? []).map((project) => ({ project, list: project.conversations.filter((row) => rowMatches(row, query)) })).filter(({ project, list }) => list.length > 0 || rowMatches({ title: project.name, firstUser: '', projectPath: project.path }, query)), [projects, query])

  const loadProjects = async () => {
    setBusy(true); setError(null)
    try {
      const reply = await rpc<ImportProjects>('import.projects', { sinceDays: since, sources: [...sources] })
      setProjects(reply)
      setPicked(new Set())
      setOpen(new Set(reply.projects.length === 1 ? [reply.projects[0]!.key] : []))
      setStep('projects')
    } catch (reason) { setError(message(reason, 'The conversations could not be listed.')) } finally { setBusy(false) }
  }

  const apply = async (items: { source: ImportSourceId; conversationId: string }[], assign: Record<string, unknown>, startAt: number) => {
    let imported = 0, skipped = 0, failed = 0
    const sessions: string[] = []
    for (let index = 0; index < items.length; index += IMPORT_BATCH) {
      const batch = items.slice(index, index + IMPORT_BATCH)
      base.current = startAt + index
      const reply = await rpc<ImportApplyResult>('import.apply', { items: batch, assign, sinceDays: since })
      imported += reply.imported; skipped += reply.skipped; failed += reply.failed; sessions.push(...reply.sessions)
      setProgress((current) => ({ ...current, done: Math.max(current.done, base.current + batch.length) }))
    }
    return { imported, skipped, failed, sessions }
  }

  /** One employee, the CEO, or the suggestions the owner confirmed: one import call per person. */
  const start = async (groups: Map<string, { source: ImportSourceId; conversationId: string }[]>) => {
    const total = [...groups.values()].reduce((sum, list) => sum + list.length, 0)
    if (!total) return
    setStep('importing'); setError(null); setProgress({ done: 0, total }); base.current = 0
    const result: Outcome = { imported: 0, skipped: 0, failed: 0, sessions: [], to: {} }
    try {
      let done = 0
      for (const [person, items] of groups) {
        const assign = person ? { agentId: person } : { mode: 'ceo' }
        const reply = await apply(items, assign, done)
        result.imported += reply.imported; result.skipped += reply.skipped; result.failed += reply.failed; result.sessions.push(...reply.sessions)
        result.to[person] = (result.to[person] ?? 0) + reply.imported
        done += items.length
      }
      setOutcome(result); setStep('done')
    } catch (reason) {
      setError(message(reason, 'The import stopped.'))
      if (result.imported) { setOutcome(result); setStep('done') } else setStep('who')
    }
  }

  /** A whole folder in one go: the daemon makes the project, pages through it and gives each conversation to the employee its rule picks. */
  const importProject = async (project: ImportProject) => {
    setStep('importing'); setError(null); setProgress({ done: 0, total: project.total, last: project.name }); base.current = 0
    const result: Outcome = { imported: 0, skipped: 0, failed: 0, sessions: [], to: {}, project: project.name }
    try {
      let offset: number | null = 0
      while (offset !== null) {
        base.current = offset
        const reply: ImportApplyResult = await rpc<ImportApplyResult>('import.apply', { projectKey: project.key, offset, assign: { mode: 'project' }, sinceDays: since, sources: [...sources] })
        result.imported += reply.imported; result.skipped += reply.skipped; result.failed += reply.failed; result.sessions.push(...reply.sessions)
        const next: number | null = typeof reply.next === 'number' && reply.next > offset ? reply.next : null
        setProgress((current) => ({ ...current, done: Math.max(current.done, next ?? project.total) }))
        offset = next
      }
      setOutcome(result); setStep('done')
    } catch (reason) {
      setError(message(reason, 'The import stopped.'))
      if (result.imported) { setOutcome(result); setStep('done') } else setStep('projects')
    }
  }

  const suggest = async () => {
    setBusy(true); setError(null)
    try {
      const items = chosen.map((row) => ({ source: row.source, conversationId: row.conversationId }))
      const reply = await rpc<{ suggestions: ImportSuggestion[] }>('import.distribute', { items })
      setSuggestions(reply.suggestions)
      setChoices(Object.fromEntries(reply.suggestions.map((item) => [`${item.source}:${item.conversationId}`, item.agentId && bots.some((agent) => agent.id === item.agentId) ? item.agentId : ''])))
      setStep('review')
    } catch (reason) { setError(message(reason, 'Retinue could not suggest who gets them.')) } finally { setBusy(false) }
  }

  const next = () => {
    const items = chosen.map((row) => ({ source: row.source, conversationId: row.conversationId }))
    if (who === 'distribute') return void suggest()
    const person = who === 'one' ? agentId : ''
    return void start(new Map([[person, items]]))
  }

  const toggle = (id: string) => setPicked((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next })
  const toggleProject = (list: ImportRow[]) => setPicked((current) => {
    const ids = list.filter(pickable).map(key)
    const all = ids.length > 0 && ids.every((id) => current.has(id))
    const next = new Set(current)
    for (const id of ids) { if (all) next.delete(id); else next.add(id) }
    return next
  })
  const expand = (id: string) => setOpen((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next })
  const hires = [...new Map(suggestions.filter((item) => item.hire).map((item) => [item.source, item])).values()]
  const at = step === 'sources' ? 0 : step === 'projects' ? 1 : step === 'who' || step === 'review' ? 2 : 3

  return <div className="sheet-backdrop blob-dialog-backdrop is-entered">
    <section ref={ref as React.RefObject<HTMLElement>} className="blob-dialog import-conversations" role="dialog" aria-modal="true" aria-labelledby="import-conversations-title">
      <h2 id="import-conversations-title"><Download size={17} aria-hidden="true" />Import conversations</h2>
      <ol className="import-steps" aria-label="Steps">{(['Where from', 'Which ones', 'Who gets them', 'Import'] as const).map((label, index) =>
        <li key={label} data-state={index < at ? 'done' : index === at ? 'current' : 'next'}>{index < at ? <Check size={11} aria-hidden="true" /> : <span>{index + 1}</span>}{label}</li>)}</ol>

      {error && <p className="import-error" role="alert">{error}</p>}

      {step === 'sources' && <div className="import-body">
        <div className="import-window">
          <span>Found on this computer. Imports run here, only.</span>
          <Select ariaLabel="How far back" variant="muted" value={String(since)} onChange={(value) => setSince(Number(value))} options={IMPORT_WINDOWS.map((days) => ({ value: String(days), label: `From ${windowLabel(days)}` }))} />
        </div>
        {!scan && !error && <p className="import-note" role="status">Looking for conversations on this computer…</p>}
        {scan && scan.sources.length === 0 && <p className="import-note">No Claude Code, Codex or OpenCode conversations were found on this computer.</p>}
        {scan && scan.sources.length > 0 && <>
          <ul className="import-sources">{scan.sources.map((source) => <li key={source.source}>
            <label className={source.error || source.recent === 0 ? 'disabled' : undefined}>
              <input type="checkbox" checked={sources.has(source.source)} disabled={!!source.error || source.recent === 0} onChange={() => setSources((current) => { const next = new Set(current); if (next.has(source.source)) next.delete(source.source); else next.add(source.source); return next })} />
              <ProviderLogo provider={source.source} size={18} />
              <span className="import-source-copy"><strong>{source.label}</strong><small>{source.error ?? `${source.recent} from ${windowLabel(scan.sinceDays)} · ${source.conversations} in all · newest ${day(source.newest)}`}</small></span>
            </label>
          </li>)}</ul>
          <p className="import-note">Passwords, sign-ins and the agents' private reasoning are never copied.</p>
        </>}
        <div className="blob-dialog-actions"><button type="button" className="secondary-button" onClick={close}>Cancel</button><button type="button" className="primary-button" disabled={!sources.size || busy} onClick={() => void loadProjects()}>{busy ? 'Listing…' : 'Next'}</button></div>
      </div>}

      {step === 'projects' && projects && <div className="import-body">
        <label className="import-search"><Search size={14} aria-hidden="true" /><input type="search" placeholder="Search titles, first messages and folders" aria-label="Search conversations" value={query} onChange={(event) => setQuery(event.target.value)} /></label>
        {projects.errors.map((item) => <p key={item.source} className="import-note">{item.label}: {item.message}</p>)}
        {projects.projects.length === 0 && <p className="import-note">Nothing from {windowLabel(since)}. Choose a longer time on the first step.</p>}
        <div className="import-rows">{shown.map(({ project, list }) => {
          const expanded = open.has(project.key) || !!query.trim()
          const selectable = list.filter(pickable)
          const all = selectable.length > 0 && selectable.every((row) => picked.has(key(row)))
          const count = project.conversations.filter((row) => picked.has(key(row))).length
          const canWhole = !!project.existingProjectId || project.exists
          return <section key={project.key} className="import-project">
            <header>
              <button type="button" className="import-project-toggle" aria-expanded={expanded} onClick={() => expand(project.key)}>{expanded ? <ChevronDown size={14} aria-hidden="true" /> : <ChevronRight size={14} aria-hidden="true" />}<FolderOpen size={14} aria-hidden="true" /><span className="import-row-copy"><strong title={project.path}>{project.name}</strong><small>{plural(project.total, 'conversation')} · newest {day(project.newest)} · {size(project.bytes)}{count ? ` · ${count} chosen` : ''}</small></span></button>
              <span className="import-project-logos" aria-label={project.sources.map((source) => providerBrand(source).name).join(', ')}>{project.sources.map((source) => <ProviderLogo key={source} provider={source} size={14} />)}</span>
              <span className="import-badge">{projectStatus(project)}</span>
            </header>
            {expanded && <>
              <ul>{list.map((row) => <li key={key(row)}>
                <label className={pickable(row) ? undefined : 'disabled'}>
                  <input type="checkbox" checked={picked.has(key(row))} disabled={!pickable(row)} onChange={() => toggle(key(row))} />
                  <ProviderLogo provider={row.agent} size={14} />
                  <span className="import-row-copy"><strong>{row.title || row.firstUser || 'Untitled'}</strong><small>{day(row.updatedAt)} · {plural(row.messageCount, 'message')} · {size(row.bytes)}{row.archived ? ' · archived' : ''}</small></span>
                  <span className={`import-badge${row.canResumeWith ? ' resume' : ''}`}>{row.alreadyImported ? 'Imported' : row.error ? 'Unreadable' : row.canResumeWith ? `Can continue in ${row.agentLabel}` : 'Read only'}</span>
                </label>
              </li>)}</ul>
              {project.total > project.shown && <p className="import-note">{project.total - project.shown} more in this folder are not listed. Import all brings them in too.</p>}
              <div className="import-project-actions">
                {selectable.length > 0 && <button type="button" className="queue-link" onClick={() => toggleProject(list)}>{all ? 'Choose none' : 'Choose all listed'}</button>}
                <button type="button" className="secondary-button small" disabled={!canWhole} title={canWhole ? undefined : 'The folder is not on this computer any more, so it cannot be made a project.'} onClick={() => void importProject(project)}>Import all {project.total}</button>
              </div>
            </>}
          </section>
        })}</div>
        {projects.older > 0 && <p className="import-note">{plural(projects.older, 'older conversation')} {projects.older === 1 ? 'is' : 'are'} not shown. Choose a longer time on the first step to see {projects.older === 1 ? 'it' : 'them'}.</p>}
        <p className="import-note">Imported conversations are read-only, unless the agent that wrote them is still installed here and still has the file; then the right employee can carry on.</p>
        <div className="blob-dialog-actions"><button type="button" className="secondary-button" onClick={() => setStep('sources')}>Back</button><button type="button" className="primary-button" disabled={!chosen.length} onClick={() => setStep('who')}>Next · {chosen.length} chosen</button></div>
      </div>}

      {step === 'who' && <div className="import-body">
        <div className="import-who" role="radiogroup" aria-label="Who gets them">
          <button type="button" role="radio" aria-checked={who === 'distribute'} onClick={() => setWho('distribute')}><Sparkles size={16} aria-hidden="true" /><strong>Share them out</strong><small>Retinue suggests an employee for each one, by project, coding agent and role. You check them before anything is imported.</small></button>
          <button type="button" role="radio" aria-checked={who === 'one'} onClick={() => setWho('one')}><User size={16} aria-hidden="true" /><strong>One employee</strong><small>All {chosen.length} go to the employee you pick.</small></button>
          <button type="button" role="radio" aria-checked={who === 'ceo'} onClick={() => setWho('ceo')}><Users size={16} aria-hidden="true" /><strong>{ceoName}</strong><small>All of them go to the CEO, who knows every project.</small></button>
        </div>
        {who === 'one' && <label className="blob-field">Employee<select value={agentId} onChange={(event) => setAgentId(event.target.value)}>{bots.map((agent) => <option key={agent.id} value={agent.id}>{agent.name}{agent.role ? ` · ${agent.role}` : ''}</option>)}</select></label>}
        <div className="blob-dialog-actions"><button type="button" className="secondary-button" onClick={() => setStep('projects')}>Back</button><button type="button" className="primary-button" disabled={busy || (who === 'one' && !agentId)} onClick={next}>{who === 'distribute' ? busy ? 'Suggesting…' : 'Next' : `Import ${chosen.length}`}</button></div>
      </div>}

      {step === 'review' && <div className="import-body">
        <p className="import-note">Here's who each conversation would go to, and why. Change any of them, then import.</p>
        <ul className="import-review">{suggestions.map((item) => {
          const id = `${item.source}:${item.conversationId}`
          return <li key={id}>
            <ProviderLogo provider={item.source} size={14} />
            <span className="import-row-copy"><strong>{item.title || 'Untitled'}</strong><small>{item.detail}</small></span>
            <Select ariaLabel={`Who gets ${item.title || 'this conversation'}`} variant="muted" value={choices[id] ?? ''} onChange={(value) => setChoices((current) => ({ ...current, [id]: value }))} options={[{ value: '', label: ceoName }, ...bots.map((agent) => ({ value: agent.id, label: agent.name }))]} />
          </li>
        })}</ul>
        {hires.map((item) => <p key={item.source} className="import-hire">No employee uses {providerBrand(item.source).name} yet, so its conversations stay read-only. To carry them on, hire one in Team, such as {item.hire!.name}.</p>)}
        <div className="blob-dialog-actions"><button type="button" className="secondary-button" onClick={() => setStep('who')}>Back</button><button type="button" className="primary-button" onClick={() => void start(groupByChoice(suggestions.map((item) => ({ source: item.source as ImportSourceId, conversationId: item.conversationId })), choices))}>Import {suggestions.length}</button></div>
      </div>}

      {step === 'importing' && <div className="import-body" role="status">
        <div className="import-progress"><span style={{ width: `${progress.total ? Math.min(100, Math.round((progress.done / progress.total) * 100)) : 0}%` }} /></div>
        <p className="import-note">{Math.min(progress.done, progress.total)} of {progress.total} imported{progress.last ? ` · ${progress.last}` : ''}</p>
      </div>}

      {step === 'done' && outcome && <div className="import-body">
        <p className="import-done"><Check size={16} aria-hidden="true" />{outcomeSentence(outcome, nameOf)}</p>
        {outcome.project && <p className="import-note">They are in the project {outcome.project}, each with the employee Retinue's rule picked.</p>}
        <div className="blob-dialog-actions">{outcome.sessions[0] && onOpen && <button type="button" className="secondary-button" onClick={() => { onOpen(outcome.sessions[0]!); close() }}>Open the first one</button>}<button type="button" className="primary-button" onClick={close}>Done</button></div>
      </div>}
    </section>
  </div>
}
