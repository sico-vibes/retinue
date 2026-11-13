// Settings › Integrations › Computer. Links a computer on the owner's own server (Harbor), shows
// what it is doing, wakes and sleeps it, and decides whether employees get its tools. The daemon
// keeps the token; this page sends it once when linking and never sees it again.
import { useEffect, useState } from 'react'
import { Check, Copy, Monitor, Moon, Play, RotateCw, ShieldAlert, Sun } from 'lucide-react'
import { rpc } from '../tauri'
import { useDialogAccessibility, useDialogEntered } from './dialogFocus'
import { ComputerView } from './ComputerView'
import { COMPUTER_SETUP_PROMPT } from './computerSetup'
import { SettingsGroup, SettingsRow } from './SettingsParts'
import { addressProblem, parseRoutines, scheduleText, sizeText, spanSince, stateWord, tidyAddress, useComputer, type ComputerRoutine, type ComputerStatus } from './computer'
import './blobPage.css'
import './computer.css'

const reason = (error: unknown, fallback: string) => error instanceof Error && error.message ? error.message : fallback

function Meter({ label, value, percent }: { label: string; value: string; percent: number | null }) {
  const level = percent === null ? '' : percent >= 90 ? ' critical' : percent >= 75 ? ' warning' : ''
  return <div className="computer-meter">
    <span className="computer-meter-line"><span>{label}</span><b>{value}</b></span>
    <span className={`rail-meter${level}`} role="img" aria-label={percent === null ? `${label}: ${value}` : `${label}: ${Math.round(percent)} percent`}><b style={{ width: `${percent ?? 0}%` }} /></span>
  </div>
}

/** The computer itself: its name, one coloured word for its state, and what it can be told to do. */
function ComputerCard({ status, busy, onAction, onWatch }: { status: ComputerStatus; busy: string | null; onAction: (action: 'wake' | 'sleep' | 'restart') => void; onWatch: () => void }) {
  const awake = status.reachable && status.state === 'awake'
  const asleep = status.reachable && status.state === 'asleep'
  const span = awake ? spanSince(status.since) : null
  const host = (() => { try { return status.address ? new URL(status.address).host : null } catch { return null } })()
  return <div className="computer-card" data-state={status.reachable ? status.state : 'unreachable'}>
    <div className="computer-head">
      <span className="computer-icon" aria-hidden="true"><Monitor size={18} /></span>
      <span className="computer-copy">
        <strong>{status.name ?? 'Computer'}</strong>
        <small><span className="computer-state">{stateWord(status)}</span>{span ? ` for ${span}` : ''}{host ? ` · ${host}` : ''}</small>
      </span>
      <span className="computer-actions">
        {asleep && <button type="button" className="primary-button small" disabled={!!busy} onClick={() => onAction('wake')}><Sun size={13} aria-hidden="true" />{busy === 'wake' ? 'Waking…' : 'Wake'}</button>}
        {awake && <button type="button" className="secondary-button small" disabled={!!busy} onClick={() => onAction('sleep')}><Moon size={13} aria-hidden="true" />{busy === 'sleep' ? 'Putting to sleep…' : 'Sleep'}</button>}
        {awake && <button type="button" className="secondary-button small" disabled={!!busy} onClick={() => onAction('restart')}><RotateCw size={13} aria-hidden="true" />{busy === 'restart' ? 'Restarting…' : 'Restart'}</button>}
        {awake && <button type="button" className="primary-button small" onClick={onWatch}><Monitor size={13} aria-hidden="true" />Open the screen</button>}
      </span>
    </div>
    {awake && <div className="computer-meters">
      <Meter label="Processor" value={status.cpuPercent === null ? 'Not reported' : `${Math.round(status.cpuPercent)}%${status.limits?.cpu ? ` of ${status.limits.cpu} cores` : ''}`} percent={status.cpuPercent} />
      <Meter label="Memory" value={status.memoryText ?? (status.memoryPercent === null ? 'Not reported' : `${Math.round(status.memoryPercent)}%`)} percent={status.memoryPercent} />
      <div className="computer-meter"><span className="computer-meter-line"><span>Kept files</span><b>{sizeText(status.diskBytes) ?? 'Not reported'}</b></span><small>Stay when it sleeps or restarts.</small></div>
    </div>}
    {asleep && <p className="computer-note">Its files are kept and its routines wait. An employee can wake it when a job needs it.</p>}
    {status.reachable && status.state === 'missing' && <p className="computer-note">Harbor has no computer set up yet. Open Harbor on your server and choose Provision, then check again.</p>}
    {!status.reachable && <p className="computer-note" role="status">{status.problem ?? 'Your server did not answer.'} Nothing was changed.</p>}
  </div>
}

