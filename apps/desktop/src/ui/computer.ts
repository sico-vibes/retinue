// A computer on the owner's own server (a Harbor "bot computer") that employees can use.
// The daemon holds the address and the token and makes every call; the window only ever sees
// this status, and never the token.
import { useCallback, useEffect, useState } from 'react'
import { listenForDaemonEvents, rpc } from '../tauri'

export type ComputerState = 'awake' | 'asleep' | 'missing' | 'unknown'

export interface ComputerStatus {
  linked: boolean
  /** Whether employees get the computer's tools in new conversations. */
  enabled: boolean
  address: string | null
  name: string | null
  reachable: boolean
  state: ComputerState
  since: string | null
  restarts: number
  cpuPercent: number | null
  memoryPercent: number | null
  memoryText: string | null
  pids: number | null
  diskBytes: number | null
  limits: { cpu: string | null; memory: string | null } | null
  desktopUrl: string | null
  checkedAt: string | null
  problem: string | null
}

export interface ComputerRoutine {
  id: string
  name: string
  cron: string
  command: string
  enabled: boolean
  lastRunAt: string | null
  lastOk: boolean | null
  lastOutput: string | null
}

const STATES: readonly ComputerState[] = ['awake', 'asleep', 'missing', 'unknown']
const text = (value: unknown) => typeof value === 'string' && value.trim() ? value.trim() : null
const number = (value: unknown) => typeof value === 'number' && Number.isFinite(value) ? value : null
const percent = (value: unknown) => { const n = number(value); return n === null ? null : Math.max(0, Math.min(100, n)) }

/** Only an http(s) address is ever offered as a link. */
function webAddress(value: unknown): string | null {
  const raw = text(value)
  if (!raw) return null
  try { const url = new URL(raw); return url.protocol === 'https:' || url.protocol === 'http:' ? raw : null } catch { return null }
}

/** Accepts only a well-formed status. An unlinked computer has no state worth showing. */
export function parseComputerStatus(raw: unknown): ComputerStatus | null {
  if (!raw || typeof raw !== 'object') return null
  const item = raw as Record<string, unknown>
  if (typeof item.linked !== 'boolean') return null
  const linked = item.linked
  const limits = item.limits && typeof item.limits === 'object' ? item.limits as Record<string, unknown> : null
  const address = webAddress(item.address)
  const desktop = webAddress(item.desktopUrl)
  // The desktop link is only kept when it is on the linked server itself.
  const sameOrigin = (() => { try { return !!address && !!desktop && new URL(address).origin === new URL(desktop).origin } catch { return false } })()
  return {
    linked,
    enabled: linked && item.enabled === true,
    address: linked ? address : null,
    name: linked ? text(item.name) : null,
    reachable: linked && item.reachable === true,
    state: linked && item.reachable === true && STATES.includes(item.state as ComputerState) ? item.state as ComputerState : 'unknown',
    since: text(item.since),
    restarts: number(item.restarts) ?? 0,
    cpuPercent: percent(item.cpuPercent),
    memoryPercent: percent(item.memoryPercent),
    memoryText: text(item.memoryText),
    pids: number(item.pids),
    diskBytes: number(item.diskBytes),
    limits: limits ? { cpu: text(limits.cpu), memory: text(limits.memory) } : null,
    desktopUrl: linked && sameOrigin ? desktop : null,
    checkedAt: text(item.checkedAt),
    problem: linked ? text(item.problem) : null,
  }
}

export function parseRoutines(raw: unknown): ComputerRoutine[] {
  const list = (raw as { routines?: unknown } | null)?.routines
  if (!Array.isArray(list)) return []
  return list.flatMap((entry): ComputerRoutine[] => {
    const row = entry as Record<string, unknown> | null
    const id = text(row?.id)
    const command = text(row?.command)
    if (!row || !id || !command) return []
    return [{ id, name: text(row.name) ?? 'Routine', cron: text(row.cron) ?? '', command, enabled: row.enabled !== false, lastRunAt: text(row.lastRunAt), lastOk: typeof row.lastOk === 'boolean' ? row.lastOk : null, lastOutput: text(row.lastOutput) }]
  })
}

/** The one word the state is shown as. Colour goes on this word, never on a box. */
export function stateWord(status: Pick<ComputerStatus, 'reachable' | 'state'>): string {
  if (!status.reachable) return 'Not reachable'
  if (status.state === 'awake') return 'Awake'
  if (status.state === 'asleep') return 'Asleep'
  if (status.state === 'missing') return 'Not set up'
  return 'Unknown'
}

