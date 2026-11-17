// Settings > Vault (owner, 16 November 2026): logins, cards, API keys and notes that employees can use without
// ever seeing them. Values live in Windows Credential Manager through the daemon; a bot asks to use an item, the
// owner approves, and Retinue types the value itself. Contract: docs/plans/vault-channels-pi.md, section A.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Copy, CreditCard, Eye, EyeOff, FileText, Globe, KeyRound, Pencil, Plus, Search, ShieldCheck, Trash2 } from 'lucide-react'
import { listenForDaemonEvents, rpc } from '../tauri'
import { ConfirmDialog } from './BlobPage'
import { useDialogAccessibility, useDialogEntered } from './dialogFocus'
import { SettingsGroup } from './SettingsParts'
import { secondsLeft } from './SecretsPage'
import './accounts.css'
import './vault.css'

export type VaultKind = 'login' | 'card' | 'apiKey' | 'note'
export type VaultItem = {
  id: string; kind: VaultKind; title: string
  origin: string | null; username: string | null; holder: string | null; brand: string | null; last4: string | null; expiry: string | null; service: string | null
  hasTotp: boolean
  createdAt: string; updatedAt: string; lastUsedAt: string | null; useCount: number
}
export type VaultActivity = { id: string; at: string; itemId: string | null; itemTitle: string | null; action: 'fill' | 'reveal' | 'create' | 'update' | 'delete' | 'denied' | 'expired' | 'uncertain'; where: 'browser' | 'cloud' | 'owner' | 'desktop'; origin: string | null; bot: string | null; sessionId: string | null }
export type VaultUseRequest = { requestId: string; sessionId: string; bot: string; itemId: string; itemTitle: string; kind: VaultKind; field: string; surface: 'browser' | 'cloud' | 'owner'; origin: string | null; expiresAt: string | null }

const messageOf = (reason: unknown, fallback: string) => reason instanceof Error && reason.message ? reason.message : typeof reason === 'string' && reason ? reason : fallback

export const KIND_LABEL: Record<VaultKind, string> = { login: 'Login', card: 'Card', apiKey: 'API key', note: 'Note' }
const KIND_ICON = { login: Globe, card: CreditCard, apiKey: KeyRound, note: FileText }
/** The secret fields of each kind, in the order the editor shows them. */
const SECRET_FIELDS: Record<VaultKind, Array<{ id: string; label: string }>> = {
  login: [{ id: 'password', label: 'Password' }, { id: 'totp', label: 'Authenticator key' }],
  card: [{ id: 'number', label: 'Card number' }, { id: 'cvc', label: 'Security code' }],
  apiKey: [{ id: 'value', label: 'Key' }],
  note: [{ id: 'text', label: 'Note' }],
}
const FIELD_LABEL: Record<string, string> = { password: 'password', totp: 'authenticator code', number: 'card number', cvc: 'security code', value: 'key', text: 'note', username: 'username' }
const SURFACE_LABEL = { browser: 'its browser', cloud: 'the cloud computer', owner: 'your computer' }
const WHERE_LABEL = { browser: 'in its browser', cloud: 'on the cloud computer', owner: 'on your computer', desktop: 'here' }