function RoutineRow({ routine, awake, running, result, onRun }: { routine: ComputerRoutine; awake: boolean; running: boolean; result: { ok: boolean; output: string | null } | null; onRun: () => void }) {
  const [open, setOpen] = useState(false)
  const last = spanSince(routine.lastRunAt)
  const outcome = result ?? (routine.lastOk === null ? null : { ok: routine.lastOk, output: routine.lastOutput })
  const output = outcome?.output ?? null
  return <li className="computer-routine">
    <div className="computer-routine-line">
      <span className="computer-routine-copy">
        <strong>{routine.name}{!routine.enabled && <span className="rail-tag">Paused</span>}</strong>
        <small>{scheduleText(routine.cron)}{result ? result.ok ? ' · ran just now' : ' · ' : last ? ` · last ran ${last} ago` : ' · not run yet'}{outcome && !outcome.ok && <span className="computer-failed">{result ? 'failed just now' : ', and failed'}</span>}</small>
      </span>
      {output && <button type="button" className="queue-link" aria-expanded={open} onClick={() => setOpen((value) => !value)}>{open ? 'Hide output' : 'Output'}</button>}
      <button type="button" className="secondary-button small" disabled={!awake || running} title={awake ? undefined : 'Wake the computer first'} onClick={onRun}><Play size={12} aria-hidden="true" />{running ? 'Running…' : 'Run now'}</button>
    </div>
    <code className="computer-routine-command" title={routine.command}>{routine.command}</code>
    {open && output && <pre className="computer-output">{output}</pre>}
  </li>
}

/** No server yet: a prompt the owner gives to a coding agent on their server, which sets the computer up and hands back the two values the form asks for. */
function SetupPrompt() {
  const [copied, setCopied] = useState(false)
  const [failed, setFailed] = useState(false)
  const copy = async () => {
    try { await navigator.clipboard.writeText(COMPUTER_SETUP_PROMPT); setFailed(false); setCopied(true); window.setTimeout(() => setCopied(false), 1600) } catch { setFailed(true) }
  }
  return <div className="computer-setup">
    <ol className="computer-steps">
      <li><b>Copy this prompt.</b> It describes the computer and the small service Retinue talks to.</li>
      <li><b>Give it to a coding agent on your server.</b> Claude Code, Codex or OpenCode over SSH all work. It builds the computer and tells you an address and a token.</li>
      <li><b>Paste the address and the token above.</b> Retinue checks them with your server before keeping anything.</li>
    </ol>
    <div className="computer-setup-prompt">
      <pre tabIndex={0} aria-label="Setup prompt for an agent on your server">{COMPUTER_SETUP_PROMPT}</pre>
      <button type="button" className="icon-button tiny" onClick={() => void copy()} aria-label={copied ? 'Copied' : 'Copy the setup prompt'} title={copied ? 'Copied' : 'Copy'}>{copied ? <Check size={13} /> : <Copy size={13} />}</button>
    </div>
    <div className="computer-setup-actions">
      <button type="button" className="secondary-button small" onClick={() => void copy()}>{copied ? <Check size={13} aria-hidden="true" /> : <Copy size={13} aria-hidden="true" />}{copied ? 'Copied' : 'Copy the prompt'}</button>
      {failed && <span className="computer-field-problem" role="alert">It could not be copied. Select the text and copy it yourself.</span>}
    </div>
    <p>Needs a Linux server with Docker and about 4 GB of free memory. The computer runs in its own container, apart from everything else on the server.</p>
  </div>
}

