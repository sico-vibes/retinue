import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { Agent, Runtime } from '../types'
import { type LaunchCheck } from './launchChecks'
import { friendlyError } from './errorCopy'
import { ProviderLogo } from './providerBrand'
import { PixelClouds } from './PixelClouds'
import markWhite from '../assets/brand/retinue-mark-white.png'
import markBlack from '../assets/brand/retinue-mark-black.png'
import { useLegacyPixel } from '../blob/bitbots'

type Phase = 'scanning' | 'greeting' | 'ready'

/**
 * Startup checks. The mark breathes while Bitbots looks for coding agents,
 * then settles once everything is ready; the checks scroll inside a fixed
 * window whose edges fade into the background.
 */
export function LaunchIntro({ checks, runtimes, agent: _agent = null, onComplete, onRetry, onContinueOffline, serviceDown, autoComplete = true }: {
  checks: LaunchCheck[]; runtimes: Runtime[]; agent?: Agent | null; onComplete: () => void; onRetry: () => void; onContinueOffline: () => void; serviceDown: boolean; autoComplete?: boolean
}) {
  const completed = useRef(false)
  const onCompleteRef = useRef(onComplete)
  const completionTimer = useRef<number | null>(null)
  const finished = checks.length > 0 && checks.every((check) => check.state !== 'running')
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches
  const [phase, setPhase] = useState<Phase>('scanning')
  const bitbots = useLegacyPixel()
  useEffect(() => { onCompleteRef.current = onComplete }, [onComplete])

  const complete = () => {
    if (completed.current) return
    completed.current = true
    onCompleteRef.current()
  }

  useEffect(() => {
    if (!finished || serviceDown) { setPhase('scanning'); return }
    if (!reducedMotion) { setPhase((current) => current === 'scanning' ? 'greeting' : current); return }
    setPhase('ready')
  }, [finished, serviceDown, reducedMotion])

  // Without the greeting (reduced motion), finish shortly after the last check settles.
  useEffect(() => {
    if (!autoComplete || phase !== 'ready' || !reducedMotion || serviceDown || completed.current) return
    const timer = window.setTimeout(() => {
      completionTimer.current = null
      complete()
    }, 220)
    completionTimer.current = timer
    return () => {
      window.clearTimeout(timer)
      if (completionTimer.current === timer) completionTimer.current = null
    }
  }, [autoComplete, phase, reducedMotion, serviceDown])

  const greetingDone = () => {
    setPhase('ready')
    if (autoComplete && !serviceDown) complete()
  }
  const continueOffline = () => {
    if (completionTimer.current !== null) window.clearTimeout(completionTimer.current)
    completionTimer.current = null
    completed.current = true
    onContinueOffline()
  }
  const retry = () => {
    if (completionTimer.current !== null) window.clearTimeout(completionTimer.current)
    completionTimer.current = null
    completed.current = false
    onRetry()
  }

  const names = new Map(runtimes.map((runtime) => [runtime.id, runtime.provider]))
  const running = checks.find((check) => check.state === 'running')
  const status = serviceDown ? 'Bitbots could not connect to its service.' : finished ? 'Ready for your next conversation.' : running?.label ?? 'Getting Bitbots ready…'
  return <main className={`launch-intro${reducedMotion ? ' reduced-motion' : ''}`} data-phase={phase} aria-label="Bitbots launch checks" onPointerDown={() => { if (phase === 'greeting' && autoComplete) complete() }}>
    <PixelClouds className="launch-clouds" />
    <section className="launch-intro-content">
      <div className="launch-blob-stage">
        <LaunchLogo state={serviceDown ? 'error' : phase === 'greeting' ? 'greeting' : phase === 'ready' ? 'ready' : 'working'} onGreeted={greetingDone} />
      </div>
      {bitbots && !reducedMotion && phase === 'scanning' && !serviceDown && <PixelScan />}
      <p className="launch-status" role="status" aria-live="polite">{status}</p>
      <LaunchChecklist checks={checks} names={names} />
      {serviceDown && <div className="launch-actions"><button className="primary-button" onClick={retry}>Retry</button><button className="secondary-button" onClick={continueOffline}>Continue offline</button></div>}
    </section>
  </main>
}

