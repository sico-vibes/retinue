// 22 November 2026: "Connect coding agents". Retinue writes its own MCP entry (the bridge and the endpoint file,
// never the key) into each coding agent's user settings, so the owner no longer pastes a command. The daemon
// finds the agents, writes and removes only its own entry and keeps a backup; this list only asks it to.
import { useCallback, useEffect, useState } from 'react'
import { rpc } from '../tauri'
import { ProviderLogo } from './providerBrand'
import { SettingsGroup, SettingsRow } from './SettingsParts'

export type ExternalClientState = 'connected' | 'not_connected' | 'different'
export type ExternalClient = {
  id: string
  name: string
  installed: boolean
  state: ExternalClientState
  configPath: string | null
  addedByRetinue: boolean
  error: string | null
}

const STATES: readonly ExternalClientState[] = ['connected', 'not_connected', 'different']
/** The logo each client shows: Claude Desktop is a Claude app, so it wears Claude's mark. */
const LOGOS: Record<string, string> = { 'claude-code': 'claude', 'claude-desktop': 'claude' }
const text = (value: unknown) => typeof value === 'string' && value ? value : null

/** Accepts only well-formed rows from the daemon; anything else is dropped. */
export function normalizeExternalClients(raw: unknown): ExternalClient[] {
  const list = (raw as { clients?: unknown } | null)?.clients
  if (!Array.isArray(list)) return []
  return list.flatMap((item): ExternalClient[] => {
    if (!item || typeof item !== 'object') return []
    const row = item as Record<string, unknown>
    const id = text(row.id), name = text(row.name)
    if (!id || !name) return []
    const state = STATES.includes(row.state as ExternalClientState) ? row.state as ExternalClientState : 'not_connected'
    return [{ id, name, installed: row.installed === true, state, configPath: text(row.configPath), addedByRetinue: row.addedByRetinue === true, error: text(row.error) }]
  })
}