function LinkForm({ busy, problem, onLink }: { busy: boolean; problem: string | null; onLink: (address: string, token: string) => void }) {
  const [address, setAddress] = useState('')
  const [token, setToken] = useState('')
  const invalid = addressProblem(address)
  const ready = !!address.trim() && !invalid && token.trim().length >= 8 && !busy
  return <form className="computer-link" onSubmit={(event) => { event.preventDefault(); if (ready) onLink(tidyAddress(address), token.trim()) }}>
    <label className="new-project-field"><span>Harbor address</span>
      <input className="text-input" value={address} spellCheck={false} autoComplete="off" placeholder="https://dashboard.example.com" aria-invalid={!!invalid} onChange={(event) => setAddress(event.target.value)} />
      {invalid && <small className="computer-field-problem">{invalid}</small>}
    </label>
    <label className="new-project-field"><span>Computer token</span>
      <input className="text-input" type="password" value={token} spellCheck={false} autoComplete="off" placeholder="hrb_…" onChange={(event) => setToken(event.target.value)} />
      <small className="project-settings-note">Kept on this PC only. It is never shown again and never goes into a backup.</small>
    </label>
    {problem && <p className="memory-problem" role="alert">{problem}</p>}
    <div className="computer-link-actions"><button type="submit" className="primary-button small" disabled={!ready}>{busy ? 'Checking…' : 'Link computer'}</button></div>
  </form>
}