/** One line under an item's title: what identifies it without showing anything secret. */
export function vaultSubtitle(item: VaultItem): string {
  if (item.kind === 'login') return [item.username, item.origin?.replace(/^https?:\/\//, '')].filter(Boolean).join(' · ') || 'Login'
  if (item.kind === 'card') return [item.brand && item.last4 ? `${item.brand} •••• ${item.last4}` : item.last4 ? `•••• ${item.last4}` : null, item.expiry ? `expires ${item.expiry}` : null].filter(Boolean).join(' · ') || 'Card'
  if (item.kind === 'apiKey') return item.service || 'API key'
  return 'Note'
}

/** Luhn check for a card number typed with or without spaces. The daemon checks again. */
export function cardNumberProblem(number: string): string | null {
  const digits = number.replace(/[\s-]/g, '')
  if (!/^\d{12,19}$/.test(digits)) return 'A card number has 12 to 19 digits.'
  let sum = 0
  for (let index = 0; index < digits.length; index += 1) {
    let digit = Number(digits[digits.length - 1 - index])
    if (index % 2 === 1) { digit *= 2; if (digit > 9) digit -= 9 }
    sum += digit
  }
  return sum % 10 === 0 ? null : 'That card number doesn’t look right.'
}

export function originProblem(origin: string): string | null {
  try {
    const url = new URL(origin.includes('://') ? origin : `https://${origin}`)
    return url.protocol === 'https:' || url.hostname === 'localhost' ? null : 'Use the site’s https address.'
  } catch { return 'Use the site’s address, like github.com.' }
}

function normalOrigin(origin: string) {
  const url = new URL(origin.includes('://') ? origin : `https://${origin}`)
  return url.origin
}

function ago(at: string | null) {
  if (!at) return 'Never used'
  const minutes = Math.round((Date.now() - Date.parse(at)) / 60000)
  if (!Number.isFinite(minutes)) return ''
  if (minutes < 1) return 'Used just now'
  if (minutes < 60) return `Used ${minutes} min ago`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return `Used ${hours} h ago`
  return `Used ${new Date(at).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}`
}

function activityLine(row: VaultActivity) {
  const what = row.itemTitle ?? 'An item'
  const who = row.bot ?? 'You'
  switch (row.action) {
    case 'fill': return `${who} used ${what} ${WHERE_LABEL[row.where]}${row.origin ? ` on ${row.origin.replace(/^https?:\/\//, '')}` : ''}`
    case 'reveal': return `You revealed ${what}`
    case 'create': return `You added ${what}`
    case 'update': return `You changed ${what}`
    case 'delete': return `You deleted ${what}`
    case 'denied': return `${who} wasn’t allowed to use ${what}`
    case 'expired': return `${who}’s request for ${what} ran out of time`
    case 'uncertain': return `${who} may have used ${what}; the page changed before Retinue could confirm`
  }
}

type Draft = { kind: VaultKind; title: string; origin: string; username: string; holder: string; expiry: string; service: string; secrets: Record<string, string> }
const emptyDraft = (kind: VaultKind): Draft => ({ kind, title: '', origin: '', username: '', holder: '', expiry: '', service: '', secrets: {} })

function VaultEditor({ item, kind, onClose, onSaved }: { item: VaultItem | null; kind: VaultKind; onClose: () => void; onSaved: () => void }) {
  const entered = useDialogEntered()
  const { ref } = useDialogAccessibility(onClose, () => true)
  const [draft, setDraft] = useState<Draft>(() => item ? { kind: item.kind, title: item.title, origin: item.origin ?? '', username: item.username ?? '', holder: item.holder ?? '', expiry: item.expiry ?? '', service: item.service ?? '', secrets: {} } : emptyDraft(kind))
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const first = useRef<HTMLInputElement>(null)
  useEffect(() => { first.current?.focus() }, [])
  const set = (patch: Partial<Draft>) => setDraft((value) => ({ ...value, ...patch }))
  const setSecret = (id: string, value: string) => setDraft((current) => ({ ...current, secrets: { ...current.secrets, [id]: value } }))
  const fields = SECRET_FIELDS[draft.kind]
  const required = fields[0]!.id
  const check = (): string | null => {
    if (!draft.title.trim()) return 'Give it a name.'
    if (draft.kind === 'login') { if (!draft.origin.trim()) return 'Add the site’s address, so Retinue only fills it there.'; const bad = originProblem(draft.origin.trim()); if (bad) return bad }
    if (draft.kind === 'card' && draft.expiry && !/^(0[1-9]|1[0-2])\/\d{2}$/.test(draft.expiry.trim())) return 'Write the expiry as MM/YY.'
    if (!item && !draft.secrets[required]) return `Add the ${FIELD_LABEL[required] ?? 'value'}.`
    if (draft.kind === 'card' && draft.secrets.number) return cardNumberProblem(draft.secrets.number)
    return null
  }
  const save = async () => {
    const bad = check()
    if (bad) { setProblem(bad); return }
    setBusy(true); setProblem(null)
    const secrets = Object.fromEntries(Object.entries(draft.secrets).filter(([, value]) => value !== ''))
    const body = {
      title: draft.title.trim(),
      ...(draft.kind === 'login' ? { origin: normalOrigin(draft.origin.trim()), username: draft.username.trim() || null } : {}),
      ...(draft.kind === 'card' ? { holder: draft.holder.trim() || null, expiry: draft.expiry.trim() || null } : {}),
      ...(draft.kind === 'apiKey' ? { service: draft.service.trim() || null } : {}),
      ...(Object.keys(secrets).length ? { secrets } : {}),
    }
    try {
      await rpc(item ? 'vault.update' : 'vault.create', item ? { id: item.id, ...body } : { kind: draft.kind, ...body, secrets })
      onSaved()
    } catch (reason) { setProblem(messageOf(reason, 'That could not be saved.')); setBusy(false) }
  }
  const text = (label: string, value: string, onChange: (value: string) => void, extra: Partial<React.InputHTMLAttributes<HTMLInputElement>> = {}, hint?: string) =>
    <label className="new-project-field"><span>{label}</span><input className="text-input" value={value} spellCheck={false} onChange={(event) => onChange(event.target.value)} {...extra} />{hint && <small className="project-settings-note">{hint}</small>}</label>
  // Rendered inside the Settings sheet, like ConfirmDialog, so it stacks above it.
  return <div className={`sheet-backdrop blob-dialog-backdrop ${entered ? 'is-entered' : ''}`} onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose() }}>
    <form ref={ref as unknown as React.RefObject<HTMLFormElement>} className="blob-dialog vault-editor" role="dialog" aria-modal="true" aria-labelledby="vault-editor-title" tabIndex={-1} autoComplete="off" onSubmit={(event) => { event.preventDefault(); void save() }}>
      <h2 id="vault-editor-title">{item ? `Edit ${item.title}` : `New ${KIND_LABEL[draft.kind].toLowerCase()}`}</h2>
      <label className="new-project-field"><span>Name</span><input ref={first} className="text-input" value={draft.title} maxLength={80} onChange={(event) => set({ title: event.target.value })} placeholder={draft.kind === 'login' ? 'GitHub' : draft.kind === 'card' ? 'Personal Visa' : draft.kind === 'apiKey' ? 'OpenAI' : 'Wi-Fi at home'} /></label>
      {draft.kind === 'login' && <>
        {text('Site', draft.origin, (value) => set({ origin: value }), { placeholder: 'github.com', inputMode: 'url' }, 'Retinue fills this login only on this site.')}
        {text('Username or email', draft.username, (value) => set({ username: value }), { autoComplete: 'off' })}
      </>}
      {draft.kind === 'card' && <div className="vault-editor-pair">
        {text('Name on card', draft.holder, (value) => set({ holder: value }))}
        {text('Expiry', draft.expiry, (value) => set({ expiry: value }), { placeholder: 'MM/YY', maxLength: 5, inputMode: 'numeric' })}
      </div>}
      {draft.kind === 'apiKey' && text('Service', draft.service, (value) => set({ service: value }), { placeholder: 'OpenAI' })}
      {fields.map((field, index) => <label key={field.id} className="new-project-field"><span>{field.label}{index > 0 ? ' (optional)' : ''}</span>
        {field.id === 'text'
          ? <textarea className="text-input vault-note-input" value={draft.secrets[field.id] ?? ''} rows={5} placeholder={item ? 'Unchanged' : ''} onChange={(event) => setSecret(field.id, event.target.value)} />
          : <input className="text-input" type="password" autoComplete="new-password" spellCheck={false} value={draft.secrets[field.id] ?? ''} placeholder={item ? 'Unchanged' : ''} inputMode={field.id === 'number' || field.id === 'cvc' ? 'numeric' : undefined} onChange={(event) => setSecret(field.id, event.target.value)} />}
      </label>)}
      <small className="project-settings-note">Saved in Windows Credential Manager. Employees never see it: when one needs it, you approve, and Retinue types it in.</small>
      {problem && <p className="new-project-error" role="alert">{problem}</p>}
      <div className="blob-dialog-actions">
        <button type="button" className="secondary-button" disabled={busy} onClick={onClose}>Cancel</button>
        <button type="submit" className="primary-button" disabled={busy}>{item ? 'Save changes' : 'Add to vault'}</button>
      </div>
    </form>
  </div>
}

/** A revealed value, shown for 30 seconds then hidden again. */
function Revealed({ value, onHide }: { value: string; onHide: () => void }) {
  const [shown, setShown] = useState(true)
  const [copied, setCopied] = useState(false)
  useEffect(() => { const timer = window.setTimeout(onHide, 30_000); return () => window.clearTimeout(timer) }, [onHide])
  return <span className="vault-revealed">
    <code>{shown ? value : '•'.repeat(Math.min(value.length, 16))}</code>
    <button type="button" className="icon-button" aria-label={shown ? 'Hide' : 'Show'} title={shown ? 'Hide' : 'Show'} onClick={() => setShown((value) => !value)}>{shown ? <EyeOff size={13} aria-hidden="true" /> : <Eye size={13} aria-hidden="true" />}</button>
    <button type="button" className="icon-button" aria-label="Copy" title={copied ? 'Copied' : 'Copy'} onClick={() => { void navigator.clipboard.writeText(value).then(() => setCopied(true)).catch(() => undefined) }}><Copy size={13} aria-hidden="true" /></button>
  </span>
}

export function VaultPage() {
  const [items, setItems] = useState<VaultItem[] | null>(null)
  const [activity, setActivity] = useState<VaultActivity[]>([])
  const [query, setQuery] = useState('')
  const [adding, setAdding] = useState<VaultKind | null>(null)
  const [menuOpen, setMenuOpen] = useState(false)
  const [editing, setEditing] = useState<VaultItem | null>(null)
  const [removing, setRemoving] = useState<VaultItem | null>(null)
  const [revealing, setRevealing] = useState<{ item: VaultItem; field: string } | null>(null)
  const [revealed, setRevealed] = useState<{ itemId: string; field: string; value: string } | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const load = useCallback(() => {
    void rpc<{ items?: VaultItem[] }>('vault.list', {}).then((reply) => setItems(reply?.items ?? [])).catch((reason) => { setItems([]); setProblem(messageOf(reason, 'The vault could not be read.')) })
    void rpc<{ rows?: VaultActivity[] }>('vault.activity', { limit: 12 }).then((reply) => setActivity(reply?.rows ?? [])).catch(() => undefined)
  }, [])
  useEffect(() => {
    load()
    let live = true
    let stop: (() => void) | null = null
    void listenForDaemonEvents((event) => { if (event.type === 'vault.use.updated' || event.type === 'vault.changed') load() }).then((unlisten) => { if (live) stop = unlisten; else unlisten() }).catch(() => undefined)
    return () => { live = false; stop?.() }
  }, [load])
  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase()
    const list = items ?? []
    return needle ? list.filter((item) => [item.title, vaultSubtitle(item), KIND_LABEL[item.kind]].join(' ').toLowerCase().includes(needle)) : list
  }, [items, query])
  const reveal = async (item: VaultItem, field: string) => {
    setRevealing(null); setProblem(null)
    try {
      const reply = field === 'totp' ? await rpc<{ code: string }>('vault.totp', { id: item.id }) : await rpc<{ value: string }>('vault.reveal', { id: item.id, field })
      setRevealed({ itemId: item.id, field, value: 'code' in reply ? reply.code : reply.value })
      load()
    } catch (reason) { setProblem(messageOf(reason, 'That could not be revealed.')) }
  }
  const hideRevealed = useCallback(() => setRevealed(null), [])
  return <>
    <p className="settings-intro">Logins, cards and keys your employees can use without ever seeing them. When one needs an item, you approve, and Retinue types it into the page itself.</p>
    <SettingsGroup title="Items" action={<div className="vault-add">
      <button type="button" className="secondary-button small" aria-haspopup="menu" aria-expanded={menuOpen} onClick={() => setMenuOpen((open) => !open)}><Plus size={13} aria-hidden="true" />Add</button>
      {menuOpen && <div className="vault-add-menu" role="menu" onKeyDown={(event) => { if (event.key === 'Escape') setMenuOpen(false) }}>
        {(Object.keys(KIND_LABEL) as VaultKind[]).map((kind) => { const Icon = KIND_ICON[kind]; return <button key={kind} type="button" role="menuitem" onClick={() => { setMenuOpen(false); setAdding(kind) }}><Icon size={14} aria-hidden="true" />{KIND_LABEL[kind]}</button> })}
      </div>}
    </div>}>
      {(items?.length ?? 0) > 5 && <label className="vault-search"><Search size={13} aria-hidden="true" /><input className="text-input" value={query} placeholder="Search the vault" aria-label="Search the vault" onChange={(event) => setQuery(event.target.value)} /></label>}
      {items === null && <p className="settings-empty">Opening the vault…</p>}
      {items !== null && !items.length && <p className="settings-empty">Nothing saved yet. Add a login, a card or an API key.</p>}
      {items !== null && items.length > 0 && !shown.length && <p className="settings-empty">Nothing matches “{query.trim()}”.</p>}
      {shown.map((item) => {
        const Icon = KIND_ICON[item.kind]
        const fields = SECRET_FIELDS[item.kind].filter((field) => field.id !== 'totp' || item.hasTotp)
        return <div key={item.id} className="vault-row">
          <span className={`vault-row-icon kind-${item.kind}`} aria-hidden="true"><Icon size={15} /></span>
          <span className="account-row-copy"><strong>{item.title}</strong><small>{vaultSubtitle(item)}</small>
            {revealed?.itemId === item.id && <Revealed key={`${revealed.field}-${revealed.value}`} value={revealed.value} onHide={hideRevealed} />}
          </span>
          <small className="vault-row-used">{ago(item.lastUsedAt)}</small>
          <span className="vault-row-actions">
            {fields.map((field) => <button key={field.id} type="button" className="icon-button" aria-label={`Reveal ${FIELD_LABEL[field.id]} for ${item.title}`} title={`Reveal ${FIELD_LABEL[field.id]}`} onClick={() => setRevealing({ item, field: field.id })}>{field.id === 'totp' ? <ShieldCheck size={14} aria-hidden="true" /> : <Eye size={14} aria-hidden="true" />}</button>)}
            <button type="button" className="icon-button" aria-label={`Edit ${item.title}`} title="Edit" onClick={() => setEditing(item)}><Pencil size={14} aria-hidden="true" /></button>
            <button type="button" className="icon-button" aria-label={`Delete ${item.title}`} title="Delete" onClick={() => setRemoving(item)}><Trash2 size={14} aria-hidden="true" /></button>
          </span>
        </div>
      })}
      {problem && <p className="new-project-error vault-problem" role="alert">{problem}</p>}
    </SettingsGroup>
    <SettingsGroup title="Recent activity">
      {!activity.length && <p className="settings-empty">No activity yet. Every use and reveal is listed here, never the value.</p>}
      {activity.map((row) => <div key={row.id} className={`vault-activity-row action-${row.action}`}><span>{activityLine(row)}</span><time dateTime={row.at}>{new Date(row.at).toLocaleString(undefined, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })}</time></div>)}
    </SettingsGroup>
    {(adding || editing) && <VaultEditor item={editing} kind={editing?.kind ?? adding ?? 'login'} onClose={() => { setAdding(null); setEditing(null) }} onSaved={() => { setAdding(null); setEditing(null); load() }} />}
    {revealing && <ConfirmDialog title={`Reveal the ${FIELD_LABEL[revealing.field]} for ${revealing.item.title}?`} body="It shows here for 30 seconds. The reveal is added to the vault's activity." confirmLabel="Reveal" cancelLabel="Cancel"
      onConfirm={() => void reveal(revealing.item, revealing.field)} onCancel={() => setRevealing(null)} />}
    {removing && <ConfirmDialog title={`Delete ${removing.title}?`} body="It's removed from the vault and from Windows Credential Manager. Employees can't use it any more, and it can't be brought back." confirmLabel="Delete" cancelLabel="Keep it" tone="warning"
      onConfirm={() => { const target = removing; setRemoving(null); setProblem(null); void rpc('vault.delete', { id: target.id, confirm: true }).then(load).catch((reason) => setProblem(messageOf(reason, 'That item could not be deleted.'))) }}
      onCancel={() => setRemoving(null)} />}
  </>
}

