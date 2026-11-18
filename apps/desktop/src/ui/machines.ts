// Phase 4: other machines, as the daemon reports them. The window never talks to another
// machine: the daemon holds the link and its token, and this only reads what it's told.
import { useEffect, useRef, useState } from 'react'
import type { Agent, Project } from '../types'
import { listenForDaemonEvents, rpc } from '../tauri'

export type MachineState = 'online' | 'offline' | 'revoked'
export type MachineTier = 'viewer' | 'chat' | 'operator'

export type MachineBot = {
  id: string
  name: string
  color?: string | null
  outfit?: Agent['outfit']
  look?: Agent['look']
  role?: string | null
  provider?: string | null
  model?: string | null
  busy: boolean
  activeSessionId: string | null
  conversations: number
}

export type Machine = {
  id: string
  name: string
  url: string
  auth: 'code' | 'password'
  fingerprint: string | null
  tier: MachineTier
  pairedAt: string | null
  lastSeenAt: string | null
  state: MachineState
  detail: string | null
  version: string | null
  bots: MachineBot[]
  /** The far machine's OS family ("windows", "linux", "macos"), or null before the first read or on an older build. */
  kind: string | null
  /** A live event socket to that machine is open right now (17 November 2026); otherwise its rows refresh every 20 s. */
  live: boolean
}

/** One of another machine's conversations, as `machine.sessions` holds it in memory (kept while the machine is offline). */
export type MachineSessionRow = {
  machineId: string
  machineName: string
  kind: string | null
  id: string
  title: string | null
  agentId: string | null
  agentName: string | null
  projectName: string | null
  state: string | null
  updatedAt: string | null
  archived: boolean
}

export type MachineProbe = { origin: string; reachable: boolean; isRetinue: boolean; auth: 'code' | 'password'; fingerprint: string | null; trusted: boolean }
export type PairJobState = 'pending' | 'approved' | 'rejected' | 'expired' | 'failed' | 'cancelled'
export type PairJob = { id: string; url: string; state: PairJobState; detail: string | null; machine: Machine | null }

export type TakeOverStep = { id: string; status: 'done' | 'failed' | 'skipped'; detail: string | null }
export type TakeOverJob = { id: string; state: 'running' | 'done' | 'failed'; steps: TakeOverStep[]; sessionId: string | null; detail: string | null }
export type TakeOverPreview = {
  ready: boolean
  reason: string | null
  title: string
  bot: { id: string; name: string; provider: string | null; model: string | null; agentId: string | null; willCreate: boolean }
  repo: { identity: string | null; url: string | null; branch: string | null; isWorktree: boolean; hasRemote: boolean }
  uncommitted: number
  commitsAhead: number
  project: Pick<Project, 'id' | 'name'> | null
  needsProject: boolean
  runtimeReady: boolean
}

const text = (value: unknown) => typeof value === 'string' && value ? value : null

function normalizeBot(raw: unknown): MachineBot | null {
  const item = raw && typeof raw === 'object' ? raw as Record<string, unknown> : null
  const id = text(item?.id)
  if (!item || !id) return null
  return {
    id,
    name: text(item.name) ?? 'Employee',
    color: text(item.color),
    outfit: item.outfit as Agent['outfit'],
    look: item.look as Agent['look'],
    role: text(item.role),
    provider: text(item.provider),
    model: text(item.model),
    busy: item.busy === true,
    activeSessionId: text(item.activeSessionId),
    conversations: typeof item.conversations === 'number' ? item.conversations : 0,
  }
}

/** Whatever the daemon sent, as complete rows. A row without an id is left out. */
export function normalizeMachines(raw: unknown): Machine[] {
  const rows = raw && typeof raw === 'object' ? (raw as { machines?: unknown }).machines : null
  if (!Array.isArray(rows)) return []
  return rows.flatMap((row): Machine[] => {
    const item = row && typeof row === 'object' ? row as Record<string, unknown> : null
    const id = text(item?.id)
    if (!item || !id) return []
    return [{
      id,
      name: text(item.name) ?? 'Another machine',
      url: text(item.url) ?? '',
      auth: item.auth === 'password' ? 'password' : 'code',
      fingerprint: text(item.fingerprint),
      tier: item.tier === 'viewer' || item.tier === 'operator' ? item.tier : 'chat',
      pairedAt: text(item.pairedAt),
      lastSeenAt: text(item.lastSeenAt),
      state: item.state === 'online' || item.state === 'revoked' ? item.state : 'offline',
      detail: text(item.detail),
      version: text(item.version),
      bots: Array.isArray(item.bots) ? item.bots.flatMap((bot) => normalizeBot(bot) ?? []) : [],
      kind: text(item.kind),
      live: item.live === true,
    }]
  })
}

