// Settings > Remote access > Chat apps (owner, 16 November 2026): talk to the CEO from Telegram or WhatsApp.
// One paired sender per channel, approved here; the CEO's approval cards can arrive with Allow and Deny buttons.
// Contract: docs/plans/vault-channels-pi.md, section B.
import { useCallback, useEffect, useState } from 'react'
import { Check, Copy, ExternalLink, MessageCircle, Send, Unplug } from 'lucide-react'
import { listenForDaemonEvents, rpc } from '../tauri'
import { ConfirmDialog } from './BlobPage'
import { SettingsGroup, SettingsRow } from './SettingsParts'
import { openSafeLink } from './openSafeLink'
import { SegmentedControl } from './SegmentedControl'
import './accounts.css'
import './channels.css'

export type ChannelKind = 'telegram' | 'whatsapp-web' | 'whatsapp-cloud'
export type Channel = {
  kind: ChannelKind
  status: 'off' | 'connecting' | 'pairing' | 'connected' | 'error'
  detail: string | null
  account: string | null
  peer: { name: string; handle: string | null } | null
  approvals: boolean
  sessionId: string | null
  lastMessageAt: string | null
  qrDataUrl?: string | null
  pairing?: { link: string | null; code: string; expiresAt: string } | null
  candidate?: { name: string; handle: string | null; at: string } | null
}

const messageOf = (reason: unknown, fallback: string) => reason instanceof Error && reason.message ? reason.message : typeof reason === 'string' && reason ? reason : fallback
const NAME: Record<ChannelKind, string> = { telegram: 'Telegram', 'whatsapp-web': 'WhatsApp', 'whatsapp-cloud': 'WhatsApp' }

export function channelStatusLine(channel: Channel): string {
  switch (channel.status) {
    case 'off': return 'Not connected'
    case 'connecting': return 'Connecting…'
    case 'pairing': return channel.candidate ? `${channel.candidate.name} wants to pair` : 'Waiting for you to pair'
    case 'connected': return channel.peer ? `Connected to ${channel.peer.name}${channel.peer.handle ? ` (${channel.peer.handle})` : ''}` : 'Connected'
    case 'error': return channel.detail ?? 'Something went wrong'
  }
}

function StatusDot({ channel }: { channel: Channel }) {
  return <span className={`channel-dot status-${channel.status}`} aria-hidden="true" />
}

function CandidateCard({ channel, busy, onAnswer }: { channel: Channel; busy: boolean; onAnswer: (approve: boolean) => void }) {
  if (!channel.candidate) return null
  return <div className="channel-candidate" role="group" aria-label="Pairing request">
    <span className="account-row-copy"><strong>{channel.candidate.name}{channel.candidate.handle ? ` · ${channel.candidate.handle}` : ''} wants to talk to your CEO</strong><small>Approve only if this is you. They will be the only person who can message your CEO here.</small></span>
    <button type="button" className="secondary-button small" disabled={busy} onClick={() => onAnswer(false)}>Reject</button>
    <button type="button" className="primary-button small" disabled={busy} onClick={() => onAnswer(true)}>Approve</button>
  </div>
}

function ConnectedRows({ channel, busy, onUpdate, onTest, onDisconnect }: { channel: Channel; busy: boolean; onUpdate: (approvals: boolean) => void; onTest: () => void; onDisconnect: () => void }) {
  return <>
    <SettingsRow label="Approval buttons" hint={`Your CEO's approval requests also arrive in ${NAME[channel.kind]} with Allow and Deny buttons. Vault items, computer control and settings are always approved here.`}>
      <button type="button" role="switch" aria-checked={channel.approvals} aria-label="Approval buttons in chat" className={`toggle ${channel.approvals ? 'on' : ''}`} disabled={busy} onClick={() => onUpdate(!channel.approvals)}><i /></button>
    </SettingsRow>
    <div className="channel-actions">
      <button type="button" className="secondary-button small" disabled={busy} onClick={onTest}><Send size={13} aria-hidden="true" />Send a test message</button>
      <button type="button" className="secondary-button small" disabled={busy} onClick={onDisconnect}><Unplug size={13} aria-hidden="true" />Disconnect</button>
    </div>
  </>
}