function VaultUseDialog({ request, onDone }: { request: VaultUseRequest; onDone: () => void }) {
  const entered = useDialogEntered()
  const { ref } = useDialogAccessibility(() => undefined, () => false)
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => { const timer = window.setInterval(() => setNow(Date.now()), 1000); return () => window.clearInterval(timer) }, [])
  const left = secondsLeft(request.expiresAt, now)
  const answer = async (decision: 'allow' | 'deny') => {
    setBusy(true); setProblem(null)
    try { await rpc('vault.use.answer', { requestId: request.requestId, decision }); onDone() } catch (reason) { setProblem(messageOf(reason, 'That could not be done.')); setBusy(false) }
  }
  const Icon = KIND_ICON[request.kind]
  return createPortal(<div className={`sheet-backdrop blob-dialog-backdrop vault-use-backdrop ${entered ? 'is-entered' : ''}`}>
    <section ref={ref as unknown as React.RefObject<HTMLElement>} className="blob-dialog vault-use" role="alertdialog" aria-modal="true" aria-labelledby="vault-use-title" aria-describedby="vault-use-body" tabIndex={-1}>
      <h2 id="vault-use-title">{request.bot} wants to use {request.itemTitle}</h2>
      <div className="vault-use-item"><span className={`vault-row-icon kind-${request.kind}`} aria-hidden="true"><Icon size={15} /></span><span className="account-row-copy"><strong>{request.itemTitle}</strong><small>{KIND_LABEL[request.kind]}{request.origin ? ` · ${request.origin.replace(/^https?:\/\//, '')}` : ''}</small></span></div>
      <p id="vault-use-body" className="secret-reason">Retinue will type the {FIELD_LABEL[request.field] ?? request.field} into {SURFACE_LABEL[request.surface]}{request.origin ? `, only on ${request.origin.replace(/^https?:\/\//, '')}` : ''}. {request.bot} won’t see it.</p>
      {left !== null && <small className="project-settings-note" role="timer">{left > 0 ? `Answer within ${left} second${left === 1 ? '' : 's'}.` : 'This request has run out of time.'}</small>}
      {problem && <p className="new-project-error" role="alert">{problem}</p>}
      <div className="blob-dialog-actions">
        <button type="button" className="secondary-button" disabled={busy} onClick={() => void answer('deny')}>Deny</button>
        <button type="button" className="primary-button" disabled={busy || left === 0} onClick={() => void answer('allow')}>Allow once</button>
      </div>
    </section>
  </div>, document.body)
}

/** Mounted in the main window. Shows the oldest waiting vault use; a phone or chat can never answer one. */
export function VaultUseRequests({ active = true }: { active?: boolean }) {
  const [requests, setRequests] = useState<VaultUseRequest[]>([])
  const load = useCallback(() => { void rpc<{ requests?: VaultUseRequest[] }>('vault.use.list', {}).then((reply) => setRequests(reply?.requests ?? [])).catch(() => undefined) }, [])
  useEffect(() => {
    if (!active) return
    load()
    let live = true
    let stop: (() => void) | null = null
    void listenForDaemonEvents((event) => { if (event.type === 'vault.use.requested' || event.type === 'vault.use.updated') load() }).then((unlisten) => { if (live) stop = unlisten; else unlisten() }).catch(() => undefined)
    return () => { live = false; stop?.() }
  }, [active, load])
  const first = requests[0]
  return first ? <VaultUseDialog key={first.requestId} request={first} onDone={load} /> : null
}
