// A bot on another machine. Its conversation runs there (only one machine runs a conversation); from here
// the owner reads it live, messages, steers or stops it, answers its approvals, or takes it over.
// Since 17 November 2026 the daemon holds a live socket to each linked machine and announces changes
// with `machine.session.changed` (ids only), so this view re-reads on change and polls only as a fallback.
import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { ArrowUp, CircleAlert, CircleCheck, FolderGit2, GitBranch, Info, Laptop, LoaderCircle, Plus, RefreshCw, SendHorizontal, Square, X } from 'lucide-react'
import type { PermissionRequest, Session } from '../types'
import { rpc } from '../tauri'
import { useDialogAccessibility, useDialogEntered } from './dialogFocus'
import { canChat, canTakeOver, machineBusyBots, normalizeMachineSessions, useMachineSessionChanges, useMachines, type Machine, type MachineBot, type TakeOverJob, type TakeOverPreview } from './machines'
import { doingNowLabel } from './doingNow'
import { MachineBadge, MachineKindIcon, openMachineBotInMainWindow } from './MachineBots'
import { BlobCanvas } from '../blob/BlobCanvas'
import { agentLook } from '../blob/look'
import { agentColorHex } from './agentColor'
import type { Agent } from '../types'
import { openSafeLink } from './openSafeLink'
import { SafeMarkdown } from './SafeMarkdown'
import { shortTime } from './rosterSelectors'
import { useWorkMode } from './workMode'
import { RuntimeMark, providerBrand } from './providerBrand'
import { ProBadge } from './ProBadge'
import { WhereIcon, whereKindOf } from './ProfessionalDevices'
import { sessionIsBusy } from './teamSelectors'
import './machines.css'
import { LoadingBlock } from './Skeleton'

const messageOf = (reason: unknown, fallback: string) => reason instanceof Error && reason.message ? reason.message : typeof reason === 'string' && reason ? reason : fallback
const POLL_MS = 3000
/** With a live link, a slow safety read in case an event was missed. */
const LIVE_POLL_MS = 20000

type Conversation = { id: string; title?: string | null; updatedAt?: string | null; projectName?: string | null; state?: string | null; link?: Session['link']; offline?: boolean }
const STEP_WORD: Record<string, string> = { prepare: 'Saved the work on the other machine', code: 'Got the code', bot: 'Matched the employee', import: 'Copied the conversation', commit: 'Handed over', open: 'Opened it here' }


function conversationsFrom(raw: unknown): Conversation[] {
  const rows = raw && typeof raw === 'object' ? (raw as { conversations?: unknown }).conversations : null
  return Array.isArray(rows) ? rows.filter((row): row is Conversation => !!row && typeof (row as Conversation).id === 'string') : []
}

/** The sentence and button for what's possible with this conversation. */
export function takeOverState(machine: Machine, session: Pick<Session, 'link'> | null, selfId: string | null): { allowed: boolean; note: string } {
  const movedTo = session?.link?.movedTo
  if (movedTo) return { allowed: false, note: movedTo.machineId === selfId ? 'This conversation moved to this machine.' : `This conversation moved to ${movedTo.name}.` }
  if (machine.state === 'revoked') return { allowed: false, note: `${machine.name} removed this link. Pair again in Settings, Machines.` }
  if (machine.state !== 'online') return { allowed: false, note: `${machine.name} can't be reached, so this may be out of date.` }
  if (!canTakeOver(machine)) return { allowed: false, note: `Read only. To take conversations over, give this machine the Operator level in Settings, Remote, on ${machine.name}.` }
  return { allowed: true, note: `Running on ${machine.name}. Take it over to run it on this machine instead.` }
}

