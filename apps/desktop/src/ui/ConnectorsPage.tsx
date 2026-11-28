// Settings > Integrations > Connectors (owner, 16 November 2026): MCP servers the daemon calls for the employees,
// so a bot can use Notion, Linear or GitHub without ever holding their tokens. Contract: docs/plans/nod-adoption-plan.md, D.
import { platform as hostPlatform } from '../transport/platform'
import { secretStoreName } from './platformCopy'
import { useCallback, useEffect, useState } from 'react'
import { ChevronDown, ChevronRight, Plug, Plus, RefreshCw, Trash2 } from 'lucide-react'
import { listenForDaemonEvents, rpc } from '../tauri'
import { ConfirmDialog } from './BlobPage'
import { SettingsGroup } from './SettingsParts'
import { SegmentedControl } from './SegmentedControl'
import { openSafeLink } from './openSafeLink'
import './accounts.css'
import './connectors.css'

export type Connector = { id: string; name: string; url: string; auth: 'none' | 'token' | 'oauth'; enabled: boolean; status: 'ok' | 'error' | 'unchecked'; detail: string | null; toolCount: number | null; unattended: 'off' | 'readOnly'; lastCheckedAt: string | null }
export type ConnectorTool = { name: string; title: string | null; description: string; readOnly: boolean | null; rule: 'ask' | 'allow' | 'deny' }
export type CatalogueEntry = { id: string; name: string; url: string; docsUrl: string; auth: 'token' | 'oauth'; tokenHint: string }
/** A browser sign-in's state (wave 3, `connector.oauth.status`). Tokens never reach the window. */
export type ConnectorSignIn = { id: string; auth: 'none' | 'token' | 'oauth'; status: 'idle' | 'pending' | 'authorized' | 'error'; detail: string | null; expiresAt: string | null }

/** The sign-in line for an oauth connector. */
export function signInLine(signIn: ConnectorSignIn | null): string {
  if (!signIn) return 'Checking the sign-in…'
  if (signIn.status === 'authorized') return 'Signed in'
  if (signIn.status === 'pending') return 'Waiting for you to finish signing in, in your browser'
  if (signIn.status === 'error') return signIn.detail ?? 'The sign-in didn’t finish'
  return 'Not signed in yet'
}

const messageOf = (reason: unknown, fallback: string) => reason instanceof Error && reason.message ? reason.message : typeof reason === 'string' && reason ? reason : fallback

/** Why this address can't be used, or null. The daemon checks again, including DNS. */
export function connectorUrlProblem(url: string): string | null {
  try {
    const parsed = new URL(url)
    if (parsed.protocol === 'https:') return null
    if (parsed.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(parsed.hostname)) return null
    return 'Use an https address. Plain http is only allowed for a server on this computer.'
  } catch { return 'Paste the server’s full address, starting with https://.' }
}

export function connectorStatusLine(connector: Connector): string {
  if (!connector.enabled) return 'Turned off'
  if (connector.status === 'error') return connector.detail ?? 'It couldn’t be reached'
  if (connector.status === 'unchecked') return 'Not checked yet'
  return connector.toolCount === null ? 'Connected' : `Connected · ${connector.toolCount} tool${connector.toolCount === 1 ? '' : 's'}`
}

