// The phone works on the machines its host is linked to (owner amendment of 17 November 2026): it pairs
// with one host and reaches the others through that host's own links, at the lower of the two tiers.
// It may watch, start and steer; never link, rename, remove, move or manage machines. Everything here goes
// through `machine.*` on the host, which forwards to the other machine; the other machine's gateway decides.
import { useCallback, useEffect, useRef, useState } from 'react'
import { ArrowLeft, Laptop, Monitor, Plus, Send, Server, Square } from 'lucide-react'
import { listenForDaemonEvents, rpc } from '../tauri'
import type { PermissionRequest, Session } from '../types'
import { BlobCanvas } from '../blob/BlobCanvas'
import { agentLook } from '../blob/look'
import { agentColorHex } from '../ui/agentColor'
import { ApprovalCard } from '../ui/ApprovalCard'
import { normalizeMachineSessions, normalizeMachines, type Machine, type MachineBot, type MachineSessionRow } from '../ui/machines'
import type { RemoteTier } from './remoteContract'

const messageOf = (reason: unknown, fallback: string) => reason instanceof Error && reason.message ? reason.message : typeof reason === 'string' && reason ? reason : fallback
const TIER_RANK: Record<string, number> = { viewer: 0, chat: 1, operator: 2 }

/** The level the phone works at on another machine: the lower of its own and the host's link. */
export function effectiveTier(phone: RemoteTier, link: string): RemoteTier {
  const rank = Math.min(TIER_RANK[phone] ?? 0, TIER_RANK[link] ?? 0)
  return rank >= 2 ? 'operator' : rank === 1 ? 'chat' : 'viewer'
}

function KindIcon({ kind }: { kind: string | null }) {
  if (kind === 'linux') return <Server size={14} aria-hidden="true" />
  if (kind === 'windows') return <Monitor size={14} aria-hidden="true" />
  return <Laptop size={14} aria-hidden="true" />
}

/** The host's links, for the phone. An older host (no `linked` class) answers nothing, and the picker hides. */
export function usePhoneMachines(online: boolean): { machines: Machine[]; reload: () => void } {
  const [machines, setMachines] = useState<Machine[]>([])
  const reload = useCallback(() => { void rpc('machine.list', {}).then((reply) => setMachines(normalizeMachines(reply))).catch(() => setMachines([])) }, [])
  useEffect(() => { if (online) reload() }, [online, reload])
  useEffect(() => {
    if (!online) return
    // machine.changed doesn't reach a phone, so the picker re-reads every minute and on each live change.
    const timer = window.setInterval(reload, 60_000)
    return () => window.clearInterval(timer)
  }, [online, reload])
  return { machines, reload }
}

/** "This computer" or one of the host's links. */
export function MachinePicker({ hostName, machines, current, onPick }: { hostName: string; machines: readonly Machine[]; current: string | null; onPick: (id: string | null) => void }) {
  if (!machines.length) return null
  return <nav className="rm-machines" aria-label="Machines">
    <button type="button" className={current === null ? 'on' : ''} aria-pressed={current === null} onClick={() => onPick(null)}><Monitor size={14} aria-hidden="true" />{hostName}</button>
    {machines.filter((machine) => machine.state !== 'revoked').map((machine) => <button key={machine.id} type="button" className={`${current === machine.id ? 'on' : ''}${machine.state === 'online' ? '' : ' off'}`} aria-pressed={current === machine.id} title={machine.state === 'online' ? machine.live ? 'Live' : 'Updates every 20 seconds' : `${machine.name} can't be reached`} onClick={() => onPick(machine.id)}>
      <KindIcon kind={machine.kind} />{machine.name}{machine.state === 'online' && machine.live && <i className="rm-live" aria-label="live" />}
    </button>)}
  </nav>
}

