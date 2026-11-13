// The computer's screen, inside the app. The owner watches by default; nothing they do reaches
// the computer until they choose Take control. The picture comes through the daemon on this PC
// (`computer.view` hands back a loopback address), so the window never holds the server's token.
// Where the daemon cannot offer that, the screen opens in the owner's browser instead.
import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { ExternalLink, Eye, Monitor, MousePointerClick, Sun, X } from 'lucide-react'
import { rpc } from '../tauri'
import { useDialogAccessibility, useDialogEntered } from './dialogFocus'
import { openSafeLink } from './openSafeLink'
import { stateWord, type ComputerStatus } from './computer'
import './computer.css'

const LOOPBACK = ['127.0.0.1', 'localhost', '[::1]', '::1']

/** Only an address on this PC is ever framed. Anything else the daemon might say is dropped. */
export function parseComputerViewAddress(reply: unknown): { url: string | null; reason: string | null } {
  const row = (reply ?? {}) as Record<string, unknown>
  const reason = typeof row.reason === 'string' && row.reason.trim() ? row.reason.trim() : null
  if (typeof row.url !== 'string') return { url: null, reason }
  try {
    const url = new URL(row.url)
    if ((url.protocol === 'http:' || url.protocol === 'https:') && LOOPBACK.includes(url.hostname)) return { url: row.url, reason: null }
  } catch { /* not an address */ }
  return { url: null, reason }
}

export function ComputerView({ status, onClose, onWake, waking = false }: { status: ComputerStatus; onClose: () => void; onWake?: () => void; waking?: boolean }) {
  const entered = useDialogEntered()
  const { ref, close } = useDialogAccessibility(onClose)
  const [view, setView] = useState<{ url: string | null; reason: string | null } | null>(null)
  const [control, setControl] = useState(false)
  const awake = status.reachable && status.state === 'awake'
  const name = status.name ?? 'Computer'
  useEffect(() => {
    setView(null); setControl(false)
    if (!awake) return
    let live = true
    rpc<unknown>('computer.view', {}).then((reply) => { if (live) setView(parseComputerViewAddress(reply)) }).catch(() => { if (live) setView({ url: null, reason: null }) })
    return () => { live = false }
  }, [awake])
  const browser = awake && status.desktopUrl ? <button type="button" className="secondary-button small" onClick={() => void openSafeLink(status.desktopUrl!)}><ExternalLink size={13} aria-hidden="true" />Open in your browser</button> : null
  return createPortal(<div className={`sheet-backdrop computer-view-backdrop ${entered ? 'is-entered' : ''}`}>
    <section ref={ref} className="computer-view" role="dialog" aria-modal="true" aria-label={`${name}'s screen`} tabIndex={-1} data-state={status.reachable ? status.state : 'unreachable'}>
      <header className="computer-view-head">
        <Monitor size={15} aria-hidden="true" />
        <strong>{name}</strong>
        <span className="computer-state">{stateWord(status)}</span>
        <span className="computer-view-spacer" />
        {view?.url && browser}
        <button type="button" className="icon-button tiny" aria-label="Close" data-dialog-initial-focus="" onClick={close}><X size={15} /></button>
      </header>
      <div className="computer-view-stage">
        {awake && view?.url && <>
          <iframe className="computer-view-frame" title={`${name}'s screen`} src={view.url} referrerPolicy="no-referrer" sandbox="allow-scripts allow-same-origin allow-forms" tabIndex={control ? 0 : -1} />
          {/* While watching, this sits over the picture so no click, scroll or key reaches the computer. */}
          {!control && <div className="computer-view-shield" aria-hidden="true" />}
        </>}
        {awake && view === null && <p className="computer-view-plain" role="status">Opening the screen…</p>}
        {awake && view && !view.url && <div className="computer-view-plain">
          <p>{view.reason ?? 'The screen cannot be shown inside Retinue yet.'}</p>
          {browser}
        </div>}
        {!awake && <div className="computer-view-plain">
          <p>{!status.reachable ? status.problem ?? 'Your server did not answer.' : status.state === 'asleep' ? `${name} is asleep. Its files are kept.` : 'There is no screen to show yet.'}</p>
          {status.reachable && status.state === 'asleep' && onWake && <button type="button" className="primary-button small" disabled={waking} onClick={onWake}><Sun size={13} aria-hidden="true" />{waking ? 'Waking…' : 'Wake'}</button>}
        </div>}
      </div>
      {awake && view?.url && <footer className="computer-view-foot">
        <div className="computer-view-modes" role="radiogroup" aria-label="Who is using the computer">
          <button type="button" role="radio" aria-checked={!control} className={control ? '' : 'on'} onClick={() => setControl(false)}><Eye size={13} aria-hidden="true" />Watching</button>
          <button type="button" role="radio" aria-checked={control} className={control ? 'on' : ''} onClick={() => setControl(true)}><MousePointerClick size={13} aria-hidden="true" />Take control</button>
        </div>
        <small>{control ? 'Your mouse and keyboard now reach the computer. An employee using it at the same time will be interrupted.' : 'Employees use it; you are only watching.'}</small>
      </footer>}
    </section>
  </div>, document.body)
}
