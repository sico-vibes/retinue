// Bots controlling this computer (owner amendment, 14 November 2026). Off by
// default; when on, each conversation still asks first. While a bot holds a grant,
// a bar names it with a Stop button; the companion shows the same.
import { useEffect, useState } from 'react'
import { MousePointerClick, Square } from 'lucide-react'
import { listenForDaemonEvents, rpc } from '../tauri'
import { SettingsGroup, SettingsRow } from './SettingsParts'
import './ownerComputer.css'

export const OWNER_COMPUTER_SETTING = 'experimental.ownerComputer'

function recordFrom(value: unknown): Record<string, unknown> { return value && typeof value === 'object' ? value as Record<string, unknown> : {} }

/** Settings › Computer: the switch, with what it does in plain words. */
export function OwnerComputerSettings() {
  const [on, setOn] = useState<boolean | null>(null)
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  useEffect(() => {
    let live = true
    void rpc<Record<string, unknown>>('settings.get').then((reply) => { if (!live) return; const settings = recordFrom(recordFrom(reply).settings ?? reply); setOn(settings[OWNER_COMPUTER_SETTING] === true) }).catch(() => { if (live) setOn(false) })
    return () => { live = false }
  }, [])
  const flip = async () => {
    if (on === null) return
    setBusy(true); setProblem(null)
    try { await rpc('settings.set', { key: OWNER_COMPUTER_SETTING, value: !on }); setOn(!on) } catch (error) { setProblem(error instanceof Error ? error.message : 'The setting could not be saved.') } finally { setBusy(false) }
  }
  return <SettingsGroup title="This computer">
    <SettingsRow label="Let employees control this computer" hint="Off: no employee can read your screen or use your mouse and keyboard. On: an employee still has to ask in its conversation, you approve it there, and a bar with Stop shows while it works. Screens are read as text, with password fields hidden; no pictures of your screen are sent.">
      <button type="button" className={`toggle ${on ? 'on' : ''}`} role="switch" aria-label="Let employees control this computer" aria-checked={!!on} disabled={on === null || busy} onClick={() => void flip()}><span /></button>
    </SettingsRow>
    {problem && <p className="settings-row-note" role="alert">{problem}</p>}
  </SettingsGroup>
}

export type OwnerGrant = { sessionId: string; agentId?: string; endsAt?: string }

/** Live grants, from `owner_computer.grant` events. */
export function useOwnerGrants(): OwnerGrant[] {
  const [grants, setGrants] = useState<OwnerGrant[]>([])
  useEffect(() => {
    let stop: (() => void) | null = null
    let live = true
    void listenForDaemonEvents((event) => {
      if (event.type !== 'owner_computer.grant') return
      const payload = recordFrom(event.payload)
      const sessionId = typeof payload.sessionId === 'string' ? payload.sessionId : ''
      if (!sessionId) return
      setGrants((current) => payload.state === 'live'
        ? [...current.filter((grant) => grant.sessionId !== sessionId), { sessionId, agentId: typeof payload.agentId === 'string' ? payload.agentId : undefined, endsAt: typeof payload.endsAt === 'string' ? payload.endsAt : undefined }]
        : current.filter((grant) => grant.sessionId !== sessionId))
    }).then((unlisten) => { if (live) stop = unlisten; else unlisten() }).catch(() => undefined)
    return () => { live = false; stop?.() }
  }, [])
  return grants
}

function left(endsAt: string | undefined, now: number) {
  const end = endsAt ? Date.parse(endsAt) : NaN
  if (!Number.isFinite(end)) return null
  const seconds = Math.max(0, Math.round((end - now) / 1000))
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

/** The bar: which employee is controlling this computer, the time left, and Stop. */
export function OwnerComputerBar({ grant, name, compact = false }: { grant: OwnerGrant; name: string; compact?: boolean }) {
  const [now, setNow] = useState(() => Date.now())
  const [stopping, setStopping] = useState(false)
  useEffect(() => { const timer = window.setInterval(() => setNow(Date.now()), 1000); return () => window.clearInterval(timer) }, [])
  const remaining = left(grant.endsAt, now)
  return <div className={`owner-computer-bar${compact ? ' compact' : ''}`} role="status">
    <MousePointerClick size={15} aria-hidden="true" />
    <span><strong>{name}</strong> is controlling this computer{remaining ? <small> · {remaining} left</small> : null}</span>
    <button type="button" className="owner-computer-stop" disabled={stopping} onClick={() => { setStopping(true); void rpc('owner_computer.stop', { sessionId: grant.sessionId }).catch(() => undefined).finally(() => setStopping(false)) }}><Square size={11} aria-hidden="true" />Stop</button>
  </div>
}