/** Settings › Integrations › Computer. */
export function ComputerPage({ connected }: { connected: boolean }) {
  const { status, failed, refresh, apply } = useComputer(connected)
  const [busy, setBusy] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const [consent, setConsent] = useState(false)
  const [unlinking, setUnlinking] = useState(false)
  const [screen, setScreen] = useState(false)
  const [routines, setRoutines] = useState<ComputerRoutine[] | null>(null)
  const [running, setRunning] = useState<string | null>(null)
  const [results, setResults] = useState<Record<string, { ok: boolean; output: string | null }>>({})
  const linked = !!status?.linked
  const reachable = !!status?.reachable
  const awake = reachable && status?.state === 'awake'
  useEffect(() => {
    if (!connected || !linked || !reachable) { setRoutines(null); return }
    let live = true
    rpc<unknown>('computer.routines', {}).then((reply) => { if (live) setRoutines(parseRoutines(reply)) }).catch(() => { if (live) setRoutines([]) })
    return () => { live = false }
  }, [connected, linked, reachable, status?.state])

  const run = async (key: string, method: string, params: Record<string, unknown>, done?: string) => {
    setProblem(null); setNotice(null); setBusy(key)
    try {
      apply(await rpc<unknown>(method, params))
      if (done) setNotice(done)
      return true
    } catch (error) { setProblem(reason(error, 'That could not be done.')); return false } finally { setBusy(null) }
  }
  const act = (action: 'wake' | 'sleep' | 'restart') => void run(action, 'computer.action', { action }, action === 'wake' ? 'Awake.' : action === 'sleep' ? 'Asleep. Its files are kept.' : 'Restarted.')
  const runRoutine = async (routine: ComputerRoutine) => {
    setProblem(null); setRunning(routine.id)
    try {
      const reply = await rpc<{ ok?: unknown; output?: unknown }>('computer.routine.run', { id: routine.id })
      setResults((all) => ({ ...all, [routine.id]: { ok: reply?.ok === true, output: typeof reply?.output === 'string' && reply.output.trim() ? reply.output : null } }))
    } catch (error) { setProblem(reason(error, `${routine.name} could not be run.`)) } finally { setRunning(null) }
  }

  const intro = <p className="settings-intro">A computer on your own server that employees can use for browser work, long jobs and routines. It keeps running when this PC is off.</p>
  if (!connected) return <>{intro}<p className="settings-row-note computer-plain">Available in the desktop app.</p></>
  if (!status) return <>{intro}<p className="settings-row-note computer-plain" role="status">{failed ? 'The computer settings could not be read.' : 'Reading…'}</p></>

  if (!linked) return <>
    {intro}
    <SettingsGroup title="Link a computer">
      <LinkForm busy={busy === 'link'} problem={problem} onLink={(address, token) => void run('link', 'computer.link', { address, token, confirm: true }, 'Linked. Employees cannot use it until you turn that on below.')} />
    </SettingsGroup>
    <SettingsGroup title="Set one up">
      <SetupPrompt />
    </SettingsGroup>
  </>

  return <>
    {intro}
    <SettingsGroup title="Computer" action={<button type="button" className="queue-link" disabled={!!busy} onClick={refresh}>Check now</button>}>
      <ComputerCard status={status} busy={busy} onAction={act} onWatch={() => setScreen(true)} />
    </SettingsGroup>
    {notice && <p className="backup-done settings-row-note" role="status"><Check size={13} aria-hidden="true" />{notice}</p>}
    {problem && <p className="memory-problem settings-row-note" role="alert">{problem}</p>}
    <SettingsGroup title="Employees">
      <SettingsRow label="Let employees use this computer" hint="New conversations get tools to wake it, run commands on it, read its log and manage its routines. A command still waits for your approval when the employee is set to ask.">
        <button type="button" className={`toggle ${status.enabled ? 'on' : ''}`} role="switch" aria-label="Let employees use this computer" aria-checked={status.enabled} disabled={!!busy} onClick={() => status.enabled ? void run('enable', 'computer.setEnabled', { enabled: false }, 'Off. Employees can no longer use the computer.') : setConsent(true)}><i /></button>
      </SettingsRow>
      <SettingsRow label="From your phone" detail="Never. A paired device cannot see or control the computer." />
    </SettingsGroup>
    {reachable && status.state !== 'missing' && <SettingsGroup title="Routines">
      {routines === null && <p className="settings-row-note computer-plain" role="status">Reading…</p>}
      {routines?.length === 0 && <p className="settings-row-note computer-plain">No routines yet. Ask an employee to set one up, or add one in Harbor. They run only while the computer is awake.</p>}
      {!!routines?.length && <ul className="computer-routines">
        {routines.map((routine) => <RoutineRow key={routine.id} routine={routine} awake={awake} running={running === routine.id} result={results[routine.id] ?? null} onRun={() => void runRoutine(routine)} />)}
      </ul>}
    </SettingsGroup>}
    <SettingsGroup title="Link">
      <SettingsRow label="Linked to" detail={status.address ?? 'Your server'}>
        <button type="button" className="secondary-button small" disabled={!!busy} onClick={() => setUnlinking(true)}>Unlink</button>
      </SettingsRow>
    </SettingsGroup>
    {screen && <ComputerView status={status} waking={busy === 'wake'} onClose={() => setScreen(false)} onWake={() => act('wake')} />}
    {consent && <ConsentDialog title="Let employees use this computer?" confirm="Turn on" busyWord="Turning on…" busy={busy === 'enable'} onCancel={() => setConsent(false)} onConfirm={async () => { if (await run('enable', 'computer.setEnabled', { enabled: true, confirm: true }, 'On. New conversations can use the computer.')) setConsent(false) }}>
      <p>Employees in new conversations can run commands on {status.name ?? 'the computer'}, use its browser and set up routines there. Commands run inside the computer, never on the server itself or on this PC.</p>
      <p>All employees share the one computer, so two using its screen at once will get in each other’s way.</p>
    </ConsentDialog>}
    {unlinking && <ConsentDialog title="Unlink this computer?" confirm="Unlink" busyWord="Unlinking…" danger busy={busy === 'unlink'} onCancel={() => setUnlinking(false)} onConfirm={async () => { if (await run('unlink', 'computer.unlink', {}, undefined)) setUnlinking(false) }}>
      <p>Retinue forgets the address and the token, and employees lose the tools. The computer keeps running on your server. To cut the token off for good, revoke it in Harbor.</p>
    </ConsentDialog>}
  </>
}

function ConsentDialog({ title, confirm, busyWord, busy, danger = false, onCancel, onConfirm, children }: { title: string; confirm: string; busyWord: string; busy: boolean; danger?: boolean; onCancel: () => void; onConfirm: () => void; children: React.ReactNode }) {
  const entered = useDialogEntered()
  const { ref, close } = useDialogAccessibility(onCancel, () => !busy)
  return <div className={`sheet-backdrop blob-dialog-backdrop ${entered ? 'is-entered' : ''}`}>
    <section ref={ref} className="blob-dialog connect-consent full-access-consent" role="dialog" aria-modal="true" aria-labelledby="computer-consent-title" tabIndex={-1}>
      <h2 id="computer-consent-title">{!danger && <ShieldAlert size={18} aria-hidden="true" />}{title}</h2>
      {children}
      <div className="blob-dialog-actions">
        <button type="button" className="secondary-button" data-dialog-initial-focus="" disabled={busy} onClick={close}>Cancel</button>
        <button type="button" className={danger ? 'secondary-button danger-button' : 'primary-button'} disabled={busy} onClick={onConfirm}>{busy ? busyWord : confirm}</button>
      </div>
    </section>
  </div>
}