function TelegramCard({ channel, busy, run }: { channel: Channel; busy: boolean; run: (method: string, params: Record<string, unknown>, after?: (reply: unknown) => void) => void }) {
  const [token, setToken] = useState('')
  const [copied, setCopied] = useState(false)
  const [confirmOff, setConfirmOff] = useState(false)
  const off = channel.status === 'off' || channel.status === 'error'
  return <SettingsGroup title="Telegram">
    <div className="channel-head"><StatusDot channel={channel} /><span className="account-row-copy"><strong>{channel.account ?? 'Telegram bot'}</strong><small>{channelStatusLine(channel)}</small></span></div>
    {off && <form className="channel-form" autoComplete="off" onSubmit={(event) => { event.preventDefault(); if (token.trim()) run('channel.telegram.connect', { botToken: token.trim() }, () => setToken('')) }}>
      <ol className="channel-steps">
        <li>In Telegram, open <button type="button" className="markdown-link" onClick={() => void openSafeLink('https://t.me/BotFather')}>@BotFather</button> and send <code>/newbot</code>.</li>
        <li>Paste the token it gives you here.</li>
      </ol>
      <div className="channel-form-row">
        <input className="text-input" type="password" autoComplete="new-password" spellCheck={false} value={token} placeholder="123456789:AA…" aria-label="Telegram bot token" onChange={(event) => setToken(event.target.value)} />
        <button type="submit" className="primary-button small" disabled={busy || !token.trim()}>Connect</button>
      </div>
    </form>}
    {channel.status === 'pairing' && channel.pairing && !channel.candidate && <div className="channel-pairing">
      <p>Open your bot in Telegram and press Start, or send it this code. The code works for 10 minutes.</p>
      <div className="channel-code"><code>{channel.pairing.code}</code>
        <button type="button" className="icon-button" aria-label="Copy the code" title={copied ? 'Copied' : 'Copy'} onClick={() => { void navigator.clipboard.writeText(channel.pairing!.code).then(() => setCopied(true)).catch(() => undefined) }}>{copied ? <Check size={13} aria-hidden="true" /> : <Copy size={13} aria-hidden="true" />}</button>
        {channel.pairing.link && <button type="button" className="secondary-button small" onClick={() => void openSafeLink(channel.pairing!.link!)}><ExternalLink size={13} aria-hidden="true" />Open in Telegram</button>}
      </div>
    </div>}
    <CandidateCard channel={channel} busy={busy} onAnswer={(approve) => run('channel.pairing.answer', { kind: channel.kind, approve })} />
    {channel.status === 'connected' && <ConnectedRows channel={channel} busy={busy} onUpdate={(approvals) => run('channel.update', { kind: channel.kind, approvals })} onTest={() => run('channel.test', { kind: channel.kind })} onDisconnect={() => setConfirmOff(true)} />}
    {channel.status === 'pairing' && <div className="channel-actions"><button type="button" className="secondary-button small" disabled={busy} onClick={() => setConfirmOff(true)}>Cancel</button></div>}
    {confirmOff && <ConfirmDialog title="Disconnect Telegram?" body="Retinue forgets the bot token and the paired sender. Your CEO's Telegram conversation stays in Retinue." confirmLabel="Disconnect" cancelLabel="Keep it" tone="warning"
      onConfirm={() => { setConfirmOff(false); run('channel.disconnect', { kind: channel.kind }) }} onCancel={() => setConfirmOff(false)} />}
  </SettingsGroup>
}

