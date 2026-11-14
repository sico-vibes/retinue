// Importing conversations from other coding agents (owner, 14 November 2026):
// where from, which ones, who gets them, the import itself, and what happened.
// Design: docs/research/owner-requests-2026-11-14.md, section 2. The daemon reads
// the files; nothing private (credentials, thinking, environment) is copied.
import { useEffect, useMemo, useState } from 'react'
import { Check, Download, FolderOpen, Sparkles, User, Users } from 'lucide-react'
import type { Agent, ImportApplyResult, ImportPreview, ImportProgress, ImportRow, ImportScan, ImportSourceId, ImportSuggestion } from '../types'
import { listenForDaemonEvents, rpc } from '../tauri'
import { useDialogAccessibility } from './dialogFocus'
import { BlobCanvas } from '../blob/BlobCanvas'
import { agentLook } from '../blob/look'
import { agentColorHex } from './agentColor'
import { ProviderLogo } from './providerBrand'
import './importDialog.css'

type Step = 'sources' | 'rows' | 'who' | 'importing' | 'review' | 'done'
type Who = 'one' | 'ceo' | 'distribute'
const key = (row: Pick<ImportRow, 'source' | 'conversationId'>) => `${row.source}:${row.conversationId}`
const message = (error: unknown, fallback: string) => error instanceof Error && error.message ? error.message : fallback
const day = (value: string | null) => { const at = value ? Date.parse(value) : NaN; return Number.isFinite(at) ? new Date(at).toLocaleDateString([], { day: 'numeric', month: 'short' }) : '' }
const size = (bytes: number) => bytes >= 1_000_000 ? `${(bytes / 1_000_000).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1000))} KB`
const folderName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path

