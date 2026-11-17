// Settings > Coding agents > Pi (owner, 16 November 2026): Pi as an optional way to reach many model providers.
// Retinue keeps each provider's API key in Windows Credential Manager and hands it only to the Pi process it starts.
// Contract: docs/plans/vault-channels-pi.md, section C.
import { useCallback, useEffect, useState } from 'react'
import { Check, ExternalLink, KeyRound, Trash2 } from 'lucide-react'
import { rpc } from '../tauri'
import { ConfirmDialog } from './BlobPage'
import { SettingsGroup } from './SettingsParts'
import { openSafeLink } from './openSafeLink'
import './accounts.css'
import './pi.css'

export type PiProvider = { id: string; name: string; keyEnv: string; hasKey: boolean; models: number | null; docsUrl: string | null }
export type PiState = { installed: boolean; version: string | null; installHint?: string | null; providers: PiProvider[] }

const messageOf = (reason: unknown, fallback: string) => reason instanceof Error && reason.message ? reason.message : typeof reason === 'string' && reason ? reason : fallback

function ProviderRow({ provider, onChanged }: { provider: PiProvider; onChanged: () => void }) {
  const [editing, setEditing] = useState(false)
  const [key, setKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null)
  const [removing, setRemoving] = useState(false)
  const act = async (work: () => Promise<unknown>, fallback: string) => {
    setBusy(true); setNote(null)
    try { await work() } catch (reason) { setNote({ ok: false, text: messageOf(reason, fallback) }) } finally { setBusy(false) }
  }
  const save = () => act(async () => { await rpc('pi.key.set', { provider: provider.id, key: key.trim() }); setKey(''); setEditing(false); onChanged() }, 'That key could not be saved.')
  const test = () => act(async () => { const reply = await rpc<{ ok: boolean; detail: string | null }>('pi.key.test', { provider: provider.id }); setNote({ ok: reply.ok, text: reply.detail ?? (reply.ok ? 'The key works.' : 'The key did not work.') }) }, 'The key could not be tested.')
  return <div className="pi-row">
    <div className="pi-row-main">
      <KeyRound size={14} aria-hidden="true" />
      <span className="account-row-copy"><strong>{provider.name}</strong><small>{provider.hasKey ? `Key saved${provider.models !== null ? ` · ${provider.models} model${provider.models === 1 ? '' : 's'}` : ''}` : 'No key'}</small></span>
      {provider.docsUrl && <button type="button" className="icon-button" aria-label={`Where to get a ${provider.name} key`} title="Where to get a key" onClick={() => void openSafeLink(provider.docsUrl!)}><ExternalLink size={13} aria-hidden="true" /></button>}
      {provider.hasKey && <button type="button" className="secondary-button small" disabled={busy} onClick={() => void test()}>Test</button>}
      <button type="button" className="secondary-button small" disabled={busy} aria-expanded={editing} onClick={() => setEditing((value) => !value)}>{provider.hasKey ? 'Replace' : 'Add key'}</button>
      {provider.hasKey && <button type="button" className="icon-button" aria-label={`Remove the ${provider.name} key`} title="Remove" disabled={busy} onClick={() => setRemoving(true)}><Trash2 size={14} aria-hidden="true" /></button>}
    </div>
    {editing && <form className="pi-key-form" autoComplete="off" onSubmit={(event) => { event.preventDefault(); if (key.trim()) void save() }}>
      <input className="text-input" type="password" autoComplete="new-password" spellCheck={false} value={key} placeholder={`${provider.name} API key`} aria-label={`${provider.name} API key`} autoFocus onChange={(event) => setKey(event.target.value)} />
      <button type="submit" className="primary-button small" disabled={busy || !key.trim()}>Save</button>
    </form>}
    {note && <p className={`pi-note ${note.ok ? 'ok' : 'error'}`} role={note.ok ? 'status' : 'alert'}>{note.ok && <Check size={13} aria-hidden="true" />}{note.text}</p>}
    {removing && <ConfirmDialog title={`Remove the ${provider.name} key?`} body="Employees using Pi with this provider stop working until you add a key again." confirmLabel="Remove" cancelLabel="Keep it" tone="warning"
      onConfirm={() => { setRemoving(false); void act(async () => { await rpc('pi.key.delete', { provider: provider.id }); onChanged() }, 'That key could not be removed.') }} onCancel={() => setRemoving(false)} />}
  </div>
}

export function PiPage() {
  const [state, setState] = useState<PiState | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const load = useCallback(() => { void rpc<PiState>('pi.providers', {}).then(setState).catch((reason) => { setProblem(messageOf(reason, 'Pi could not be checked.')); setState({ installed: false, version: null, providers: [] }) }) }, [])
  useEffect(load, [load])
  const withKeys = state?.providers.filter((provider) => provider.hasKey).length ?? 0
  return <>
    <p className="settings-intro">Pi is an optional coding agent that reaches many model providers with your own API keys. Retinue keeps the keys in Windows Credential Manager and gives them only to the Pi it starts.</p>
    {problem && <p className="new-project-error" role="alert">{problem}</p>}
    {state === null ? <p className="settings-empty">Checking for Pi…</p> : <>
      <SettingsGroup title="Pi">
        <div className="pi-status"><span className="account-row-copy"><strong>{state.installed ? `Pi ${state.version ?? ''}`.trim() : 'Pi isn’t installed'}</strong>
          <small>{state.installed ? (withKeys ? `${withKeys} provider${withKeys === 1 ? '' : 's'} ready. Choose Pi as an employee's coding agent to use it.` : 'Add a provider key below to start.') : 'Install it with npm, then come back here.'}</small></span>
          {!state.installed && <code className="pi-install">{state.installHint ?? 'npm install -g @earendil-works/pi-coding-agent'}</code>}
        </div>
      </SettingsGroup>
      <SettingsGroup title="Providers">
        {!state.providers.length && <p className="settings-empty">{state.installed ? 'Pi reported no providers.' : 'Providers appear once Pi is installed.'}</p>}
        {state.providers.map((provider) => <ProviderRow key={provider.id} provider={provider} onChanged={load} />)}
      </SettingsGroup>
    </>}
  </>
}