function AddForm({ preset, busy, onAdd, onCancel }: { preset: CatalogueEntry | null; busy: boolean; onAdd: (body: { name: string; url: string; token: string; oauth: boolean }) => void; onCancel: () => void }) {
  const [name, setName] = useState(preset?.name ?? '')
  const [url, setUrl] = useState(preset?.url ?? '')
  const [token, setToken] = useState('')
  const [how, setHow] = useState<'browser' | 'token'>(preset?.auth === 'oauth' ? 'browser' : 'token')
  const urlProblem = url ? connectorUrlProblem(url.trim()) : null
  return <form className="connector-form" autoComplete="off" onSubmit={(event) => { event.preventDefault(); if (name.trim() && url.trim() && !urlProblem) onAdd({ name: name.trim(), url: url.trim(), token: how === 'token' ? token.trim() : '', oauth: how === 'browser' }) }}>
    <div className="connector-form-grid">
      <label className="new-project-field"><span>Name</span><input className="text-input" value={name} maxLength={40} placeholder="Linear" autoFocus={!preset} onChange={(event) => setName(event.target.value)} /></label>
      <label className="new-project-field"><span>Server address</span><input className="text-input" value={url} spellCheck={false} placeholder="https://mcp.example.com/mcp" inputMode="url" aria-invalid={!!urlProblem} onChange={(event) => setUrl(event.target.value)} /></label>
    </div>
    <div className="new-project-field"><span>How Retinue signs in</span>
      <SegmentedControl label="How Retinue signs in" value={how} options={[{ value: 'browser', label: 'Sign in with your browser' }, { value: 'token', label: 'Paste a token' }]} onChange={setHow} />
    </div>
    {how === 'token'
      ? <label className="new-project-field"><span>Token (if the server needs one)</span><input className="text-input" type="password" autoComplete="new-password" spellCheck={false} value={token} autoFocus={!!preset} onChange={(event) => setToken(event.target.value)} />
        <small className="project-settings-note">{preset ? <>{preset.tokenHint} <button type="button" className="markdown-link" onClick={() => void openSafeLink(preset.docsUrl)}>Where to get it</button></> : `Kept in ${secretStoreName(hostPlatform().os)}. Employees never see it.`}</small>
      </label>
      : <small className="project-settings-note">Your browser opens the service's own sign-in page. Retinue keeps what it gets in {secretStoreName(hostPlatform().os)}, renews it, and signs out at the service when you remove the connector. Employees never see it.</small>}
    {urlProblem && <p className="new-project-error" role="alert">{urlProblem}</p>}
    <div className="connector-actions">
      <button type="button" className="secondary-button small" disabled={busy} onClick={onCancel}>Cancel</button>
      <button type="submit" className="primary-button small" disabled={busy || !name.trim() || !url.trim() || !!urlProblem}>Connect</button>
    </div>
  </form>
}

function ToolList({ connector }: { connector: Connector }) {
  const [tools, setTools] = useState<ConnectorTool[] | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  useEffect(() => { void rpc<{ tools?: ConnectorTool[] }>('connector.tools', { id: connector.id }).then((reply) => setTools(reply?.tools ?? [])).catch((reason) => { setTools([]); setProblem(messageOf(reason, 'The tools could not be listed.')) }) }, [connector.id])
  const setRule = (tool: ConnectorTool, rule: ConnectorTool['rule']) => {
    setTools((list) => (list ?? []).map((item) => item.name === tool.name ? { ...item, rule } : item))
    void rpc('connector.tool.rule', { id: connector.id, tool: tool.name, rule }).catch((reason) => { setProblem(messageOf(reason, 'That rule could not be saved.')); setTools((list) => (list ?? []).map((item) => item.name === tool.name ? tool : item)) })
  }
  if (tools === null) return <p className="connector-tools-note">Loading tools…</p>
  return <div className="connector-tools">
    {problem && <p className="new-project-error" role="alert">{problem}</p>}
    {!tools.length && !problem && <p className="connector-tools-note">This server offers no tools.</p>}
    {tools.map((tool) => <div key={tool.name} className="connector-tool">
      <span className="account-row-copy"><strong>{tool.title ?? tool.name}{tool.readOnly === true && <em className="connector-tag">Read only</em>}</strong><small title={tool.description}>{tool.description || tool.name}</small></span>
      <SegmentedControl label={`When an employee uses ${tool.title ?? tool.name}`} value={tool.rule} options={[{ value: 'ask', label: 'Ask' }, { value: 'allow', label: 'Allow' }, { value: 'deny', label: 'Never' }]} onChange={(rule) => setRule(tool, rule)} />
    </div>)}
  </div>
}

/** A browser sign-in for one connector: its state, and the one action that fits it. */
function SignInLine({ connector, busy, run, signIn }: { connector: Connector; busy: boolean; run: (method: string, params: Record<string, unknown>) => void; signIn: ConnectorSignIn | null }) {
  const [pageUrl, setPageUrl] = useState<string | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const start = async () => {
    setProblem(null)
    try { const reply = await rpc<{ authorizationUrl?: string }>('connector.oauth.start', { id: connector.id }); setPageUrl(reply?.authorizationUrl ?? null) }
    catch (reason) { setProblem(messageOf(reason, 'The sign-in could not start.')) }
  }
  const status = signIn?.status ?? null
  return <div className="connector-signin" data-status={status ?? 'loading'}>
    <span className="account-row-copy"><strong>{signInLine(signIn)}</strong>{status === 'pending' && signIn?.expiresAt && <small>It stops waiting at {new Date(signIn.expiresAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}.</small>}</span>
    {(status === 'idle' || status === 'error') && <button type="button" className="primary-button small" disabled={busy} onClick={() => void start()}>Sign in</button>}
    {status === 'pending' && <>
      {pageUrl && <button type="button" className="markdown-link" onClick={() => void openSafeLink(pageUrl)}>Open the sign-in page again</button>}
      <button type="button" className="secondary-button small" disabled={busy} onClick={() => run('connector.oauth.cancel', { id: connector.id })}>Cancel</button>
    </>}
    {status === 'authorized' && <button type="button" className="secondary-button small" disabled={busy} title="Signs out at the service and forgets the sign-in" onClick={() => run('connector.update', { id: connector.id, token: '' })}>Disconnect</button>}
    {problem && <p className="new-project-error" role="alert">{problem}</p>}
  </div>
}

function ConnectorRow({ connector, busy, run, signIn }: { connector: Connector; busy: boolean; run: (method: string, params: Record<string, unknown>) => void; signIn: ConnectorSignIn | null }) {
  const [open, setOpen] = useState(false)
  const [removing, setRemoving] = useState(false)
  return <div className={`connector-row${connector.enabled ? '' : ' is-off'}`}>
    <div className="connector-row-main">
      <button type="button" className="icon-button" aria-expanded={open} aria-label={`${open ? 'Hide' : 'Show'} ${connector.name}'s tools`} onClick={() => setOpen((value) => !value)}>{open ? <ChevronDown size={14} aria-hidden="true" /> : <ChevronRight size={14} aria-hidden="true" />}</button>
      <span className={`connector-dot status-${connector.enabled ? connector.status : 'off'}`} aria-hidden="true" />
      <span className="account-row-copy"><strong>{connector.name}</strong><small title={connector.url}>{connectorStatusLine(connector)} · {connector.url.replace(/^https?:\/\//, '')}</small></span>
      <button type="button" className="icon-button" aria-label={`Check ${connector.name} again`} title="Check again" disabled={busy} onClick={() => run('connector.test', { id: connector.id })}><RefreshCw size={14} aria-hidden="true" /></button>
      <button type="button" role="switch" aria-checked={connector.enabled} aria-label={`Use ${connector.name}`} className={`toggle ${connector.enabled ? 'on' : ''}`} disabled={busy} onClick={() => run('connector.update', { id: connector.id, enabled: !connector.enabled })}><i /></button>
      <button type="button" className="icon-button" aria-label={`Remove ${connector.name}`} title="Remove" disabled={busy} onClick={() => setRemoving(true)}><Trash2 size={14} aria-hidden="true" /></button>
    </div>
    {connector.auth === 'oauth' && <SignInLine connector={connector} busy={busy} run={run} signIn={signIn} />}
    {open && <div className="connector-detail">
      <div className="connector-unattended"><span className="account-row-copy"><strong>Scheduled and unattended work</strong><small>Automations can use only tools the server marks as read only, and only if you allow it here.</small></span>
        <SegmentedControl label={`Unattended use of ${connector.name}`} value={connector.unattended} options={[{ value: 'off', label: 'Off' }, { value: 'readOnly', label: 'Read only' }]} onChange={(unattended) => run('connector.update', { id: connector.id, unattended })} />
      </div>
      <ToolList connector={connector} />
    </div>}
    {removing && <ConfirmDialog title={`Remove ${connector.name}?`} body={connector.auth === 'oauth' ? "Employees can't use its tools any more. Retinue signs out at the service and forgets the sign-in." : "Employees can't use its tools any more, and Retinue forgets its token."} confirmLabel="Remove" cancelLabel="Keep it" tone="warning"
      onConfirm={() => { setRemoving(false); run('connector.remove', { id: connector.id, confirm: true }) }} onCancel={() => setRemoving(false)} />}
  </div>
}

export function ConnectorsPage() {
  const [state, setState] = useState<{ connectors: Connector[]; catalogue: CatalogueEntry[] } | null>(null)
  const [adding, setAdding] = useState<{ preset: CatalogueEntry | null } | null>(null)
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const [signIns, setSignIns] = useState<Record<string, ConnectorSignIn>>({})
  const load = useCallback(() => { void rpc<{ connectors?: Connector[]; catalogue?: CatalogueEntry[] }>('connector.list', {}).then((reply) => {
    const connectors = reply?.connectors ?? []
    setState({ connectors, catalogue: reply?.catalogue ?? [] })
    // A browser sign-in's state isn't on the row; it is read once per oauth connector, then kept current by events.
    for (const connector of connectors.filter((item) => item.auth === 'oauth')) {
      void rpc<ConnectorSignIn>('connector.oauth.status', { id: connector.id }).then((signIn) => { if (signIn?.id) setSignIns((current) => ({ ...current, [signIn.id]: signIn })) }).catch(() => undefined)
    }
  }).catch((reason) => { setState({ connectors: [], catalogue: [] }); setProblem(messageOf(reason, 'Connectors could not be read.')) }) }, [])
  useEffect(load, [load])
  useEffect(() => {
    let stop: (() => void) | null = null
    let live = true
    void listenForDaemonEvents((event) => {
      if (event.type !== 'connector.oauth.changed') return
      const signIn = event.payload as unknown as ConnectorSignIn | undefined
      if (signIn?.id) setSignIns((current) => ({ ...current, [signIn.id]: signIn }))
      if (signIn?.status === 'authorized' || signIn?.status === 'idle') load()
    }).then((unlisten) => { if (live) stop = unlisten; else unlisten() }).catch(() => undefined)
    return () => { live = false; stop?.() }
  }, [load])
  const run = (method: string, params: Record<string, unknown>, after?: () => void) => {
    setBusy(true); setProblem(null)
    void rpc(method, params).then(() => { after?.(); load() }).catch((reason) => setProblem(messageOf(reason, 'That could not be done.'))).finally(() => setBusy(false))
  }
  const added = new Set(state?.connectors.map((connector) => connector.url) ?? [])
  const suggestions = state?.catalogue.filter((entry) => !added.has(entry.url)) ?? []
  return <>
    <p className="settings-intro">Connect apps your employees can use, like Linear, Notion or GitHub. Retinue calls them for the employee and keeps the tokens, and you choose which tools need your approval.</p>
    {problem && <p className="new-project-error" role="alert">{problem}</p>}
    <SettingsGroup title="Connected" action={!adding && <button type="button" className="secondary-button small connector-add" onClick={() => setAdding({ preset: null })}><Plus size={13} aria-hidden="true" />Add a server</button>}>
      {state === null && <p className="settings-empty">Loading…</p>}
      {state !== null && !state.connectors.length && !adding && <p className="settings-empty">Nothing connected yet. Pick an app below or add any MCP server.</p>}
      {state?.connectors.map((connector) => <ConnectorRow key={connector.id} connector={connector} busy={busy} run={run} signIn={signIns[connector.id] ?? null} />)}
      {adding && <AddForm key={adding.preset?.id ?? 'custom'} preset={adding.preset} busy={busy} onCancel={() => setAdding(null)} onAdd={(body) => {
        if (!body.oauth) { run('connector.add', { name: body.name, url: body.url, ...(body.token ? { token: body.token } : {}) }, () => setAdding(null)); return }
        // Add it, then open the browser sign-in straight away.
        setBusy(true); setProblem(null)
        void rpc<{ connector?: Connector }>('connector.add', { name: body.name, url: body.url, auth: 'oauth' })
          .then((reply) => { setAdding(null); load(); return reply?.connector?.id ? rpc('connector.oauth.start', { id: reply.connector.id }) : null })
          .catch((reason) => setProblem(messageOf(reason, 'That connector could not be added.')))
          .finally(() => setBusy(false))
      }} />}
    </SettingsGroup>
    {suggestions.length > 0 && <SettingsGroup title="Apps with an official server">
      <div className="connector-catalogue">
        {suggestions.map((entry) => <button key={entry.id} type="button" className="connector-suggestion" onClick={() => setAdding({ preset: entry })}><Plug size={14} aria-hidden="true" /><span>{entry.name}</span></button>)}
      </div>
    </SettingsGroup>}
  </>
}