/** Plain names joined for a sentence: "Codex", "Codex and Cursor", "Codex, Cursor and Qwen Code". */
export function joinNames(names: readonly string[]) {
  return names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`
}

function detailOf(client: ExternalClient) {
  if (client.error) return client.error
  if (client.state === 'connected') return client.configPath ? `Connected · ${client.configPath}` : 'Connected'
  if (client.state === 'different') return 'It already has a different “retinue” entry. Remove that one in its settings and Retinue can add its own.'
  return 'Not connected'
}

const reason = (error: unknown, fallback: string) => error instanceof Error && error.message ? error.message : fallback

/**
 * The coding agents on this computer, each with Connect or Disconnect. `enabled` is the MCP switch: with it
 * off the list only offers to remove entries Retinue added earlier.
 */
export function ExternalClients({ enabled, onNotice, onProblem }: { enabled: boolean; onNotice: (text: string) => void; onProblem: (text: string | null) => void }) {
  const [clients, setClients] = useState<ExternalClient[] | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)
  const load = useCallback(async () => {
    try { setClients(normalizeExternalClients(await rpc<unknown>('external.clients.list', {}))); setFailed(false) } catch { setFailed(true) }
  }, [])
  useEffect(() => { void load() }, [load, enabled])

  const replace = (next: ExternalClient | undefined) => { if (next) setClients((list) => list?.map((item) => item.id === next.id ? next : item) ?? list) }
  const connect = async (targets: readonly ExternalClient[]) => {
    onProblem(null); setBusy(targets.length === 1 ? targets[0].id : 'all')
    const done: string[] = []
    try {
      for (const client of targets) {
        const result = await rpc<{ client?: unknown }>('external.clients.connect', { id: client.id, confirm: true })
        const next = normalizeExternalClients({ clients: [result?.client] })[0]
        replace(next)
        // The daemon answers a refused change with the row and its reason, not an RPC error.
        if (next?.state === 'connected' && !next.error) done.push(client.name)
        else onProblem(next?.error ?? `${client.name} could not be connected.`)
      }
    } catch (error) { onProblem(reason(error, 'That coding agent could not be connected.')) } finally { setBusy(null) }
    if (done.length) onNotice(`Connected to ${joinNames(done)}. Sessions you open from now on have Retinue's tools; restart any that are open.`)
    void load()
  }
  const disconnect = async (targets: readonly ExternalClient[]) => {
    onProblem(null); setBusy(targets.length === 1 ? targets[0].id : 'all')
    const done: string[] = []
    try {
      for (const client of targets) {
        const result = await rpc<{ client?: unknown }>('external.clients.disconnect', { id: client.id })
        const next = normalizeExternalClients({ clients: [result?.client] })[0]
        replace(next)
        if (next && next.state !== 'connected' && !next.error) done.push(client.name)
        else onProblem(next?.error ?? `${client.name} could not be disconnected.`)
      }
    } catch (error) { onProblem(reason(error, 'That coding agent could not be disconnected.')) } finally { setBusy(null) }
    if (done.length) onNotice(`Removed Retinue from ${joinNames(done)}.`)
    void load()
  }

  if (failed && !clients) return <SettingsGroup title="Coding agents"><SettingsRow label="Coding agents on this computer" detail="The coding agents could not be checked.">
    <button type="button" className="secondary-button small" onClick={() => void load()}>Try again</button>
  </SettingsRow></SettingsGroup>
  if (!clients) return null
  const installed = clients.filter((client) => client.installed)
  const missing = clients.filter((client) => !client.installed)
  const ours = installed.filter((client) => client.state === 'connected' && client.addedByRetinue)

  // Off: only offer to take back what Retinue added. Nothing to show when it added nothing.
  if (!enabled) {
    if (!ours.length) return null
    return <SettingsGroup title="Coding agents">
      <SettingsRow label="Still set up in other apps" hint="These entries do nothing while the Retinue MCP is off, and work again when you turn it back on." detail={joinNames(ours.map((client) => client.name))}>
        <button type="button" className="secondary-button small" disabled={!!busy} onClick={() => void disconnect(ours)}>{busy ? 'Removing…' : 'Remove'}</button>
      </SettingsRow>
    </SettingsGroup>
  }

  const open = installed.filter((client) => client.state === 'not_connected')
  const action = open.length > 1 ? <button type="button" className="ghost-button small" disabled={!!busy} onClick={() => void connect(open)}>{busy === 'all' ? 'Connecting…' : 'Connect all'}</button> : undefined
  return <SettingsGroup title="Coding agents" action={action}>
    {installed.length === 0 && <SettingsRow label="No coding agents found" detail="Install Claude Code, Codex or OpenCode and they appear here. You can still connect any MCP app with the command below." />}
    {installed.map((client) => {
      const working = busy === client.id || (busy === 'all' && open.includes(client))
      return <div key={client.id} className="settings-row mcp-client" data-state={client.state}>
        <span className="settings-row-copy">
          <span className="settings-row-title"><ProviderLogo provider={LOGOS[client.id] ?? client.id} size={18} /><strong>{client.name}</strong></span>
          <small className={client.error ? 'mcp-client-error' : undefined}>{detailOf(client)}</small>
        </span>
        <span className="settings-row-control">
          {client.state === 'not_connected' && <button type="button" className="secondary-button small" disabled={!!busy} aria-label={`Connect ${client.name}`} onClick={() => void connect([client])}>{working ? 'Connecting…' : 'Connect'}</button>}
          {client.state === 'connected' && client.addedByRetinue && <button type="button" className="ghost-button small" disabled={!!busy} aria-label={`Disconnect ${client.name}`} onClick={() => void disconnect([client])}>{working ? 'Removing…' : 'Disconnect'}</button>}
        </span>
      </div>
    })}
    {missing.length > 0 && <SettingsRow label="Not found on this computer" detail={joinNames(missing.map((client) => client.name))} />}
  </SettingsGroup>
}
