// Installing and updating a coding agent from Settings › Providers (22 November
// 2026; design docs/research/cli-install-and-update.md, section 8.8). The daemon
// does the work, windowless (`runtime.install.*`); this card shows what will
// happen before it does, and how far it has got. Consent is bound to what the card
// showed: the version and the first 12 characters of the publisher's digest.
import { useCallback, useEffect, useState } from 'react'
import { Check, Download, ExternalLink, RefreshCw, TriangleAlert } from 'lucide-react'
import { listenForDaemonEvents, rpc } from '../tauri'
import { useDialogAccessibility, useDialogEntered } from './dialogFocus'
import { openSafeLink } from './openSafeLink'
import { ProviderLogo } from './providerBrand'
import './agentInstall.css'

/** Every coding agent the daemon's install table knows; the rest have no button. */
export const INSTALL_PROVIDERS = ['claude', 'codex', 'opencode', 'pi', 'cursor', 'grok', 'antigravity', 'qwen'] as const

export interface InstallPlan {
  provider: string
  displayName: string
  mode: 'adopt' | 'manage' | 'link'
  version: string | null
  sourceHost: string | null
  bytes: number | null
  digestPrefix: string | null
  digestAlgorithm: string | null
  target: string
  freeBytes: number | null
  runsTurn: boolean
  supported: boolean
  reason: string | null
  page: string
  owner: string | null
  installed: { version: string | null; digestPrefix: string | null; damaged: boolean }
  previousVersion: string | null
  autoUpdate: boolean
}

export interface InstallJob {
  jobId: string
  provider: string
  mode: string
  phase: 'downloading' | 'checking' | 'installing' | 'done' | 'failed' | 'cancelled'
  version: string | null
  bytesDone: number
  bytesTotal: number
  line: string | null
  error: string | null
}

export const finished = (job: Pick<InstallJob, 'phase'> | null) => !!job && (job.phase === 'done' || job.phase === 'failed' || job.phase === 'cancelled')

export function megabytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`
  return `${Math.max(bytes > 0 ? 1 : 0, Math.round(bytes / 1_000_000))} MB`
}

/** The one line a job shows, in the phases the daemon reports. */
export function jobLine(job: InstallJob): string {
  if (job.phase === 'downloading') {
    if (job.bytesTotal > 0) return `Downloading ${Math.min(100, Math.floor((job.bytesDone / job.bytesTotal) * 100))}% · ${megabytes(job.bytesDone)} of ${megabytes(job.bytesTotal)}`
    return job.bytesDone > 0 ? `Downloading · ${megabytes(job.bytesDone)}` : 'Downloading'
  }
  if (job.phase === 'checking') return 'Checking the download'
  if (job.phase === 'installing') return 'Installing'
  if (job.phase === 'done') return job.version ? `Done · ${job.version}` : 'Done'
  return job.error ?? (job.phase === 'cancelled' ? 'The install was cancelled. Nothing was changed.' : 'The install failed.')
}

/** Who owns the owner's own copy, in words. */
export function ownerWords(owner: string | null): string {
  if (owner === 'npm') return 'npm'
  if (owner === 'winget') return 'WinGet'
  if (owner === 'own') return 'its own updater'
  if (owner === 'system') return 'a system-wide package manager'
  return 'its installer'
}

/** What the card says will happen. Never a guess: an unknown size or digest is left out, not shown as zero. */
export function planSentences(plan: InstallPlan): string[] {
  const lines: string[] = []
  if (plan.mode === 'adopt') {
    lines.push(`Retinue updates your own copy${plan.version ? ` to ${plan.version}` : ''} with ${ownerWords(plan.owner)}${plan.sourceHost ? `, from ${plan.sourceHost}` : ''}. No window opens.`)
    if (plan.runsTurn) lines.push('A bot is using it now. Try again when its turn has finished.')
    return lines
  }
  if (plan.mode === 'manage') {
    lines.push(`Retinue downloads ${plan.displayName}${plan.version ? ` ${plan.version}` : ''}${plan.sourceHost ? ` from ${plan.sourceHost}` : ''}${plan.bytes ? ` (${megabytes(plan.bytes)})` : ''} and checks it against the publisher's checksum before anything runs.`)
    lines.push('It goes in Retinue\'s own folder. Your PATH and any copy you installed yourself are not changed.')
    if (plan.installed.damaged) lines.push('Retinue\'s copy is damaged. Installing replaces it.')
    else if (plan.installed.version) lines.push(`Retinue's copy is ${plan.installed.version} now. It is kept for one "go back" until the next install.`)
    if (plan.runsTurn) lines.push('A bot is using it now. The new version takes effect after this turn.')
  }
  return lines
}

