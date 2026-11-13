// The dev servers a conversation's coding agent started (`preview.servers`). The daemon
// attributes a listening port to a conversation by process, never by guessing, and only
// looks while someone is asking: so this asks again while the rail is showing.
import { useEffect, useState } from 'react'
import { rpc } from '../tauri'
import type { Session } from '../types'
import { isDraftSession } from './teamSelectors'

export interface LocalServer {
  port: number
  url: string
  /** The listener's short process name ("node.exe"), when the daemon could read it. */
  process: string | null
  /** Whether one request for the page came back as a page. */
  answeredHtml: boolean
}

/** Loopback only: anything else the daemon might report is dropped, so the rail can never offer a link off this computer. */
export function parseLocalServers(reply: unknown): LocalServer[] {
  const list = (reply as { servers?: unknown } | null)?.servers
  if (!Array.isArray(list)) return []
  return list.flatMap((item): LocalServer[] => {
    const row = item as Record<string, unknown>
    const port = typeof row.port === 'number' && Number.isInteger(row.port) && row.port > 0 && row.port < 65536 ? row.port : null
    const url = typeof row.url === 'string' ? row.url : ''
    let host = ''
    try { host = new URL(url).hostname } catch { return [] }
    if (!port || !['127.0.0.1', 'localhost', '[::1]', '::1'].includes(host)) return []
    return [{ port, url, process: typeof row.process === 'string' && row.process.trim() ? row.process.trim() : null, answeredHtml: row.answeredHtml === true }]
  })
}

const ASK_EVERY_MS = 15_000

/** The servers for the open conversation, re-read every 15 seconds while `active`. A client the daemon refuses (a phone) simply gets none. */
export function useLocalServers(session: Session | null | undefined, active: boolean): LocalServer[] {
  const [servers, setServers] = useState<LocalServer[]>([])
  const id = session && !isDraftSession(session) ? session.id : null
  useEffect(() => {
    setServers([])
    if (!id || !active) return
    let live = true
    const ask = () => { void rpc<unknown>('preview.servers', { sessionId: id }).then((reply) => { if (live) setServers(parseLocalServers(reply)) }).catch(() => { if (live) setServers([]) }) }
    ask()
    const timer = window.setInterval(ask, ASK_EVERY_MS)
    return () => { live = false; window.clearInterval(timer) }
  }, [id, active])
  return servers
}
