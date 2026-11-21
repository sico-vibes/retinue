// Phase 6: this phone's notifications. They say only "Rae needs you", and they travel through the
// phone maker's push service, which is why the owner turns them on at the computer first.
import { useEffect, useState } from 'react'
import { Bell, BellOff } from 'lucide-react'
import { rpc } from '../tauri'
import { pushSupported, pushPermission, requestPushPermission, hasPushSubscription, subscribeToPush, unsubscribeFromPush, keyBytes } from './pushBrowser'
export { keyBytes } from './pushBrowser'

type PushState = 'loading' | 'unsupported' | 'off-at-computer' | 'blocked' | 'off' | 'on'

export function PushCard() {
  const [state, setState] = useState<PushState>('loading')
  const [publicKey, setPublicKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  useEffect(() => {
    let live = true
    void (async () => {
      if (!pushSupported()) { if (live) setState('unsupported'); return }
      const reply = await rpc<{ publicKey?: string; enabled?: boolean }>('push.key', {}).catch(() => null)
      if (!live) return
      if (!reply?.enabled || !reply.publicKey) { setState('off-at-computer'); return }
      setPublicKey(reply.publicKey)
      const existing = await hasPushSubscription()
      if (!live) return
      setState(pushPermission() === 'denied' ? 'blocked' : existing ? 'on' : 'off')
    })()
    return () => { live = false }
  }, [])
  const turnOn = async () => {
    setBusy(true); setProblem(null)
    try {
      const permission = await requestPushPermission()
      if (permission !== 'granted') { setState(permission === 'denied' ? 'blocked' : 'off'); return }
      await rpc('push.subscribe', await subscribeToPush(keyBytes(publicKey)))
      setState('on')
    } catch { setProblem('Notifications could not be turned on here.') } finally { setBusy(false) }
  }
  const turnOff = async () => {
    setBusy(true); setProblem(null)
    try {
      await unsubscribeFromPush()
      await rpc('push.unsubscribe', {})
      setState('off')
    } catch { setProblem('Notifications could not be turned off here.') } finally { setBusy(false) }
  }
  if (state === 'loading') return null
  const copy: Record<Exclude<PushState, 'loading'>, string> = {
    unsupported: 'This browser can\'t show notifications. On iPhone, add Retinue to your Home Screen first.',
    'off-at-computer': 'Turn them on first at the computer, in Settings › Remote.',
    blocked: 'Notifications are blocked for this site. Allow them in your browser\'s settings.',
    off: 'Get a notice when an employee needs you. It says only the employee\'s name, never what it\'s doing.',
    on: 'On. A notice says only the employee\'s name, never what it\'s doing.',
  }
  return <div className="rm-card">
    <span className="rm-card-icon">{state === 'on' ? <Bell size={18} aria-hidden="true" /> : <BellOff size={18} aria-hidden="true" />}</span>
    <div>
      <strong>Notifications</strong>
      <small>{copy[state]}</small>
      {problem && <small role="alert">{problem}</small>}
      {state === 'off' && <button type="button" className="rm-text-button" disabled={busy} onClick={() => void turnOn()}>Turn on</button>}
      {state === 'on' && <button type="button" className="rm-text-button" disabled={busy} onClick={() => void turnOff()}>Turn off</button>}
    </div>
  </div>
}