export function TakeOverDialog({ machine, sessionId, onClose, onMoved }: { machine: Machine; sessionId: string; onClose: () => void; onMoved: (sessionId: string) => void }) {
  const entered = useDialogEntered()
  const [preview, setPreview] = useState<TakeOverPreview | null>(null)
  const [failure, setFailure] = useState<string | null>(null)
  const [job, setJob] = useState<TakeOverJob | null>(null)
  const [starting, setStarting] = useState(false)
  const [consent, setConsent] = useState(false)
  const running = starting || job?.state === 'running'
  const { ref, close } = useDialogAccessibility(onClose, () => !running)
  const jobId = job?.state === 'running' ? job.id : null

  useEffect(() => {
    let live = true
    void rpc<TakeOverPreview>('machine.takeOver.preview', { machineId: machine.id, sessionId })
      .then((reply) => { if (live) setPreview(reply) })
      .catch((reason) => { if (live) setFailure(messageOf(reason, 'This conversation could not be checked.')) })
    return () => { live = false }
  }, [machine.id, sessionId])

  useEffect(() => {
    if (!jobId) return
    const timer = window.setInterval(() => {
      void rpc<{ jobs?: TakeOverJob[] }>('machine.takeOver.list', {}).then((reply) => {
        const next = reply?.jobs?.find((item) => item.id === jobId)
        if (next) setJob(next)
      }).catch(() => undefined)
    }, 1000)
    return () => window.clearInterval(timer)
  }, [jobId])

  const uncommitted = preview?.uncommitted ?? 0
  const ownCopy = !!preview?.repo.isWorktree
  const needsConsent = ownCopy && uncommitted > 0
  const blocked = !preview || !preview.ready || !preview.runtimeReady || preview.needsProject
  const start = async () => {
    setStarting(true); setFailure(null)
    try {
      const reply = await rpc<{ job: TakeOverJob }>('machine.takeOver', { machineId: machine.id, sessionId, projectId: preview?.project?.id ?? null, commitUncommitted: needsConsent && consent, confirm: true })
      setJob(reply.job)
    } catch (reason) { setFailure(messageOf(reason, 'This conversation could not be taken over.')) } finally { setStarting(false) }
  }
  const done = job?.state === 'done'

  // Portalled: this opens from inside other dialogs, and a transformed parent would trap a fixed backdrop.
  return createPortal(<div className={`sheet-backdrop blob-dialog-backdrop ${entered ? 'is-entered' : ''}`}>
    <section ref={ref} className="blob-dialog take-over" role="dialog" aria-modal="true" aria-labelledby="take-over-title" tabIndex={-1}>
      <h2 id="take-over-title">{done ? "It's on this machine now" : job?.state === 'failed' ? 'It stayed where it was' : running ? 'Taking it over…' : 'Take over this conversation?'}</h2>
      {!preview && !failure && <p role="status"><LoaderCircle size={14} className="spin" aria-hidden="true" /> Checking with {machine.name}…</p>}
      {preview && !job && <>
        <p>It moves to this machine. {machine.name} keeps a read-only copy, and only this machine can run it from then on.</p>
        <ul className="take-over-facts">
          <li><Laptop size={14} aria-hidden="true" /><span>{preview.bot.willCreate ? `This machine gets a copy of ${preview.bot.name}, without Full access or environment values.` : `${preview.bot.name} carries on here, in a fresh session with a summary.`}</span></li>
          {preview.repo.hasRemote && ownCopy && <li><GitBranch size={14} aria-hidden="true" /><span>The branch <code>{preview.repo.branch}</code> is pushed from {machine.name} and fetched here.</span></li>}
          {preview.repo.identity && preview.project && <li><FolderGit2 size={14} aria-hidden="true" /><span>It works in your project {preview.project.name}.</span></li>}
          {!preview.repo.hasRemote && <li><Info size={14} aria-hidden="true" /><span>This repository has no remote, so the files stay on {machine.name}. Only the conversation moves.</span></li>}
          {preview.repo.hasRemote && !ownCopy && <li><Info size={14} aria-hidden="true" /><span>This conversation has no copy of its own, so its files stay on {machine.name}{uncommitted > 0 ? `, including ${uncommitted} uncommitted file${uncommitted === 1 ? '' : 's'}` : ''}.</span></li>}
          <li><Info size={14} aria-hidden="true" /><span>Pictures and tool steps stay on {machine.name}.</span></li>
        </ul>
        {needsConsent && <label className="new-project-check"><input type="checkbox" checked={consent} onChange={(event) => setConsent(event.target.checked)} /><span>Commit {uncommitted} uncommitted file{uncommitted === 1 ? '' : 's'} to that branch first</span></label>}
        {!preview.ready && <p className="new-project-error" role="alert">{preview.reason}</p>}
        {preview.ready && !preview.runtimeReady && <p className="new-project-error" role="alert">{preview.reason ?? `This machine has no coding agent for ${preview.bot.name}.`}</p>}
        {preview.ready && preview.runtimeReady && preview.needsProject && <p className="new-project-error" role="alert">This machine doesn't have <code>{preview.repo.identity}</code> yet. Add it in New project, Clone, then try again.</p>}
      </>}
      {job && <ul className="take-over-steps" aria-label="Progress">
        {job.steps.map((step) => <li key={step.id} data-status={step.status}>
          {step.status === 'failed' ? <CircleAlert size={13} aria-hidden="true" /> : <CircleCheck size={13} aria-hidden="true" />}
          <span>{step.status === 'failed' ? step.detail ?? 'That step failed.' : step.status === 'skipped' ? `Skipped: ${(STEP_WORD[step.id] ?? step.id).toLowerCase()}` : STEP_WORD[step.id] ?? step.id}</span>
        </li>)}
        {running && <li><LoaderCircle size={13} className="spin" aria-hidden="true" /><span>Working…</span></li>}
      </ul>}
      {job?.state === 'failed' && <p className="new-project-error" role="alert">{job.detail ?? 'It stayed on the other machine.'} The conversation stays on {machine.name}.</p>}
      {failure && <p className="new-project-error" role="alert">{failure}</p>}
      <div className="blob-dialog-actions">
        {done
          ? <button type="button" className="primary-button" onClick={() => onMoved(job.sessionId ?? sessionId)}>Open it</button>
          : <>
            <button type="button" className="secondary-button" disabled={running} onClick={close}>{job ? 'Close' : 'Cancel'}</button>
            {!job && <button type="button" className="primary-button" disabled={blocked || running || (needsConsent && !consent)} onClick={() => void start()}>{starting ? 'Starting…' : 'Take over'}</button>}
          </>}
      </div>
    </section>
  </div>, document.body)
}

/** In place of the composer: another machine runs this conversation now. */
export function MovedBar({ sessionId, movedTo, machines, onMoved }: { sessionId: string; movedTo: { machineId: string; name: string }; machines: readonly Machine[]; onMoved: () => void }) {
  const [takingOver, setTakingOver] = useState(false)
  const machine = machines.find((item) => item.id === movedTo.machineId) ?? null
  const allowed = !!machine && canTakeOver(machine)
  const note = !machine ? `To take it back, link ${movedTo.name} in Settings, Machines.` : !allowed ? machine.state === 'online' ? `To take it back, give this machine the Operator level on ${machine.name}.` : `${machine.name} can't be reached right now.` : 'Read only here.'
  return <div className="moved-bar" role="status">
    <Laptop size={15} aria-hidden="true" />
    <span><strong>Moved to {movedTo.name}.</strong> {note}</span>
    <button type="button" className="primary-button small" disabled={!allowed} onClick={() => setTakingOver(true)}>Take over</button>
    {takingOver && machine && <TakeOverDialog machine={machine} sessionId={sessionId} onClose={() => setTakingOver(false)} onMoved={() => { setTakingOver(false); onMoved() }} />}
  </div>
}

/** The companion: bots working on another machine, each with that machine's name. */
export function CompanionMachineBots() {
  const { machines } = useMachines()
  const busy = machineBusyBots(machines)
  if (!busy.length) return null
  return <div className="companion-machine-bots" aria-label="Working on other machines">
    {busy.map(({ machine, bot }) => <button type="button" key={`${machine.id}-${bot.id}`} className="companion-machine-bot" aria-label={`${bot.name}, working on ${machine.name}`} title={`${bot.name} is working on ${machine.name}`} onClick={() => void openMachineBotInMainWindow(machine.id, bot.id).catch(() => undefined)}>
      <strong>{bot.name}</strong><MachineBadge name={machine.name} />
    </button>)}
  </div>
}

/** The message box for a conversation that runs on another machine. Sends through this machine's link; the
 * other machine runs the turn and its own coding agent. While a turn runs, a message goes in as a steer. */
export /** What a machine's bot is called on screen: Professional mode names its provider, never the employee. */
function useBotLabel(bot: MachineBot): string {
  const professional = useWorkMode() === 'professional'
  return professional && bot.provider ? providerBrand(bot.provider).name : bot.name
}

function MachineComposer({ machine, bot, sessionId, busy, onSent }: { machine: Machine; bot: MachineBot; sessionId: string; busy: boolean; onSent: () => void }) {
  const label = useBotLabel(bot)
  const professional = useWorkMode() === 'professional'
  const [text, setText] = useState('')
  const [sending, setSending] = useState<'send' | 'stop' | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const ready = text.trim().length > 0 && !sending
  const send = async () => {
    if (!ready) return
    setSending('send'); setProblem(null)
    try {
      await rpc(busy ? 'machine.steer' : 'machine.prompt', { machineId: machine.id, sessionId, text: text.trim() })
      setText('')
      onSent()
    } catch (reason) { setProblem(messageOf(reason, `${machine.name} didn't take that message.`)) } finally { setSending(null) }
  }
  const stop = async () => {
    setSending('stop'); setProblem(null)
    try { await rpc('machine.cancel', { machineId: machine.id, sessionId }); onSent() } catch (reason) { setProblem(messageOf(reason, `${machine.name} didn't stop it.`)) } finally { setSending(null) }
  }
  if (professional) return <div className="composer-wrap pro-remote-composer"><form className="composer-box" onSubmit={(event) => { event.preventDefault(); void send() }}>
    <textarea value={text} rows={1} aria-label={`Message ${label} on ${machine.name}`} placeholder={busy ? `Add to what ${label} is doing` : `Message ${label}`} onChange={(event) => setText(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); void send() } }} />
    {problem && <p className="machine-composer-problem" role="alert">{problem}</p>}
    <div className="composer-control-strip">
      <div className="composer-controls-start"><span className="composer-pill pro-remote-pill" title={`Runs on ${machine.name}, with its own coding agent and files`}><WhereIcon where={{ kind: whereKindOf(machine), name: machine.name }} size={13} /><span>{machine.name}</span></span></div>
      <div className="composer-controls-end">
        {busy && <button type="button" className="composer-icon-button" aria-label="Stop" title="Stop" disabled={!!sending} onClick={() => void stop()}><Square size={13} aria-hidden="true" /></button>}
        <button type="submit" className="send-button" disabled={!ready} aria-label={busy ? 'Send now' : 'Send message'} title={busy ? 'Send now' : 'Send'}><ArrowUp size={16} aria-hidden="true" /></button>
      </div>
    </div>
  </form></div>
  return <form className="machine-composer" onSubmit={(event) => { event.preventDefault(); void send() }}>
    <textarea value={text} rows={2} aria-label={`Message ${label} on ${machine.name}`} placeholder={busy ? `Add to what ${label} is doing` : `Message ${label}`} onChange={(event) => setText(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); void send() } }} />
    {problem && <p className="machine-composer-problem" role="alert">{problem}</p>}
    <div className="machine-composer-foot">
      <span><Laptop size={12} aria-hidden="true" />Runs on {machine.name}, with its own coding agent and files.</span>
      {busy && <button type="button" className="secondary-button small" disabled={!!sending} onClick={() => void stop()}><Square size={11} aria-hidden="true" />{sending === 'stop' ? 'Stopping…' : 'Stop'}</button>}
      <button type="submit" className="primary-button small" disabled={!ready}><SendHorizontal size={12} aria-hidden="true" />{sending === 'send' ? 'Sending…' : busy ? 'Send now' : 'Send'}</button>
    </div>
  </form>
}