function WhatsAppCard({ web, cloud, busy, run }: { web: Channel; cloud: Channel; busy: boolean; run: (method: string, params: Record<string, unknown>, after?: (reply: unknown) => void) => void }) {
  const active = cloud.status !== 'off' ? cloud : web
  const [method, setMethod] = useState<ChannelKind>(active.kind)
  useEffect(() => { if (active.status !== 'off') setMethod(active.kind) }, [active.kind, active.status])
  const [form, setForm] = useState({ phoneNumberId: '', accessToken: '', appSecret: '' })
  const [webhook, setWebhook] = useState<{ url: string | null; verifyToken: string } | null>(null)
  const [confirmOff, setConfirmOff] = useState(false)
  const channel = method === 'whatsapp-web' ? web : cloud
  const off = channel.status === 'off' || channel.status === 'error'
  const locked = active.status !== 'off' && active.status !== 'error'
  return <SettingsGroup title="WhatsApp">
    <div className="channel-head"><StatusDot channel={channel} /><span className="account-row-copy"><strong>{channel.account ?? 'WhatsApp'}</strong><small>{channelStatusLine(channel)}</small></span></div>
    {!locked && <div className="channel-method"><SegmentedControl label="How to connect WhatsApp" value={method} options={[{ value: 'whatsapp-web', label: 'Your number (QR)' }, { value: 'whatsapp-cloud', label: 'Business API' }]} onChange={setMethod} /></div>}
    {method === 'whatsapp-web' && off && <div className="channel-form">
      <p className="channel-note">Links like WhatsApp Desktop: scan a QR code with your phone. This isn't an official WhatsApp connection, and WhatsApp can block numbers that use one, so a spare number is safest.</p>
      <div className="channel-actions"><button type="button" className="primary-button small" disabled={busy} onClick={() => run('channel.whatsappWeb.start', {})}><MessageCircle size={13} aria-hidden="true" />Show the QR code</button></div>
    </div>}
    {method === 'whatsapp-web' && channel.status === 'pairing' && !channel.candidate && <div className="channel-pairing">
      <p>On your phone, open WhatsApp, then Settings, Linked devices, Link a device, and scan this code.</p>
      {channel.qrDataUrl ? <img className="channel-qr" src={channel.qrDataUrl} alt="WhatsApp pairing QR code" width={208} height={208} /> : <p className="channel-note">Waiting for the code…</p>}
    </div>}
    {method === 'whatsapp-cloud' && off && <form className="channel-form" autoComplete="off" onSubmit={(event) => { event.preventDefault(); run('channel.whatsappCloud.connect', { phoneNumberId: form.phoneNumberId.trim(), accessToken: form.accessToken.trim(), appSecret: form.appSecret.trim() }, (reply) => setWebhook((reply as { webhook?: { url: string | null; verifyToken: string } })?.webhook ?? null)) }}>
      <p className="channel-note">Meta's official API. It needs a Meta business app with its own phone number. Copy these from the app's WhatsApp settings.</p>
      <label className="new-project-field"><span>Phone number ID</span><input className="text-input" value={form.phoneNumberId} inputMode="numeric" spellCheck={false} onChange={(event) => setForm({ ...form, phoneNumberId: event.target.value })} /></label>
      <label className="new-project-field"><span>Access token</span><input className="text-input" type="password" autoComplete="new-password" spellCheck={false} value={form.accessToken} onChange={(event) => setForm({ ...form, accessToken: event.target.value })} /></label>
      <label className="new-project-field"><span>App secret</span><input className="text-input" type="password" autoComplete="new-password" spellCheck={false} value={form.appSecret} onChange={(event) => setForm({ ...form, appSecret: event.target.value })} /></label>
      <div className="channel-actions"><button type="submit" className="primary-button small" disabled={busy || !form.phoneNumberId.trim() || !form.accessToken.trim() || !form.appSecret.trim()}>Connect</button></div>
    </form>}
    {method === 'whatsapp-cloud' && webhook && <div className="channel-pairing">
      <p>In your Meta app, set the webhook to this address and verify token, then subscribe to messages.</p>
      <div className="channel-code"><span>Address</span><code>{webhook.url ?? 'Not available yet: link your cloud computer first.'}</code></div>
      <div className="channel-code"><span>Verify token</span><code>{webhook.verifyToken}</code></div>
    </div>}
    <CandidateCard channel={channel} busy={busy} onAnswer={(approve) => run('channel.pairing.answer', { kind: channel.kind, approve })} />
    {channel.status === 'connected' && <ConnectedRows channel={channel} busy={busy} onUpdate={(approvals) => run('channel.update', { kind: channel.kind, approvals })} onTest={() => run('channel.test', { kind: channel.kind })} onDisconnect={() => setConfirmOff(true)} />}
    {channel.status === 'pairing' && <div className="channel-actions"><button type="button" className="secondary-button small" disabled={busy} onClick={() => setConfirmOff(true)}>Cancel</button></div>}
    {confirmOff && <ConfirmDialog title="Disconnect WhatsApp?" body="Retinue unlinks this device and forgets the paired sender. Your CEO's WhatsApp conversation stays in Retinue." confirmLabel="Disconnect" cancelLabel="Keep it" tone="warning"
      onConfirm={() => { setConfirmOff(false); run('channel.disconnect', { kind: channel.kind }) }} onCancel={() => setConfirmOff(false)} />}
  </SettingsGroup>
}

const offChannel = (kind: ChannelKind): Channel => ({ kind, status: 'off', detail: null, account: null, peer: null, approvals: true, sessionId: null, lastMessageAt: null })

export function ChannelsPage() {
  const [channels, setChannels] = useState<Channel[] | null>(null)
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const load = useCallback(() => { void rpc<{ channels?: Channel[] }>('channel.list', {}).then((reply) => setChannels(reply?.channels ?? [])).catch((reason) => { setChannels([]); setProblem(messageOf(reason, 'Chat apps could not be read.')) }) }, [])
  useEffect(() => {
    load()
    let live = true
    let stop: (() => void) | null = null
    void listenForDaemonEvents((event) => {
      if (event.type === 'channel.updated' || event.type === 'channel.pairing.candidate') load()
      if (event.type === 'channel.qr') {
        const { kind, qrDataUrl } = (event.payload ?? {}) as { kind?: ChannelKind; qrDataUrl?: string }
        if (kind && qrDataUrl) setChannels((list) => (list ?? []).map((channel) => channel.kind === kind ? { ...channel, status: 'pairing', qrDataUrl } : channel))
      }
    }).then((unlisten) => { if (live) stop = unlisten; else unlisten() }).catch(() => undefined)
    return () => { live = false; stop?.() }
  }, [load])
  const run = (method: string, params: Record<string, unknown>, after?: (reply: unknown) => void) => {
    setBusy(true); setProblem(null)
    void rpc(method, params).then((reply) => { after?.(reply); load() }).catch((reason) => setProblem(messageOf(reason, 'That could not be done.'))).finally(() => setBusy(false))
  }
  const find = (kind: ChannelKind) => channels?.find((channel) => channel.kind === kind) ?? offChannel(kind)
  return <>
    <p className="settings-intro">Message your CEO from your phone. Only you can pair, and only your CEO answers. Your messages and its replies pass through Telegram or Meta.</p>
    {problem && <p className="new-project-error" role="alert">{problem}</p>}
    {channels === null ? <p className="settings-empty">Loading…</p> : <>
      <TelegramCard channel={find('telegram')} busy={busy} run={run} />
      <WhatsAppCard web={find('whatsapp-web')} cloud={find('whatsapp-cloud')} busy={busy} run={run} />
    </>}
  </>
}
