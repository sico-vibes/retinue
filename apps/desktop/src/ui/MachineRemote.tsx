// Working on another machine from here (17 November 2026, owner amendment "Retinue on many machines"):
// start a project there (a folder under the base folder that machine shares, or a clone), and manage that
// machine's paired devices. The window never talks to the other machine: every call goes to this machine's
// daemon (`machine.*`), which uses its own link, and the other machine's gateway decides.
import { useCallback, useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { ChevronLeft, CircleAlert, Folder, FolderGit2, FolderOpen, LoaderCircle, Smartphone, Trash2, X } from 'lucide-react'
import { rpc } from '../tauri'
import { useDialogAccessibility, useDialogEntered } from './dialogFocus'
import { MachineKindIcon } from './MachineBots'
import { cloneUrlProblem } from './ProjectSources'
import { Select } from './Select'
import { shortTime } from './rosterSelectors'
import type { Machine, MachineBot, MachineTier } from './machines'
import './machines.css'

const messageOf = (reason: unknown, fallback: string) => reason instanceof Error && reason.message ? reason.message : typeof reason === 'string' && reason ? reason : fallback

type BrowseEntry = { name: string; kind: 'folder' | 'file' }
type Browse = { base: string; path: string; parent: string | null; entries: BrowseEntry[]; truncated: boolean }
type CloneJob = { id: string; url: string; target: string | null; state: 'running' | 'done' | 'failed' | 'cancelled'; detail: string | null; project: { id: string; name: string } | null }
type MadeProject = { id: string; name: string }

/** The last part of a path, whichever separator the other machine uses. */
export const leafOf = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path

/** A folder name for a clone, from its URL ("https://github.com/team/repo.git" → "repo"). */
export function repoNameOf(url: string): string {
  const leaf = url.trim().replace(/[?#].*$/, '').replace(/\/+$/, '').split(/[/:]/).pop() ?? ''
  return leaf.replace(/\.git$/i, '')
}

function Shell({ title, onClose, busy, children, className = '' }: { title: string; onClose: () => void; busy: boolean; children: React.ReactNode; className?: string }) {
  const entered = useDialogEntered()
  const { ref, close } = useDialogAccessibility(onClose, () => !busy)
  return createPortal(<div className={`sheet-backdrop blob-dialog-backdrop ${entered ? 'is-entered' : ''}`}>
    <section ref={ref} className={`blob-dialog machine-remote ${className}`} role="dialog" aria-modal="true" aria-label={title} tabIndex={-1}>
      <header className="machine-view-head">
        <div className="machine-view-title"><h2>{title}</h2></div>
        <button type="button" className="icon-button" aria-label="Close" disabled={busy} onClick={close}><X size={16} aria-hidden="true" /></button>
      </header>
      {children}
    </section>
  </div>, document.body)
}

/** Choose a folder on the other machine, or clone a repository there, then start a conversation in it. */
export function NewProjectOnMachine({ machine, onClose, onStarted, initialMode = 'folder' }: { machine: Machine; onClose: () => void; onStarted: (bot: MachineBot, sessionId: string) => void; initialMode?: 'folder' | 'clone' }) {
  const [mode, setMode] = useState<'folder' | 'clone'>(initialMode)
  const [browse, setBrowse] = useState<Browse | null>(null)
  const [browseProblem, setBrowseProblem] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [name, setName] = useState('')
  const [url, setUrl] = useState('')
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const [job, setJob] = useState<CloneJob | null>(null)
  const [made, setMade] = useState<MadeProject | null>(null)
  const [starting, setStarting] = useState<string | null>(null)

  const open = useCallback(async (path?: string) => {
    setLoading(true); setBrowseProblem(null)
    try { setBrowse(await rpc<Browse>('machine.project.browse', { machineId: machine.id, ...(path ? { path } : {}) })) }
    catch (reason) { setBrowseProblem(messageOf(reason, `${machine.name} didn't list its folders.`)) }
    finally { setLoading(false) }
  }, [machine.id, machine.name])
  useEffect(() => { void open() }, [open])

  // A clone runs on the other machine; its progress is one in-memory job there, read every 2 seconds.
  const jobId = job?.state === 'running' ? job.id : null
  useEffect(() => {
    if (!jobId) return
    const timer = window.setInterval(() => {
      void rpc<{ job?: CloneJob }>('machine.clone.status', { machineId: machine.id, jobId }).then((reply) => {
        const next = reply?.job
        if (!next) return
        setJob(next)
        if (next.state === 'done' && next.project) setMade(next.project)
      }).catch((reason) => setProblem(messageOf(reason, `${machine.name} stopped answering about the clone.`)))
    }, 2000)
    return () => window.clearInterval(timer)
  }, [jobId, machine.id, machine.name])

  const here = browse?.path ?? null
  const addFolder = async () => {
    if (!here) return
    setBusy(true); setProblem(null)
    try {
      const reply = await rpc<{ project?: MadeProject }>('machine.project.create', { machineId: machine.id, path: here, ...(name.trim() ? { name: name.trim() } : {}) })
      if (reply?.project) setMade(reply.project)
    } catch (reason) { setProblem(messageOf(reason, `${machine.name} didn't add that folder.`)) } finally { setBusy(false) }
  }
  const clone = async () => {
    if (!here || !url.trim()) return
    setBusy(true); setProblem(null)
    try {
      const reply = await rpc<{ job?: CloneJob }>('machine.project.clone', { machineId: machine.id, url: url.trim(), parent: here, ...(name.trim() ? { name: name.trim() } : {}), confirm: true })
      if (reply?.job) { setJob(reply.job); if (reply.job.state === 'done' && reply.job.project) setMade(reply.job.project) }
    } catch (reason) { setProblem(messageOf(reason, `${machine.name} didn't start the clone.`)) } finally { setBusy(false) }
  }
  const start = async (bot: MachineBot) => {
    if (!made) return
    setStarting(bot.id); setProblem(null)
    try {
      const reply = await rpc<{ sessionId?: string }>('machine.start', { machineId: machine.id, agentId: bot.id, projectId: made.id })
      if (reply?.sessionId) onStarted(bot, reply.sessionId)
    } catch (reason) { setProblem(messageOf(reason, `${machine.name} didn't start a conversation.`)) } finally { setStarting(null) }
  }
  const cloning = job?.state === 'running'
  const urlProblem = url.trim() ? cloneUrlProblem(url) : null
  const folders = (browse?.entries ?? []).filter((entry) => entry.kind === 'folder')

  return <Shell title={`New project on ${machine.name}`} onClose={onClose} busy={busy || cloning} className="machine-new-project">
    {made ? <div className="machine-remote-body">
      <p className="machine-remote-done"><FolderGit2 size={15} aria-hidden="true" /><span><strong>{made.name}</strong> is a project on {machine.name} now.</span></p>
      {machine.bots.length > 0
        ? <><p>Start a conversation there with:</p>
          <ul className="machine-remote-bots">{machine.bots.map((bot) => <li key={bot.id}><button type="button" className="secondary-button small" disabled={!!starting} onClick={() => void start(bot)}>{starting === bot.id ? 'Starting…' : bot.name}</button></li>)}</ul></>
        : <p className="blob-muted">{machine.name} has no employees yet. Hire one there to work in this project.</p>}
      {problem && <p className="new-project-error" role="alert">{problem}</p>}
      <div className="blob-dialog-actions"><button type="button" className="primary-button" onClick={onClose}>Done</button></div>
    </div> : <div className="machine-remote-body">
      <div className="machine-remote-tabs" role="tablist" aria-label="How to start">
        <button type="button" role="tab" aria-selected={mode === 'folder'} className={mode === 'folder' ? 'active' : ''} disabled={cloning} onClick={() => setMode('folder')}><FolderOpen size={13} aria-hidden="true" />A folder there</button>
        <button type="button" role="tab" aria-selected={mode === 'clone'} className={mode === 'clone' ? 'active' : ''} disabled={cloning} onClick={() => setMode('clone')}><FolderGit2 size={13} aria-hidden="true" />Clone a repository there</button>
      </div>
      {browseProblem
        ? <p className="new-project-error" role="alert"><CircleAlert size={13} aria-hidden="true" /> {browseProblem}</p>
        : <div className="machine-browse" aria-busy={loading}>
          <div className="machine-browse-path">
            <button type="button" className="queue-icon" aria-label="Up one folder" title="Up one folder" disabled={!browse?.parent || loading || cloning} onClick={() => browse?.parent && void open(browse.parent)}><ChevronLeft size={14} /></button>
            <code title={here ?? undefined}>{here ?? 'Reading…'}</code>
          </div>
          <ul className="machine-browse-list" aria-label={`Folders in ${here ? leafOf(here) : 'the shared folder'}`}>
            {loading && <li><LoaderCircle size={13} className="spin" aria-hidden="true" /> Reading…</li>}
            {!loading && folders.length === 0 && <li className="blob-muted">No folders here.</li>}
            {!loading && folders.map((entry) => <li key={entry.name}><button type="button" disabled={cloning} onClick={() => here && void open(`${here}${here.includes('\\') ? '\\' : '/'}${entry.name}`)}><Folder size={13} aria-hidden="true" />{entry.name}</button></li>)}
          </ul>
          {browse?.truncated && <small className="blob-muted">Only the first 500 entries are shown.</small>}
          <small className="blob-muted">Only folders inside the one {machine.name} shares for new projects can be used.</small>
        </div>}
      {mode === 'clone' && <label className="blob-field"><span>Repository address</span><input value={url} placeholder="https://github.com/you/repo.git" disabled={cloning} onChange={(event) => setUrl(event.target.value)} />{urlProblem && <small className="new-project-error">{urlProblem}</small>}</label>}
      <label className="blob-field"><span>Project name <small>optional</small></span><input value={name} maxLength={120} placeholder={mode === 'clone' ? repoNameOf(url) || 'From the repository' : here ? leafOf(here) : ''} disabled={cloning} onChange={(event) => setName(event.target.value)} /></label>
      {mode === 'clone' && <small className="blob-muted">It clones into the folder shown above, with {machine.name}'s own git sign-in.</small>}
      {job && <p className={`machine-clone ${job.state}`} role="status">{job.state === 'running' ? <><LoaderCircle size={13} className="spin" aria-hidden="true" /> Cloning on {machine.name}…</> : job.state === 'failed' ? `The clone failed: ${job.detail ?? 'no reason given.'}` : job.state === 'cancelled' ? 'The clone was stopped.' : 'Cloned.'}</p>}
      {problem && <p className="new-project-error" role="alert">{problem}</p>}
      <div className="blob-dialog-actions">
        <button type="button" className="secondary-button" disabled={busy || cloning} onClick={onClose}>Cancel</button>
        {mode === 'folder'
          ? <button type="button" className="primary-button" disabled={!here || busy || !!browseProblem || browse?.parent === null} title={browse?.parent === null ? 'Choose a folder inside the shared one' : undefined} onClick={() => void addFolder()}>{busy ? 'Adding…' : 'Use this folder'}</button>
          : <button type="button" className="primary-button" disabled={!here || !url.trim() || !!urlProblem || busy || cloning || !!browseProblem} onClick={() => void clone()}>{busy ? 'Starting…' : cloning ? 'Cloning…' : 'Clone'}</button>}
      </div>
    </div>}
  </Shell>
}

type RemoteStatus = { enabled?: boolean; method?: string; url?: string | null; deviceCount?: number; connectedCount?: number; link?: string }
type Device = { id: string; name: string; kind: string; platform?: string | null; tier: MachineTier; login?: string | null; pairedAt?: string | null; lastSeenAt?: string | null; online?: boolean }
type Pending = { pairingId: string; name: string; kind: string; platform?: string | null; login?: string | null; method?: string; expiresAt?: string } | null
type Activity = { id: string; at: string; deviceName?: string | null; kind: string; summary: string }

const TIERS: Array<{ value: MachineTier; label: string }> = [{ value: 'viewer', label: 'Viewer' }, { value: 'chat', label: 'Chat' }, { value: 'operator', label: 'Operator' }]

/** Another machine's paired devices, its waiting pairing request and its activity. Machines only, at Operator. */
export function ManageMachine({ machine, onClose }: { machine: Machine; onClose: () => void }) {
  const [status, setStatus] = useState<RemoteStatus | null>(null)
  const [devices, setDevices] = useState<Device[] | null>(null)
  const [pending, setPending] = useState<Pending>(null)
  const [activity, setActivity] = useState<Activity[]>([])
  const [problem, setProblem] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [confirmRevoke, setConfirmRevoke] = useState<Device | null>(null)
  const [approveTier, setApproveTier] = useState<MachineTier>('chat')
  const base = { machineId: machine.id }

  const load = useCallback(async () => {
    try {
      const [nextStatus, list, log] = await Promise.all([
        rpc<RemoteStatus>('machine.remote.status', base),
        rpc<{ devices?: Device[] }>('machine.devices.list', base),
        rpc<{ entries?: Activity[] }>('machine.activity', { ...base, limit: 30 }),
      ])
      setStatus(nextStatus); setDevices(list?.devices ?? []); setActivity(log?.entries ?? []); setProblem(null)
    } catch (reason) { setDevices((current) => current ?? []); setProblem(messageOf(reason, `${machine.name} can't be managed from here right now.`)) }
  }, [machine.id, machine.name]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { void load() }, [load])
  // No event reaches another machine for a waiting request, so it is read every 2 seconds while this is open.
  useEffect(() => {
    let live = true
    const read = () => void rpc<{ pending?: Pending }>('machine.pair.pending', base).then((reply) => { if (live) setPending(reply?.pending ?? null) }).catch(() => undefined)
    read()
    const timer = window.setInterval(read, 2000)
    return () => { live = false; window.clearInterval(timer) }
  }, [machine.id]) // eslint-disable-line react-hooks/exhaustive-deps

  const act = async (key: string, work: () => Promise<unknown>) => {
    setBusy(key); setProblem(null)
    try { await work(); await load() } catch (reason) { setProblem(messageOf(reason, `${machine.name} refused that.`)) } finally { setBusy(null) }
  }

  return <Shell title={`Manage ${machine.name}`} onClose={onClose} busy={!!busy} className="machine-manage">
    <div className="machine-remote-body">
      <p className="machine-manage-status"><MachineKindIcon kind={machine.kind} size={14} />
        <span>{status === null ? 'Reading…' : status.enabled ? `Remote is on there, ${status.method === 'native' ? 'over the internet (Native)' : 'over Tailscale'}${status.url ? ` at ${status.url}` : ''}.` : 'Remote is off there.'}{machine.version ? ` Retinue ${machine.version}.` : ''}</span>
      </p>
      {pending && <section className="machine-approval" aria-label="Waiting to pair">
        <strong>{pending.name} wants to pair with {machine.name}</strong>
        <small>{[pending.kind, pending.platform, pending.login].filter(Boolean).join(' · ')}{pending.expiresAt ? ` · expires ${shortTime(pending.expiresAt)}` : ''}</small>
        {pending.kind === 'machine'
          ? <small>Another machine can only be approved at {machine.name} itself.</small>
          : <div className="machine-approval-actions">
            <Select ariaLabel="Level for this device" value={approveTier} onChange={(value) => setApproveTier(value as MachineTier)} options={TIERS} />
            <button type="button" className="secondary-button small" disabled={!!busy} onClick={() => void act('refuse', () => rpc('machine.pair.confirm', { ...base, pairingId: pending.pairingId, approve: false }))}>Refuse</button>
            <button type="button" className="primary-button small" disabled={!!busy} onClick={() => void act('approve', () => rpc('machine.pair.confirm', { ...base, pairingId: pending.pairingId, approve: true, tier: approveTier }))}>{busy === 'approve' ? 'Approving…' : 'Approve'}</button>
          </div>}
      </section>}
      <h3 className="machine-remote-h">Devices paired with {machine.name}</h3>
      {devices === null && <p className="blob-muted"><LoaderCircle size={13} className="spin" aria-hidden="true" /> Reading…</p>}
      {devices?.length === 0 && !problem && <p className="blob-muted">Nothing is paired there.</p>}
      {devices && devices.length > 0 && <ul className="machine-devices">{devices.map((device) => <li key={device.id}>
        {device.kind === 'machine' ? <MachineKindIcon kind={null} size={14} /> : <Smartphone size={14} aria-hidden="true" />}
        <span className="machine-device-copy"><strong>{device.name}</strong><small>{[device.platform, device.online ? 'connected' : device.lastSeenAt ? `seen ${shortTime(device.lastSeenAt)}` : null].filter(Boolean).join(' · ')}</small></span>
        <Select ariaLabel={`Level for ${device.name}`} value={device.tier} disabled={!!busy} onChange={(value) => void act(`tier-${device.id}`, () => rpc('machine.devices.update', { ...base, deviceId: device.id, tier: value }))} options={TIERS} />
        <button type="button" className="queue-icon" aria-label={`Remove ${device.name} from ${machine.name}`} title="Remove" disabled={!!busy} onClick={() => setConfirmRevoke(device)}><Trash2 size={13} /></button>
      </li>)}</ul>}
      {confirmRevoke && <div className="rail-confirm" role="group" aria-label={`Confirm removing ${confirmRevoke.name}`}>
        <span>Remove <b>{confirmRevoke.name}</b> from {machine.name}? It is signed out there at once and must pair again to come back.</span>
        <span className="rail-confirm-actions">
          <button type="button" className="secondary-button small" disabled={!!busy} onClick={() => setConfirmRevoke(null)}>Keep</button>
          <button type="button" className="primary-button small" disabled={!!busy} onClick={() => { const target = confirmRevoke; setConfirmRevoke(null); void act(`revoke-${target.id}`, () => rpc('machine.devices.revoke', { ...base, deviceId: target.id })) }}>Remove</button>
        </span>
      </div>}
      {problem && <p className="new-project-error" role="alert">{problem}</p>}
      {activity.length > 0 && <>
        <h3 className="machine-remote-h">Recent activity there</h3>
        <ul className="machine-activity">{activity.slice(0, 12).map((entry) => <li key={entry.id}><small>{shortTime(entry.at)}</small><span>{entry.summary}</span></li>)}</ul>
      </>}
      <small className="blob-muted">Changes you make here are written in {machine.name}'s own activity list, with this machine's name.</small>
    </div>
  </Shell>
}