/** `machine.sessions` rows, complete or left out. */
export function normalizeMachineSessions(raw: unknown): MachineSessionRow[] {
  const rows = raw && typeof raw === 'object' ? (raw as { sessions?: unknown }).sessions : null
  if (!Array.isArray(rows)) return []
  return rows.flatMap((row): MachineSessionRow[] => {
    const item = row && typeof row === 'object' ? row as Record<string, unknown> : null
    const id = text(item?.id)
    const machineId = text(item?.machineId)
    if (!item || !id || !machineId) return []
    return [{ machineId, machineName: text(item.machineName) ?? 'Another machine', kind: text(item.kind), id, title: text(item.title), agentId: text(item.agentId), agentName: text(item.agentName), projectName: text(item.projectName), state: text(item.state), updatedAt: text(item.updatedAt), archived: item.archived === true }]
  })
}

/** How fresh another machine's rows are, in words. */
export function machineFreshness(machine: Pick<Machine, 'state' | 'live'>): string {
  if (machine.state === 'revoked') return 'Needs pairing again'
  if (machine.state !== 'online') return 'Offline'
  return machine.live ? 'Live' : 'Every 20 s'
}

/** A word for the far machine's OS, for an icon's label. */
export function machineKindWord(kind: string | null): string | null {
  if (kind === 'windows') return 'Windows'
  if (kind === 'linux') return 'Linux'
  if (kind === 'macos') return 'macOS'
  return kind ? kind : null
}

/**
 * Calls `onChange` for each live change on another machine (`machine.session.changed`, content-free:
 * ids and a revision only). A revision at or below one already seen for that conversation is ignored.
 */
export function useMachineSessionChanges(active: boolean, onChange: (change: { machineId: string; sessionId: string; revision: number }) => void): void {
  const handler = useRef(onChange)
  handler.current = onChange
  useEffect(() => {
    if (!active) return
    const seen = new Map<string, number>()
    let unlisten: (() => void) | null = null
    let live = true
    void listenForDaemonEvents((event) => {
      if (event.type !== 'machine.session.changed') return
      const payload = (event.payload ?? {}) as { machineId?: unknown; sessionId?: unknown; revision?: unknown }
      if (typeof payload.machineId !== 'string' || typeof payload.sessionId !== 'string') return
      const revision = typeof payload.revision === 'number' ? payload.revision : 0
      const key = `${payload.machineId}/${payload.sessionId}`
      if (revision && (seen.get(key) ?? 0) >= revision) return
      if (revision) seen.set(key, revision)
      handler.current({ machineId: payload.machineId, sessionId: payload.sessionId, revision })
    }).then((stop) => { if (live) unlisten = stop; else stop() }).catch(() => undefined)
    return () => { live = false; unlisten?.() }
  }, [active])
}

/** Why this address can't be used, or null. The daemon checks again. */
export function machineUrlProblem(raw: string): string | null {
  const url = raw.trim()
  if (!url) return 'Paste the other machine\'s address.'
  if (!/^https:\/\//i.test(url)) return 'The address must start with https://.'
  try {
    const parsed = new URL(url)
    if (parsed.username || parsed.password) return 'Put no name or password in the address.'
    if (parsed.search || parsed.hash) return 'Use the address on its own.'
  } catch { return 'That doesn\'t look like an address.' }
  return null
}

/** Whether this link may press Take over. Reading needs less. */
export const canTakeOver = (machine: Machine) => machine.state === 'online' && machine.tier === 'operator'

/** Whether this link may start conversations there and send them messages (the other machine's Chat level). */
export const canChat = (machine: Machine) => machine.state === 'online' && machine.tier !== 'viewer'

export const machineStateWord = (machine: Machine) => machine.state === 'online' ? 'Online' : machine.state === 'revoked' ? 'Needs pairing again' : 'Can\'t be reached'

/** The bot of another machine that's working, with the machine's name, for a badge. */
export function machineBusyBots(machines: readonly Machine[]): Array<{ machine: Machine; bot: MachineBot }> {
  return machines.flatMap((machine) => machine.state === 'revoked' ? [] : machine.bots.filter((bot) => bot.busy).map((bot) => ({ machine, bot })))
}

/** The other machines, kept current. The daemon's own loop refreshes them. */
export function useMachines(active = true): { machines: Machine[]; selfId: string | null; loaded: boolean; reload: () => Promise<void> } {
  const [machines, setMachines] = useState<Machine[]>([])
  const [selfId, setSelfId] = useState<string | null>(null)
  const [loaded, setLoaded] = useState(false)
  const reload = async () => {
    const reply = await rpc('machine.list', {}).catch(() => null)
    if (reply) setMachines(normalizeMachines(reply))
    setLoaded(true)
  }
  useEffect(() => {
    if (!active) return
    void reload()
    void rpc<{ id?: string }>('machine.self', {}).then((reply) => setSelfId(reply?.id ?? null)).catch(() => undefined)
    let unlisten: (() => void) | null = null
    let live = true
    void listenForDaemonEvents((event) => { if (event.type === 'machine.changed') void reload() }).then((stop) => { if (live) unlisten = stop; else stop() }).catch(() => undefined)
    return () => { live = false; unlisten?.() }
  }, [active])
  return { machines, selfId, loaded, reload }
}