/** Checks scroll inside a fixed window; the one in progress stays in view. */
function LaunchChecklist({ checks, names }: { checks: LaunchCheck[]; names: Map<string, string> }) {
  const windowRef = useRef<HTMLDivElement>(null)
  const focusIndex = Math.max(0, checks.findIndex((check) => check.state === 'running'))
  const focus = checks.some((check) => check.state === 'running') ? focusIndex : checks.length - 1
  useLayoutEffect(() => {
    const box = windowRef.current
    const row = box?.querySelectorAll<HTMLElement>('li')[focus]
    if (!box || !row) return
    const target = Math.max(0, row.offsetTop - (box.clientHeight - row.offsetHeight) / 2)
    if (typeof box.scrollTo === 'function') box.scrollTo({ top: target, behavior: 'smooth' })
    else box.scrollTop = target
  }, [focus, checks.length])
  return <div className="launch-checklist-window" ref={windowRef}>
    <ol className="launch-checklist" aria-label="Startup checks">{checks.map((check) => {
      const provider = check.id.startsWith('auth:') ? check.id.slice(5) : check.id.startsWith('models:') ? check.id.slice(7) : ''
      const providerId = names.get(provider)
      const stateMark = <StateGlyph state={check.state} />
      const detail = check.detail.startsWith('runtime:') ? (names.get(check.detail.slice(8)) ?? 'Coding agent') + ' found' : check.detail.startsWith('models:') ? (names.get(check.detail.slice(7)) ?? 'Coding agent') + ' models unavailable' : check.detail
      return <li key={check.id} data-state={check.state}>
        <span className={`launch-check-icon ${providerId ? 'has-provider' : ''}`} aria-hidden="true">
          {providerId ? <><ProviderLogo provider={providerId} size={16} /><span className="launch-state-indicator">{check.state === 'running' ? <i className="launch-spinner" /> : stateMark}</span></> : check.state === 'running' ? <i className="launch-spinner" /> : <span className="launch-state-mark">{stateMark}</span>}
        </span>
        <span className="launch-check-copy"><strong>{check.label}</strong><small>{detail}</small></span>
      </li>
    })}</ol>
  </div>
}

/**
 * Check, bang and cross drawn on a square grid so they sit at the exact centre
 * of their circle or box, which a text glyph's font metrics never guarantee.
 */
function StateGlyph({ state }: { state: LaunchCheck['state'] }) {
  return <svg className="launch-state-glyph" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
    {state === 'ok' ? <path d="M2.2 5.2 4.2 7.2 7.9 2.8" /> : state === 'warning' ? <path d="M5 2v3.5M5 8v.01" /> : <path d="M2.8 2.8l4.4 4.4M7.2 2.8 2.8 7.2" />}
  </svg>
}

/** Bitbots scan: a row of pixels lights in a short run while Bitbots looks for coding agents. */
function PixelScan() {
  return <div className="launch-pixel-scan" aria-hidden="true">{Array.from({ length: 14 }, (_, index) => <i key={index} style={{ ['--i' as string]: index }} />)}</div>
}

export function LaunchWarnings({ warnings, details, retrying = false, onSettings, onDismiss }: { warnings: string[]; details?: string; retrying?: boolean; onSettings: () => void; onDismiss: () => void }) {
  const [open, setOpen] = useState(false)
  if (!warnings.length && !retrying) return null
  const lines = details
    ? details.split('\n').map((line) => line.replace(/Daemon request failed:[^\n]*/g, 'The local service connection was interrupted.').replace(/^(.+?): \1\b/, '$1').trim()).filter(Boolean)
    : warnings.map((warning) => friendlyError({ message: warning, provider: providerFromWarning(warning) }).title.replace(/[.\s]+$/, ''))
  const many = lines.length > 1
  const summary = lines.join('; ')
  return <aside className="launch-warning-notice" role="status" aria-label={details ? summary : undefined} title={summary}>
    <div className="launch-warning-body">
      <span>{retrying ? 'Finding coding agents again…' : many ? `${lines.length} startup checks need attention.` : 'Startup checks need attention: ' + summary + '.'}</span>
      {many && !retrying && open && <ul className="launch-warning-list">{lines.map((line, index) => <li key={index}>{line}</li>)}</ul>}
    </div>
    {many && !retrying && <button className="secondary-button small" aria-expanded={open} onClick={() => setOpen((value) => !value)}>{open ? 'Hide details' : 'Show details'}</button>}
    <button className="secondary-button small" onClick={onSettings}>Agents settings</button><button className="icon-button small" aria-label="Dismiss agent notice" onClick={onDismiss}>×</button>
  </aside>
}

function providerFromWarning(warning: string) {
  const lower = warning.trim().toLowerCase()
  if (lower.startsWith('claude code:')) return 'claude'
  if (lower.startsWith('codex:')) return 'codex'
  if (lower.startsWith('opencode:')) return 'opencode'
  return undefined
}

/** The app's mark on the loading screen: it fades in for the greeting, breathes
 *  while the checks run and rests when ready. White on dark, ink on light. */
function LaunchLogo({ state, onGreeted }: { state: 'greeting' | 'working' | 'ready' | 'error'; onGreeted: () => void }) {
  const done = useRef(onGreeted)
  done.current = onGreeted
  useEffect(() => {
    if (state !== 'greeting') return
    const timer = window.setTimeout(() => done.current(), 900)
    return () => window.clearTimeout(timer)
  }, [state])
  return <span className={`launch-logo is-${state}`} role="img" aria-label="Retinue">
    <img className="launch-logo-on-dark" src={markWhite} alt="" draggable={false} />
    <img className="launch-logo-on-light" src={markBlack} alt="" draggable={false} />
  </span>
}