/** Another machine, through the host: its bots, then a bot's conversations, then one conversation. */
export function PhoneMachine({ machine, hostName, tier, online, quiet }: { machine: Machine; hostName: string; tier: RemoteTier; online: boolean; quiet: boolean }) {
  const [bot, setBot] = useState<MachineBot | null>(null)
  const [rows, setRows] = useState<MachineSessionRow[]>([])
  const [sessionId, setSessionId] = useState<string | null>(null)
  const [tick, setTick] = useState(0)
  const level = effectiveTier(tier, machine.tier)
  const reachable = machine.state === 'online' && online
  useEffect(() => { setBot(null); setSessionId(null) }, [machine.id])
  useEffect(() => {
    let live = true
    void rpc('machine.sessions', { machineId: machine.id }).then((reply) => { if (live) setRows(normalizeMachineSessions(reply)) }).catch(() => { if (live) setRows([]) })
    return () => { live = false }
  }, [machine.id, tick])
  // Live: the host relays content-free changes from the other machine; the phone re-reads.
  useEffect(() => {
    let stop: (() => void) | null = null
    let live = true
    void listenForDaemonEvents((event) => {
      if (event.type !== 'machine.session.changed') return
      const payload = (event.payload ?? {}) as { machineId?: unknown }
      if (payload.machineId === machine.id) setTick((value) => value + 1)
    }).then((unlisten) => { if (live) stop = unlisten; else unlisten() }).catch(() => undefined)
    return () => { live = false; stop?.() }
  }, [machine.id])

  if (bot && sessionId) return <PhoneMachineChat machine={machine} bot={bot} sessionId={sessionId} hostName={hostName} level={level} reachable={reachable} quiet={quiet} tick={tick} onBack={() => setSessionId(null)} />
  if (bot) {
    const conversations = rows.filter((row) => row.agentId === bot.id && !row.archived)
    const start = async () => {
      try { const reply = await rpc<{ sessionId?: string }>('machine.start', { machineId: machine.id, agentId: bot.id }); if (reply?.sessionId) setSessionId(reply.sessionId) }
      catch { setTick((value) => value + 1) }
    }
    return <section className="rm-machine" aria-label={`${bot.name} on ${machine.name}`}>
      <header className="rm-machine-head"><button type="button" className="rm-icon" aria-label="Back" onClick={() => setBot(null)}><ArrowLeft size={18} /></button><strong>{bot.name}</strong><small>on {machine.name}</small></header>
      {reachable && level !== 'viewer' && !quiet && <button type="button" className="rm-text-button rm-machine-new" onClick={() => void start()}><Plus size={14} aria-hidden="true" />New conversation</button>}
      {!conversations.length && <p className="rm-empty">No conversations yet.</p>}
      <ul className="rm-convos">{conversations.map((row) => <li key={row.id}><button type="button" className={reachable ? '' : 'off'} onClick={() => setSessionId(row.id)}><span><strong>{row.title?.trim() || 'Conversation'}</strong><small>{[row.projectName, row.state].filter(Boolean).join(' · ')}</small></span></button></li>)}</ul>
    </section>
  }
  return <section className="rm-machine" aria-label={machine.name}>
    <p className="rm-machine-note">{reachable ? `Through ${hostName}. ${level === 'viewer' ? 'View only here.' : level === 'chat' ? 'You can chat here.' : 'You can chat and approve here.'}` : `${machine.name} can't be reached. Its last known conversations are shown.`}</p>
    {!machine.bots.length && <p className="rm-empty">No employees on {machine.name} yet.</p>}
    <ul className="rm-bots">{machine.bots.map((item) => <li key={item.id}><button type="button" className="rm-bot" onClick={() => setBot(item)}>
      <BlobCanvas decorative color={agentColorHex(item.color ?? 'mint')} size={36} mood={!reachable ? 'offline' : item.busy ? 'tool_activity' : 'idle'} look={agentLook(item as never)} label={item.name} />
      <span><strong>{item.name}</strong><small>{!reachable ? 'Offline' : item.busy ? 'Working' : item.role || `${item.conversations} conversation${item.conversations === 1 ? '' : 's'}`}</small></span>
    </button></li>)}</ul>
  </section>
}