export function MachineConversation({ machine, bot, initialSessionId, selfId, onClose, onMoved }: { machine: Machine; bot: MachineBot; initialSessionId?: string | null; selfId: string | null; onClose: () => void; onMoved: (sessionId: string) => void }) {
  const entered = useDialogEntered()
  const [takingOver, setTakingOver] = useState(false)
  const { ref, close } = useDialogAccessibility(onClose, () => !takingOver)
  const [conversations, setConversations] = useState<Conversation[] | null>(null)
  const [selected, setSelected] = useState<string | null>(initialSessionId ?? bot.activeSessionId)
  const [session, setSession] = useState<Session | null>(null)
  const [failure, setFailure] = useState<string | null>(null)
  const scroller = useRef<HTMLDivElement>(null)
  const pinned = useRef(true)
  const [reread, setReread] = useState(0)
  const [listTick, setListTick] = useState(0)
  const [approvals, setApprovals] = useState<PermissionRequest[]>([])
  const [answering, setAnswering] = useState<string | null>(null)
  const [starting, setStarting] = useState(false)
  const chat = canChat(machine)
  const startConversation = async () => {
    setStarting(true); setFailure(null)
    try {
      const reply = await rpc<{ sessionId?: string; session?: { id?: string } }>('machine.start', { machineId: machine.id, agentId: bot.id })
      const id = reply?.sessionId ?? reply?.session?.id ?? null
      if (id) {
        setConversations((rows) => rows?.some((row) => row.id === id) ? rows : [{ id, title: 'New conversation', updatedAt: new Date().toISOString() }, ...(rows ?? [])])
        setSelected(id)
      }
      setReread((value) => value + 1)
    } catch (reason) { setFailure(messageOf(reason, `${machine.name} didn't start a conversation.`)) } finally { setStarting(false) }
  }

  useEffect(() => {
    let live = true
    const load = () => rpc('machine.conversations', { machineId: machine.id, agentId: bot.id }).then((reply) => {
      if (!live) return
      const rows = conversationsFrom(reply)
      // Keep a conversation just started here while the other machine's list catches up.
      setConversations((current) => [...(current ?? []).filter((row) => !rows.some((item) => item.id === row.id) && row.title === 'New conversation'), ...rows])
      setSelected((current) => current ?? rows[0]?.id ?? null)
    }).catch(async (reason) => {
      // The machine can't be reached: show its last known conversations, greyed and read-only.
      const known = await rpc('machine.sessions', { machineId: machine.id }).then(normalizeMachineSessions).catch(() => [])
      if (!live) return
      const rows = known.filter((row) => row.agentId === bot.id && !row.archived).map((row): Conversation => ({ id: row.id, title: row.title, updatedAt: row.updatedAt, projectName: row.projectName, state: row.state, offline: true }))
      setConversations((current) => rows.length ? rows : current ?? [])
      setSelected((current) => current ?? rows[0]?.id ?? null)
      setFailure(messageOf(reason, `${machine.name} can't be reached right now.`))
    })
    void load()
    const timer = window.setInterval(() => void load(), machine.live ? LIVE_POLL_MS : 15000)
    return () => { live = false; window.clearInterval(timer) }
  }, [machine.id, bot.id, machine.name, machine.live, reread])

  useEffect(() => {
    if (!selected) { setSession(null); return }
    let live = true
    setSession(null)
    pinned.current = true
    const load = () => {
      void rpc<{ session?: Session }>('machine.conversation', { machineId: machine.id, sessionId: selected }).then((reply) => {
        if (live) { setSession(reply?.session ?? null); setFailure(null) }
      }).catch((reason) => { if (live) setFailure(messageOf(reason, `${machine.name} can't be reached right now.`)) })
      // Approvals waiting there, so they can be answered here. An older machine has no such read.
      void rpc<{ permissions?: PermissionRequest[] }>('machine.permissions', { machineId: machine.id, sessionId: selected }).then((reply) => {
        if (live) setApprovals(Array.isArray(reply?.permissions) ? reply.permissions.filter((card) => card && typeof card.id === 'string' && card.kind !== 'question' && card.kind !== 'proposal') : [])
      }).catch(() => { if (live) setApprovals([]) })
    }
    load()
    const timer = window.setInterval(() => { if (!document.hidden) load() }, machine.live ? LIVE_POLL_MS : POLL_MS)
    return () => { live = false; window.clearInterval(timer) }
  }, [machine.id, selected, machine.name, machine.live, reread])

  // Live: re-read when the other machine reports a change to this bot's conversations.
  useMachineSessionChanges(true, (change) => {
    if (change.machineId !== machine.id) return
    if (change.sessionId === selected) setReread((value) => value + 1)
    else setListTick((value) => value + 1)
  })
  useEffect(() => {
    if (!listTick) return
    void rpc('machine.conversations', { machineId: machine.id, agentId: bot.id }).then((reply) => {
      const rows = conversationsFrom(reply)
      if (rows.length) setConversations((current) => [...(current ?? []).filter((row) => !rows.some((item) => item.id === row.id) && row.title === 'New conversation'), ...rows])
    }).catch(() => undefined)
  }, [listTick, machine.id, bot.id])

  useEffect(() => {
    const element = scroller.current
    if (element && pinned.current) element.scrollTop = element.scrollHeight
  }, [session?.messages?.length])

  const state = takeOverState(machine, session, selfId)
  const messages = (session?.messages ?? []).filter((message) => ['user', 'assistant', 'notice'].includes(String(message.role)) && String(message.content ?? '').trim())
  // The work log: between the owner's own machines it carries tool titles and details (owner decision,
  // 17 November 2026); an older machine still sends the bare word "Tool", which is left out.
  const steps = (session?.tools ?? []).filter((tool) => String(tool.title ?? '').trim() && String(tool.title) !== 'Tool' && !tool.parentToolCallId)
  const stamp = (value: unknown) => { const at = Date.parse(String(value ?? '')); return Number.isFinite(at) ? at : 0 }
  const items: Array<{ kind: 'message'; at: number; message: (typeof messages)[number] } | { kind: 'step'; at: number; step: (typeof steps)[number] }> = [
    ...messages.map((message) => ({ kind: 'message' as const, at: stamp(message.createdAt), message })),
    ...steps.map((step) => ({ kind: 'step' as const, at: stamp(step.startedAt ?? step.completedAt), step })),
  ].sort((a, b) => a.at - b.at)
  const answer = async (card: PermissionRequest, allow: boolean) => {
    setAnswering(card.id); setFailure(null)
    try {
      await rpc('machine.permission.reply', { machineId: machine.id, permissionId: card.id, choice: allow ? 'allow_once' : 'deny' })
      setApprovals((rows) => rows.filter((row) => row.id !== card.id))
      setReread((value) => value + 1)
    } catch (reason) { setFailure(messageOf(reason, `${machine.name} didn't take that answer.`)) } finally { setAnswering(null) }
  }
  const busy = sessionIsBusy(session)
  const movedAway = !!session?.link?.movedTo
  const waitingForYou = session?.state === 'waiting_permission'

  // A conversation on another machine opens in the conversation pane, as if it ran here, marked as remote (23 November 2026);
  // with no pane around it (the component on its own) it renders in place.
  const host = typeof document === 'undefined' ? null : document.querySelector<HTMLElement>('.conversation-pane')
  const remoteOnline = machine.state === 'online'
  const professional = useWorkMode() === 'professional'
  const botLabel = useBotLabel(bot)
  const side = <div className="machine-convos">
          <div className="convo-head"><strong>Conversations</strong><span className="machine-remote-chip" title={`These conversations run on ${machine.name}`}><MachineKindIcon kind={machine.kind} size={11} />{machine.name}</span></div>
          {chat
            ? <button type="button" className="convo-new machine-convo-new" disabled={starting} onClick={() => void startConversation()}><Plus size={13} aria-hidden="true" />{starting ? 'Starting…' : 'New conversation'}</button>
            : <span />}
          <ul className="machine-view-list" aria-label={`${bot.name}'s conversations`}>
            {conversations === null && <li><LoadingBlock label="Loading…" lines={3} /></li>}
            {conversations?.length === 0 && <li><small>No conversations yet.</small></li>}
            {conversations?.map((item) => <li key={item.id} className={item.offline ? 'offline' : undefined}><button type="button" aria-current={item.id === selected ? 'true' : undefined} title={item.offline ? `${item.title || 'Conversation'}. Last known, while ${machine.name} can't be reached` : item.title || undefined} onClick={() => setSelected(item.id)}>
              <strong>{item.title || 'Conversation'}</strong>
              <small>{[item.projectName, item.updatedAt ? shortTime(item.updatedAt) : null].filter(Boolean).join(' · ')}</small>
            </button></li>)}
          </ul>
        </div>
  const sideHost = host?.closest('.app-shell')?.querySelector<HTMLElement>('.conversations-panel') ?? null
  const pane = <section ref={ref} className={`machine-pane${entered ? ' is-entered' : ''}`} role="dialog" aria-label={`${bot.name} on ${machine.name}`} data-remote-state={remoteOnline ? (machine.live ? 'live' : 'online') : 'offline'} tabIndex={-1}>
      {professional && <header className="chat-header pro-machine-head">
        <div className="chat-title"><RuntimeMark provider={bot.provider} size={16} /><span className="conversation-crumb" title={`${machine.name} / ${session?.title ?? ''}`}><span className="conversation-crumb-project">{machine.name}</span><span className="conversation-crumb-sep" aria-hidden="true">/</span><span className="conversation-crumb-title">{String(session?.title ?? '').trim() || 'Chat'}</span></span></div>
        <div className="header-actions"><ProBadge tone={remoteOnline ? 'online' : 'offline'} label={remoteOnline ? 'Online' : 'Offline'} />{selected && (!chat || movedAway || canTakeOver(machine)) && <button type="button" className="ghost-button small" title={state.note} disabled={!state.allowed || !session} onClick={() => setTakingOver(true)}>Take over</button>}</div>
      </header>}
      {professional && <div className="conversation-frame">
        <section ref={scroller} className="message-list" tabIndex={0} aria-label={chat ? 'Conversation' : 'Conversation, read only'} onScroll={(event) => { const element = event.currentTarget; pinned.current = element.scrollHeight - element.scrollTop - element.clientHeight < 48 }}>
          <div className="conversation-layout"><div className="chat-column">
            {selected && !session && !failure && <p className="pro-remote-note" role="status"><LoaderCircle size={14} className="spin" aria-hidden="true" /> Loading…</p>}
            {!selected && conversations?.length !== 0 && conversations !== null && <p className="pro-remote-note">Choose a conversation.</p>}
            {items.map((item) => item.kind === 'step'
              ? <p key={`step-${String(item.step.id)}`} className="pro-remote-step" data-state={String(item.step.state ?? '')} title={String(item.step.title ?? '')}>{doingNowLabel(item.step)}</p>
              : item.message.role === 'user'
                ? <div key={String(item.message.id)} className="message-action-shell user"><article className="message-row user" aria-label="You"><div className="message-content"><div className="message-bubble-wrap"><div className="message-bubble"><div className="safe-markdown"><p><span>{String(item.message.content)}</span></p></div></div></div><div className="message-time">{item.message.createdAt ? <time>{shortTime(String(item.message.createdAt))}</time> : null}</div></div></article></div>
                : <div key={String(item.message.id)} className="message-action-shell"><article className={`message-row ${item.message.role === 'notice' ? 'notice' : 'agent'}`} aria-label={botLabel}><span className="message-avatar" aria-hidden="true" /><div className="message-content"><div className="message-bubble-wrap"><div className="message-bubble">{item.message.role === 'assistant' ? <SafeMarkdown text={String(item.message.content)} onOpenLink={openSafeLink} /> : <div className="safe-markdown"><p><span>{String(item.message.content)}</span></p></div>}</div></div></div></article></div>)}
            {busy && !waitingForYou && <p className="pro-remote-working" role="status">{botLabel} is working on {machine.name}<span className="pro-dots" aria-hidden="true"><i /><i /><i /></span></p>}
            {approvals.map((card) => <section key={card.id} className="machine-approval" aria-label={`Approval on ${machine.name}`}>
              <strong>{card.title || `${botLabel} asks to continue`}</strong>
              {(card.command || card.description) && <code>{String(card.command || card.description)}</code>}
              <small>Asked on {machine.name}. Your answer goes there.</small>
              <div className="machine-approval-actions">
                <button type="button" className="secondary-button small" disabled={!!answering || !canTakeOver(machine)} onClick={() => void answer(card, false)}>Deny</button>
                <button type="button" className="primary-button small" disabled={!!answering || !canTakeOver(machine)} onClick={() => void answer(card, true)}>{answering === card.id ? 'Sending…' : 'Allow'}</button>
              </div>
            </section>)}
            {waitingForYou && !approvals.length && <p className="pro-remote-note" role="status">{botLabel} is waiting for an approval. Answer it on {machine.name}.</p>}
          </div></div>
        </section>
      </div>}
      {professional && failure && <p className="new-project-error pro-remote-problem" role="alert">{failure}</p>}
      {professional && selected && chat && !movedAway && session && <MachineComposer machine={machine} bot={bot} sessionId={selected} busy={busy} onSent={() => { pinned.current = true; setReread((value) => value + 1) }} />}
      {professional && selected && (!chat || movedAway) && <p className="pro-remote-note pro-remote-readonly">{state.note}</p>}
      {!professional && <header className="machine-view-head">
        <span className="machine-pane-avatar"><BlobCanvas decorative color={agentColorHex(bot.color ?? 'mint')} size={34} mood={!remoteOnline ? 'offline' : busy ? 'tool_activity' : 'idle'} look={agentLook(bot as unknown as Agent)} label={bot.name} /><span className="machine-pane-device" title={`Running on ${machine.name}`}><MachineKindIcon kind={machine.kind} size={10} /></span></span>
        <div className="machine-view-title">
          <h2 id="machine-view-title"><span className="machine-pane-name" title={bot.name}>{bot.name}</span><span className="machine-remote-chip" title={`This conversation runs on ${machine.name}`}><MachineKindIcon kind={machine.kind} size={11} />{machine.name}</span></h2>
          <small>On {machine.name}{machine.lastSeenAt && machine.state !== 'online' ? ` · last seen ${shortTime(machine.lastSeenAt)}` : ''} <span className={`machine-live-mark${machine.live && machine.state === 'online' ? ' live' : ''}`} title={machine.live ? 'Changes arrive as they happen.' : 'No live link right now; this view checks every few seconds.'}><i aria-hidden="true" />{machine.state !== 'online' ? 'Offline' : machine.live ? 'Live' : 'Checking every few seconds'}</span></small>
        </div>
        <button type="button" className="icon-button" aria-label="Close" onClick={close}><X size={16} aria-hidden="true" /></button>
      </header>}
      {!professional && <div className="machine-view-layout">
        <div className="machine-view-main">
          <div ref={scroller} className="machine-transcript" tabIndex={0} aria-label={chat ? "Conversation" : "Conversation, read only"} onScroll={(event) => { const element = event.currentTarget; pinned.current = element.scrollHeight - element.scrollTop - element.clientHeight < 48 }}>
            {selected && !session && !failure && <p role="status"><LoaderCircle size={14} className="spin" aria-hidden="true" /> Loading…</p>}
            {!selected && conversations?.length !== 0 && conversations !== null && <p>Choose a conversation.</p>}
            {items.map((item) => item.kind === 'step'
              ? <p key={`step-${String(item.step.id)}`} className="machine-step" data-state={String(item.step.state ?? '')}><span aria-hidden="true">·</span><code title={String(item.step.title ?? '')}>{doingNowLabel(item.step)}</code></p>
              : <article key={String(item.message.id)} className={`machine-message ${String(item.message.role)}`}>
                {item.message.role !== 'notice' && <small>{item.message.role === 'user' ? 'You' : bot.name}{item.message.createdAt ? ` · ${shortTime(String(item.message.createdAt))}` : ''}</small>}
                {item.message.role === 'assistant'
                  ? <SafeMarkdown text={String(item.message.content)} onOpenLink={openSafeLink} />
                  : <div>{String(item.message.content)}</div>}
              </article>)}
            {busy && !waitingForYou && <p className="machine-message notice" role="status">{botLabel} is working on {machine.name}…</p>}
            {approvals.map((card) => <section key={card.id} className="machine-approval" aria-label={`Approval on ${machine.name}`}>
              <strong>{card.title || `${botLabel} asks to continue`}</strong>
              {(card.command || card.description) && <code>{String(card.command || card.description)}</code>}
              <small>Asked on {machine.name}. Your answer goes there; {machine.name} may refuse it if approvals from other devices are off there.</small>
              <div className="machine-approval-actions">
                <button type="button" className="secondary-button small" disabled={!!answering || !canTakeOver(machine)} onClick={() => void answer(card, false)}>Deny</button>
                <button type="button" className="primary-button small" disabled={!!answering || !canTakeOver(machine)} onClick={() => void answer(card, true)}>{answering === card.id ? 'Sending…' : 'Allow'}</button>
              </div>
              {!canTakeOver(machine) && <small>Answering needs the Operator level for this machine on {machine.name}.</small>}
            </section>)}
            {waitingForYou && !approvals.length && <p className="machine-message notice" role="status">{botLabel} is waiting for an approval. Answer it on {machine.name}.</p>}
          </div>
          {failure && <p className="new-project-error" role="alert">{failure}</p>}
          {selected && chat && !movedAway && session && <MachineComposer machine={machine} bot={bot} sessionId={selected} busy={busy} onSent={() => { pinned.current = true; setReread((value) => value + 1) }} />}
          {selected && (!chat || movedAway || canTakeOver(machine)) && <div className="machine-owner-bar">
            <Laptop size={14} aria-hidden="true" />
            <span>{state.note}</span>
            {machine.state === 'offline' && <RefreshCw size={13} aria-hidden="true" />}
            <button type="button" className="primary-button small" disabled={!state.allowed || !session} onClick={() => setTakingOver(true)}>Take over</button>
          </div>}
        </div>
      </div>}
      {takingOver && selected && <TakeOverDialog machine={machine} sessionId={selected} onClose={() => setTakingOver(false)} onMoved={onMoved} />}
    </section>
  return host ? <>{createPortal(pane, host)}{sideHost && createPortal(side, sideHost)}</> : <>{pane}{side}</>
}