export function sizeText(bytes: number | null): string | null {
  if (bytes === null || bytes < 0) return null
  if (bytes < 1024 ** 2) return `${Math.max(1, Math.round(bytes / 1024))} KB`
  if (bytes < 1024 ** 3) return `${Math.round(bytes / 1024 ** 2)} MB`
  return `${(bytes / 1024 ** 3).toFixed(1)} GB`
}

/** "3 min", "5 h", "2 days": how long since a moment, without "ago". */
export function spanSince(at: string | null, now = Date.now()): string | null {
  if (!at) return null
  const time = new Date(at).getTime()
  if (Number.isNaN(time) || time > now) return null
  const minutes = Math.floor((now - time) / 60_000)
  if (minutes < 1) return 'under a minute'
  if (minutes < 60) return `${minutes} min`
  if (minutes < 48 * 60) return `${Math.floor(minutes / 60)} h`
  return `${Math.floor(minutes / (24 * 60))} days`
}

const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']

/** Reads the common five-field schedules in words; anything else is shown as written. */
export function scheduleText(cron: string): string {
  const parts = cron.trim().split(/\s+/)
  if (parts.length !== 5) return cron.trim() || 'No schedule'
  const [minute, hour, day, month, weekday] = parts as [string, string, string, string, string]
  const plain = (value: string) => /^\d+$/.test(value)
  if (day !== '*' || month !== '*') return cron.trim()
  const every = /^\*\/(\d+)$/
  if (hour === '*' && weekday === '*') {
    if (minute === '*') return 'Every minute'
    const step = every.exec(minute)
    if (step) return `Every ${step[1]} minutes`
    if (plain(minute)) return `Every hour at :${minute.padStart(2, '0')}`
    return cron.trim()
  }
  const hours = every.exec(hour)
  if (hours && plain(minute) && weekday === '*') return `Every ${hours[1]} hours`
  if (!plain(minute) || !plain(hour)) return cron.trim()
  const at = `${hour.padStart(2, '0')}:${minute.padStart(2, '0')}`
  if (weekday === '*') return `Every day at ${at}`
  if (weekday === '1-5') return `Weekdays at ${at}`
  if (plain(weekday)) return `Every ${DAYS[Number(weekday) % 7]} at ${at}`
  return cron.trim()
}

/** What the address box will accept, before the daemon checks it properly. */
export function addressProblem(value: string): string | null {
  const raw = value.trim()
  if (!raw) return null
  let url: URL
  try { url = new URL(/^[a-z]+:\/\//i.test(raw) ? raw : `https://${raw}`) } catch { return 'That is not a web address.' }
  if (url.username || url.password) return 'Leave the sign-in out of the address.'
  if (url.protocol !== 'https:' && url.protocol !== 'http:') return 'Use an https address.'
  return null
}

/** "dashboard.example.com" becomes "https://dashboard.example.com". */
export function tidyAddress(value: string): string {
  const raw = value.trim().replace(/\/+$/, '')
  return /^[a-z]+:\/\//i.test(raw) ? raw : `https://${raw}`
}

const REFRESH_MS = 30_000

/** The linked computer, kept current by the daemon's event and a slow re-read while `active`. A client the daemon refuses (a phone) simply has none. */
export function useComputer(active: boolean): { status: ComputerStatus | null; failed: boolean; refresh: () => void; apply: (raw: unknown) => ComputerStatus | null } {
  const [status, setStatus] = useState<ComputerStatus | null>(null)
  const [failed, setFailed] = useState(false)
  const [turn, setTurn] = useState(0)
  const apply = useCallback((raw: unknown) => {
    const next = parseComputerStatus((raw as { status?: unknown } | null)?.status ?? raw)
    if (next) { setStatus(next); setFailed(false) }
    return next
  }, [])
  useEffect(() => {
    if (!active) return
    let live = true
    let off: (() => void) | null = null
    const read = () => { void rpc<unknown>('computer.status', {}).then((reply) => { if (live) apply(reply) }).catch(() => { if (live) setFailed(true) }) }
    read()
    void listenForDaemonEvents((event) => { if (live && event.type === 'computer.changed') apply(event.payload) }).then((unlisten) => { if (live) off = unlisten; else unlisten() })
    const timer = window.setInterval(read, REFRESH_MS)
    return () => { live = false; off?.(); window.clearInterval(timer) }
  }, [active, apply, turn])
  const refresh = useCallback(() => setTurn((value) => value + 1), [])
  return { status, failed, refresh, apply }
}