/** What the keep-up-to-date switch does, in one line. */
export function autoUpdateLine(plan: Pick<InstallPlan, 'autoUpdate' | 'sourceHost' | 'mode'>): string {
  const from = plan.sourceHost ? ` from ${plan.sourceHost}` : ''
  const turn = plan.mode === 'adopt' ? 'never while a bot is using it' : 'and a bot that is working keeps its version until its turn ends'
  const what = plan.mode === 'adopt' ? 'your own copy' : "Retinue's copy"
  return `After your first install or update here, Retinue checks every six hours and updates ${what}${from} on its own, ${turn}. A different source still asks you first.`
}

const message = (error: unknown, fallback: string) => error instanceof Error && error.message ? error.message : fallback

export function AgentInstallDialog({ provider, found, onClose, onChanged }: { provider: string; found: boolean; onClose: () => void; onChanged: () => void }) {
  const [plan, setPlan] = useState<InstallPlan | null>(null)
  const [job, setJob] = useState<InstallJob | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [removing, setRemoving] = useState(false)
  const running = !!job && !finished(job)
  const entered = useDialogEntered()
  const { ref, close } = useDialogAccessibility(onClose, () => !busy)

  const loadPlan = useCallback(async () => {
    setError(null)
    try { setPlan((await rpc<{ plan: InstallPlan }>('runtime.install.plan', { provider })).plan) } catch (reason) { setError(message(reason, 'The publisher\'s release could not be read. Check the connection and try again.')) }
  }, [provider])
  useEffect(() => { void loadPlan() }, [loadPlan])

  const readJob = useCallback(async (jobId: string) => {
    try {
      const next = (await rpc<{ job: InstallJob }>('runtime.install.status', { jobId })).job
      setJob(next)
      if (next.phase === 'done') onChanged()
    } catch { /* the next event or tick tries again */ }
  }, [onChanged])

  useEffect(() => {
    if (!job || finished(job)) return
    const jobId = job.jobId
    let stop: (() => void) | null = null
    let live = true
    void listenForDaemonEvents((event) => { if (event.type === 'runtime.install.changed' && (event.payload as { provider?: string } | undefined)?.provider === provider) void readJob(jobId) }).then((unlisten) => { if (live) stop = unlisten; else unlisten() }).catch(() => undefined)
    const tick = window.setInterval(() => void readJob(jobId), 1500)
    return () => { live = false; stop?.(); window.clearInterval(tick) }
  }, [job?.jobId, job && finished(job), provider, readJob]) // eslint-disable-line react-hooks/exhaustive-deps

  const install = async () => {
    if (!plan) return
    setBusy(true); setError(null)
    try {
      const reply = await rpc<{ jobId: string; version?: string }>('runtime.install', { provider, confirm: true, version: plan.version, digestPrefix: plan.digestPrefix })
      setJob({ jobId: reply.jobId, provider, mode: plan.mode, phase: 'downloading', version: reply.version ?? plan.version, bytesDone: 0, bytesTotal: plan.bytes ?? 0, line: null, error: null })
      void readJob(reply.jobId)
    } catch (reason) {
      const text = message(reason, 'The install could not start.')
      setError(text)
      if (/plan changed/i.test(text)) void loadPlan()
    } finally { setBusy(false) }
  }

  const cancel = async () => { if (job) await rpc('runtime.install.cancel', { jobId: job.jobId }).catch(() => undefined) }

  const switchMode = async (mode: 'adopt' | 'manage') => {
    setBusy(true); setError(null); setPlan(null)
    try { await rpc('settings.set', { key: `agents.${provider}.mode`, value: mode }) } catch (reason) { setError(message(reason, 'That choice could not be saved.')) }
    await loadPlan()
    setBusy(false)
  }

  const keepUpToDate = async (on: boolean) => {
    if (!plan) return
    setBusy(true); setError(null)
    try {
      await rpc('settings.set', { key: `agents.${provider}.autoUpdate`, value: on })
      setPlan({ ...plan, autoUpdate: on })
    } catch (reason) { setError(message(reason, 'That choice could not be saved.')) } finally { setBusy(false) }
  }

  const remove = async () => {
    setBusy(true); setError(null)
    try {
      await rpc('runtime.install.remove', { provider, confirm: true })
      setRemoving(false)
      onChanged()
      await loadPlan()
    } catch (reason) { setError(message(reason, 'Retinue\'s copy could not be removed.')) } finally { setBusy(false) }
  }

  const name = plan?.displayName ?? provider
  const verb = plan?.mode === 'adopt' || found ? 'Update' : 'Install'
  const blocked = !plan || !plan.supported || !plan.version || (plan.mode === 'adopt' && plan.runsTurn)
  return <div className={`sheet-backdrop blob-dialog-backdrop ${entered ? 'is-entered' : ''}`}>
    <section ref={ref as React.RefObject<HTMLElement>} className="blob-dialog agent-install" role="dialog" aria-modal="true" aria-labelledby="agent-install-title" tabIndex={-1}>
      <h2 id="agent-install-title"><ProviderLogo provider={provider} size={18} />{verb} {name}</h2>
      {!plan && !error && <p className="agent-install-note" role="status">Asking the publisher for the latest release…</p>}
      {plan && !job && <>
        {!plan.supported && <p className="agent-install-note">{plan.reason ?? 'Retinue cannot install this coding agent here.'}</p>}
        {plan.supported && planSentences(plan).map((line) => <p key={line} className="agent-install-note">{line}</p>)}
        {plan.supported && plan.mode === 'manage' && <dl className="agent-install-facts">
          {plan.version && <><dt>Version</dt><dd>{plan.version}</dd></>}
          {plan.sourceHost && <><dt>From</dt><dd>{plan.sourceHost}</dd></>}
          {plan.bytes ? <><dt>Size</dt><dd>{megabytes(plan.bytes)}{plan.freeBytes ? ` · ${megabytes(plan.freeBytes)} free` : ''}</dd></> : null}
          {plan.digestPrefix && <><dt>Checksum</dt><dd><code>{plan.digestAlgorithm ? `${plan.digestAlgorithm} ` : ''}{plan.digestPrefix}…</code></dd></>}
          <dt>Folder</dt><dd className="agent-install-path" title={plan.target}>{plan.target}</dd>
        </dl>}
        {plan.supported && plan.mode !== 'link' && <div className="agent-install-auto">
          <span><strong>Keep {name} up to date</strong><small>{autoUpdateLine(plan)}</small></span>
          <button type="button" className={`toggle ${plan.autoUpdate ? 'on' : ''}`} role="switch" aria-label={`Keep ${name} up to date`} aria-checked={plan.autoUpdate} disabled={busy} onClick={() => void keepUpToDate(!plan.autoUpdate)}><i /></button>
        </div>}
        <div className="agent-install-switch">
          {plan.mode === 'adopt' && <button type="button" className="queue-link" disabled={busy} onClick={() => void switchMode('manage')}>Use a copy Retinue keeps instead</button>}
          {plan.mode === 'manage' && found && <button type="button" className="queue-link" disabled={busy} onClick={() => void switchMode('adopt')}>Update my own copy instead</button>}
          {plan.installed.version && !removing && <button type="button" className="queue-link" disabled={busy} onClick={() => setRemoving(true)}>Remove Retinue's copy</button>}
        </div>
        {removing && <div className="agent-install-confirm" role="group" aria-label="Remove Retinue's copy">
          <span>Remove Retinue's copy of {name}? Bots on it stop working until it is installed again. A copy you installed yourself is not touched.</span>
          <button type="button" className="secondary-button small" disabled={busy} onClick={() => setRemoving(false)}>Keep it</button>
          <button type="button" className="secondary-button danger-button small" disabled={busy} onClick={() => void remove()}>Remove</button>
        </div>}
      </>}
      {job && <div className={`agent-install-job${job.phase === 'failed' ? ' failed' : ''}`} role="status">
        {job.phase === 'done' ? <Check size={15} aria-hidden="true" /> : job.phase === 'failed' || job.phase === 'cancelled' ? <TriangleAlert size={15} aria-hidden="true" /> : <RefreshCw size={15} className="spinning" aria-hidden="true" />}
        <span>{jobLine(job)}{job.phase === 'failed' && job.line ? <small>{job.line}</small> : null}</span>
        {job.phase === 'downloading' && job.bytesTotal > 0 && <div className="agent-install-bar" aria-hidden="true"><i style={{ width: `${Math.min(100, (job.bytesDone / job.bytesTotal) * 100)}%` }} /></div>}
      </div>}
      {error && <p className="agent-install-error" role="alert">{error}</p>}
      <div className="blob-dialog-actions">
        {plan && !plan.supported && !job && <button type="button" className="secondary-button" onClick={() => void openSafeLink(plan.page)}><ExternalLink size={14} aria-hidden="true" />Open the install page</button>}
        {running ? <button type="button" className="secondary-button" onClick={() => void cancel()}>Stop</button> : <button type="button" className="secondary-button" disabled={busy} onClick={close}>{job && finished(job) ? 'Close' : 'Cancel'}</button>}
        {!job && plan?.supported && <button type="button" className="primary-button" disabled={busy || blocked} onClick={() => void install()}><Download size={14} aria-hidden="true" />{plan.version ? verb === 'Update' ? `Update to ${plan.version}` : `Install ${plan.version}` : verb}</button>}
        {job && (job.phase === 'failed' || job.phase === 'cancelled') && <button type="button" className="primary-button" onClick={() => { setJob(null); void loadPlan() }}>Try again</button>}
      </div>
    </section>
  </div>
}