function PhoneMachineChat({ machine, bot, sessionId, hostName, level, reachable, quiet, tick, onBack }: { machine: Machine; bot: MachineBot; sessionId: string; hostName: string; level: RemoteTier; reachable: boolean; quiet: boolean; tick: number; onBack: () => void }) {
  const [session, setSession] = useState<Session | null>(null)
  const [approvals, setApprovals] = useState<PermissionRequest[]>([])
  const [draft, setDraft] = useState('')
  const [problem, setProblem] = useState<string | null>(null)
  const [sending, setSending] = useState(false)
  const logRef = useRef<HTMLDivElement>(null)
  const load = useCallback(() => {
    void rpc<{ session?: Session }>('machine.conversation', { machineId: machine.id, sessionId }).then((reply) => setSession(reply?.session ?? null)).catch((reason) => setProblem(messageOf(reason, `${machine.name} can't be reached right now.`)))
    void rpc<{ permissions?: PermissionRequest[] }>('machine.permissions', { machineId: machine.id, sessionId }).then((reply) => setApprovals(Array.isArray(reply?.permissions) ? reply.permissions.filter((card) => card.kind !== 'question' && card.kind !== 'proposal') : [])).catch(() => setApprovals([]))
  }, [machine.id, sessionId, machine.name])
  useEffect(() => { load() }, [load, tick])
  // Without a live link from the host, a slow poll keeps the conversation moving.
  useEffect(() => { if (machine.live) return; const timer = window.setInterval(load, 5000); return () => window.clearInterval(timer) }, [load, machine.live])
  const messages = (session?.messages ?? []).filter((message) => (message.role === 'user' || message.role === 'assistant') && String(message.content ?? '').trim())
  useEffect(() => { logRef.current?.scrollTo({ top: logRef.current.scrollHeight }) }, [messages.length])
  const busy = ['working', 'starting', 'waiting_permission'].includes(String(session?.state ?? ''))
  const canSend = reachable && level !== 'viewer' && !quiet
  const send = async () => {
    const text = draft.trim()
    if (!text || !canSend) return
    setSending(true); setProblem(null)
    try { await rpc(busy ? 'machine.steer' : 'machine.prompt', { machineId: machine.id, sessionId, text }); setDraft(''); load() }
    catch (reason) { setProblem(messageOf(reason, `${machine.name} didn't take that message.`)) } finally { setSending(false) }
  }
  const stop = async () => { try { await rpc('machine.cancel', { machineId: machine.id, sessionId }); load() } catch (reason) { setProblem(messageOf(reason, `${machine.name} didn't stop it.`)) } }
  const pending = approvals[0] ?? null
  return <section className="rm-chat rm-machine-chat">
    <header className="rm-machine-head"><button type="button" className="rm-icon" aria-label="Back" onClick={onBack}><ArrowLeft size={18} /></button><strong>{bot.name}</strong><small>{session?.title?.trim() || 'Conversation'} · {machine.name}</small></header>
    <div className="rm-log" ref={logRef} role="log" aria-live="polite" aria-label={`Conversation with ${bot.name} on ${machine.name}`}>
      {session === null && !problem && <p className="rm-empty">Loading…</p>}
      {session && messages.length === 0 && <p className="rm-empty">Nothing here yet. Say hello to {bot.name}.</p>}
      {messages.slice(-40).map((message) => <p key={String(message.id)} className={`rm-msg ${String(message.role)}`}>{String(message.content)}</p>)}
      {busy && <p className="rm-msg notice">{bot.name} is working on {machine.name}…</p>}
    </div>
    {pending && (level === 'operator'
      ? <div className="rm-approval"><ApprovalCard permission={pending} focusOnMount={false} onReply={async (choice) => { try { await rpc('machine.permission.reply', { machineId: machine.id, permissionId: pending.id, choice }); load(); return true } catch (reason) { setProblem(messageOf(reason, `${machine.name} didn't take that answer.`)); return false } }} /></div>
      : <p className="rm-approval locked" role="status">{bot.name} is asking permission on {machine.name}. Answering needs Chat and approve, here and on {hostName}'s link.</p>)}
    {problem && <p className="rm-send-problem" role="alert">{problem}</p>}
    <form className="rm-composer" onSubmit={(event) => { event.preventDefault(); void send() }}>
      <textarea rows={1} value={draft} disabled={!canSend} placeholder={!reachable ? `${machine.name} can't be reached` : level === 'viewer' ? 'View only on this machine' : quiet ? 'Quiet hours: reading only' : busy ? `Add to what ${bot.name} is doing` : `Message ${bot.name}`} aria-label={`Message ${bot.name} on ${machine.name}`} onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); void send() } }} />
      {busy && canSend && <button type="button" className="rm-send" aria-label="Stop" onClick={() => void stop()}><Square size={15} aria-hidden="true" /></button>}
      <button type="submit" className="rm-send" aria-label="Send" disabled={!canSend || !draft.trim() || sending}><Send size={17} aria-hidden="true" /></button>
    </form>
    <p className="rm-tier">Runs on {machine.name}, through {hostName}</p>
  </section>
}