export function ImportDialog({ agents, ceo, initialAgentId, onClose, onOpen }: { agents: readonly Agent[]; ceo: Agent | null; initialAgentId?: string | null; onClose: () => void; onOpen?: (sessionId: string) => void }) {
  const { ref, close } = useDialogAccessibility(onClose)
  const [step, setStep] = useState<Step>('sources')
  const [scan, setScan] = useState<ImportScan | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [sources, setSources] = useState<Set<ImportSourceId>>(new Set())
  const [rows, setRows] = useState<ImportRow[]>([])
  const [picked, setPicked] = useState<Set<string>>(new Set())
  const [who, setWho] = useState<Who>(initialAgentId ? 'one' : 'distribute')
  const [agentId, setAgentId] = useState<string>(initialAgentId ?? agents.find((agent) => agent.id !== ceo?.id)?.id ?? '')
  const [progress, setProgress] = useState<{ done: number; total: number; last?: string }>({ done: 0, total: 0 })
  const [result, setResult] = useState<ImportApplyResult | null>(null)
  const [suggestions, setSuggestions] = useState<ImportSuggestion[]>([])
  const [choices, setChoices] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState(false)
  const bots = useMemo(() => agents.filter((agent) => !agent.archived && !agent.hidden && agent.id !== ceo?.id), [agents, ceo])
  const nameOf = (id: string | null | undefined) => !id ? (ceo?.name ?? 'The CEO') : agents.find((agent) => agent.id === id)?.name ?? 'An employee'

  useEffect(() => {
    let live = true
    void rpc<ImportScan>('import.scan', { sinceDays: 30 }).then((reply) => { if (!live) return; setScan(reply); setSources(new Set(reply.sources.filter((source) => !source.error && source.recent > 0).map((source) => source.source))) }).catch((reason) => { if (live) setError(message(reason, 'Retinue could not look for conversations.')) })
    return () => { live = false }
  }, [])

  useEffect(() => {
    let stop: (() => void) | null = null
    let live = true
    void listenForDaemonEvents((event) => {
      if (event.type !== 'import.progress') return
      const payload = (event.payload ?? {}) as unknown as ImportProgress
      if (payload.state === 'started') return
      setProgress((current) => ({ done: Math.max(current.done, (payload.index ?? 0) + 1), total: payload.total ?? current.total, last: payload.title }))
    }).then((unlisten) => { if (live) stop = unlisten; else unlisten() }).catch(() => undefined)
    return () => { live = false; stop?.() }
  }, [])

  const loadRows = async () => {
    setBusy(true); setError(null)
    try {
      const pages = await Promise.all([...sources].map((source) => rpc<ImportPreview>('import.preview', { source, offset: 0, limit: 50, sinceDays: 30 })))
      const all = pages.flatMap((page) => page.rows).sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
      setRows(all)
      setPicked(new Set(all.filter((row) => !row.alreadyImported && !row.error).map(key)))
      setStep('rows')
    } catch (reason) { setError(message(reason, 'The conversations could not be listed.')) } finally { setBusy(false) }
  }

  const start = async () => {
    const items = rows.filter((row) => picked.has(key(row))).map((row) => ({ source: row.source, conversationId: row.conversationId }))
    if (!items.length) return
    setStep('importing'); setError(null); setProgress({ done: 0, total: items.length })
    try {
      const assign = who === 'one' ? { agentId } : who === 'ceo' ? { agentId: ceo?.id ?? null } : {}
      const reply = await rpc<ImportApplyResult>('import.apply', { items, assign, sinceDays: 30 })
      setResult(reply)
      if (who === 'distribute' && reply.sessions.length) {
        const proposed = await rpc<{ suggestions: ImportSuggestion[] }>('import.distribute', { ids: reply.sessions })
        setSuggestions(proposed.suggestions)
        setChoices(Object.fromEntries(proposed.suggestions.map((item) => [item.sessionId, item.agentId ?? ''])))
        setStep('review')
      } else setStep('done')
    } catch (reason) { setError(message(reason, 'The import stopped.')); setStep('who') }
  }

  const confirmDistribution = async () => {
    setBusy(true); setError(null)
    try {
      for (const item of suggestions) await rpc('import.assign', { sessionId: item.sessionId, agentId: choices[item.sessionId] || ceo?.id || null })
      setStep('done')
    } catch (reason) { setError(message(reason, 'Some conversations could not be given out.')) } finally { setBusy(false) }
  }

  const groups = useMemo(() => {
    const map = new Map<string, ImportRow[]>()
    for (const row of rows) { const list = map.get(row.projectPath) ?? []; list.push(row); map.set(row.projectPath, list) }
    return [...map.entries()]
  }, [rows])
  const pickedCount = rows.filter((row) => picked.has(key(row))).length
  const toggle = (id: string) => setPicked((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); return next })
  const toggleGroup = (list: ImportRow[]) => setPicked((current) => {
    const ids = list.filter((row) => !row.alreadyImported && !row.error).map(key)
    const all = ids.every((id) => current.has(id))
    const next = new Set(current)
    for (const id of ids) { if (all) next.delete(id); else next.add(id) }
    return next
  })
  const Avatar = ({ agent }: { agent: Agent }) => <BlobCanvas decorative color={agentColorHex(agent.color)} size={22} mini mood="idle" look={agentLook(agent)} label={agent.name} />

  return <div className="sheet-backdrop blob-dialog-backdrop is-entered">
    <section ref={ref as React.RefObject<HTMLElement>} className="blob-dialog import-conversations" role="dialog" aria-modal="true" aria-labelledby="import-conversations-title">
      <h2 id="import-conversations-title"><Download size={17} aria-hidden="true" />Import conversations</h2>
      <ol className="import-steps" aria-label="Steps">{(['Where from', 'Which ones', 'Who gets them', 'Import'] as const).map((label, index) => {
        const at = step === 'sources' ? 0 : step === 'rows' ? 1 : step === 'who' ? 2 : 3
        return <li key={label} data-state={index < at ? 'done' : index === at ? 'current' : 'next'}>{index < at ? <Check size={11} aria-hidden="true" /> : <span>{index + 1}</span>}{label}</li>
      })}</ol>

      {error && <p className="import-error" role="alert">{error}</p>}

      {step === 'sources' && <div className="import-body">
        {!scan && !error && <p className="import-note" role="status">Looking for conversations on this computer…</p>}
        {scan && scan.sources.length === 0 && <p className="import-note">No Claude Code, Codex or OpenCode conversations were found on this computer.</p>}
        {scan && scan.sources.length > 0 && <>
          <p className="import-note">Found on this computer. Only the last {scan.sinceDays} days are offered. Passwords, sign-ins and the agents' private reasoning are never copied.</p>
          <ul className="import-sources">{scan.sources.map((source) => <li key={source.source}>
            <label className={source.error ? 'disabled' : undefined}>
              <input type="checkbox" checked={sources.has(source.source)} disabled={!!source.error || source.recent === 0} onChange={() => setSources((current) => { const next = new Set(current); if (next.has(source.source)) next.delete(source.source); else next.add(source.source); return next })} />
              <ProviderLogo provider={source.source} size={18} />
              <span className="import-source-copy"><strong>{source.label}</strong><small>{source.error ?? `${source.recent} in the last ${scan.sinceDays} days · ${source.conversations} in all · newest ${day(source.newest)}`}</small></span>
            </label>
          </li>)}</ul>
        </>}
        <div className="blob-dialog-actions"><button type="button" className="secondary-button" onClick={close}>Cancel</button><button type="button" className="primary-button" disabled={!sources.size || busy} onClick={() => void loadRows()}>{busy ? 'Listing…' : 'Next'}</button></div>
      </div>}

      {step === 'rows' && <div className="import-body">
        <p className="import-note">Imported conversations are read-only, unless the agent that wrote them is still installed here and still has the file; then the right employee can carry on.</p>
        <div className="import-rows">{groups.map(([path, list]) => <section key={path} className="import-group">
          <header><FolderOpen size={13} aria-hidden="true" /><strong title={path}>{folderName(path) || 'No folder'}</strong><button type="button" className="queue-link" onClick={() => toggleGroup(list)}>{list.every((row) => row.alreadyImported || row.error || picked.has(key(row))) ? 'None' : 'All'}</button></header>
          <ul>{list.map((row) => <li key={key(row)}>
            <label className={row.alreadyImported || row.error ? 'disabled' : undefined}>
              <input type="checkbox" checked={picked.has(key(row))} disabled={row.alreadyImported || !!row.error} onChange={() => toggle(key(row))} />
              <ProviderLogo provider={row.agent} size={14} />
              <span className="import-row-copy"><strong>{row.title || row.firstUser || 'Untitled'}</strong><small>{day(row.updatedAt)} · {row.messageCount} messages · {size(row.bytes)}</small></span>
              <span className={`import-badge${row.canResumeWith ? ' resume' : ''}`}>{row.alreadyImported ? 'Imported' : row.error ? 'Unreadable' : row.canResumeWith ? `Can continue in ${row.agentLabel}` : 'Read only'}</span>
            </label>
          </li>)}</ul>
        </section>)}</div>
        <div className="blob-dialog-actions"><button type="button" className="secondary-button" onClick={() => setStep('sources')}>Back</button><button type="button" className="primary-button" disabled={!pickedCount} onClick={() => setStep('who')}>Next · {pickedCount} chosen</button></div>
      </div>}

      {step === 'who' && <div className="import-body">
        <div className="import-who" role="radiogroup" aria-label="Who gets them">
          <button type="button" role="radio" aria-checked={who === 'distribute'} onClick={() => setWho('distribute')}><Sparkles size={16} aria-hidden="true" /><strong>Share them out</strong><small>Retinue suggests an employee for each one, by project folder, coding agent and role, and you can change any of them.</small></button>
          <button type="button" role="radio" aria-checked={who === 'one'} onClick={() => setWho('one')}><User size={16} aria-hidden="true" /><strong>One employee</strong><small>All {pickedCount} go to the employee you pick.</small></button>
          <button type="button" role="radio" aria-checked={who === 'ceo'} onClick={() => setWho('ceo')}><Users size={16} aria-hidden="true" /><strong>{ceo?.name ?? 'The CEO'}</strong><small>All of them go to the CEO, who knows every project.</small></button>
        </div>
        {who === 'one' && <label className="blob-field">Employee<select value={agentId} onChange={(event) => setAgentId(event.target.value)}>{bots.map((agent) => <option key={agent.id} value={agent.id}>{agent.name}{agent.role ? ` · ${agent.role}` : ''}</option>)}</select></label>}
        <div className="blob-dialog-actions"><button type="button" className="secondary-button" onClick={() => setStep('rows')}>Back</button><button type="button" className="primary-button" disabled={who === 'one' && !agentId} onClick={() => void start()}>Import {pickedCount}</button></div>
      </div>}

      {step === 'importing' && <div className="import-body" role="status">
        <div className="import-progress"><span style={{ width: `${progress.total ? Math.round((progress.done / progress.total) * 100) : 0}%` }} /></div>
        <p className="import-note">{progress.done} of {progress.total} imported{progress.last ? ` · ${progress.last}` : ''}</p>
      </div>}

      {step === 'review' && <div className="import-body">
        <p className="import-note">Here's who each conversation would go to, and why. Change any of them, then confirm.</p>
        <ul className="import-review">{suggestions.map((item) => {
          const chosen = agents.find((agent) => agent.id === choices[item.sessionId])
          return <li key={item.sessionId}>
            <span className="import-row-copy"><strong>{item.title}</strong><small>{item.detail}</small></span>
            {chosen && <Avatar agent={chosen} />}
            <select aria-label={`Who gets ${item.title}`} value={choices[item.sessionId] ?? ''} onChange={(event) => setChoices((current) => ({ ...current, [item.sessionId]: event.target.value }))}>
              <option value="">{ceo?.name ?? 'The CEO'}</option>
              {bots.map((agent) => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
            </select>
          </li>
        })}</ul>
        <div className="blob-dialog-actions"><button type="button" className="primary-button" disabled={busy} onClick={() => void confirmDistribution()}>{busy ? 'Giving them out…' : 'Confirm'}</button></div>
      </div>}

      {step === 'done' && <div className="import-body">
        <p className="import-done"><Check size={16} aria-hidden="true" />Imported {result?.imported ?? 0} conversation{result?.imported === 1 ? '' : 's'}{result?.skipped ? `, ${result.skipped} skipped` : ''}{result?.failed ? `, ${result.failed} failed` : ''}.</p>
        {who !== 'distribute' && <p className="import-note">They are with {who === 'ceo' ? nameOf(ceo?.id) : nameOf(agentId)}.</p>}
        <div className="blob-dialog-actions">{result?.sessions[0] && onOpen && <button type="button" className="secondary-button" onClick={() => { onOpen(result.sessions[0]!); close() }}>Open the first one</button>}<button type="button" className="primary-button" onClick={close}>Done</button></div>
      </div>}
    </section>
  </div>
}
