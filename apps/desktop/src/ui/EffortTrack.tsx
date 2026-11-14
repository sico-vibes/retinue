// One effort slider for every coding agent (owner, 14 November 2026), after
// React Bits' Prompt Bar (MIT + Commons Clause, © David Haz; see
// docs/legal/third-party-notices.md): a filled track with a dot per level and a
// thumb, and rising sparks once the highest level is chosen, in the effort
// panel and behind the composer. The track is a rounded rectangle in Haze and
// a pill in Stone.
import { useEffect, useRef, type KeyboardEvent, type PointerEvent, type RefObject } from 'react'
import { knobInset } from './SessionEffortMeter'

export function EffortTrack({ trackRef, count, position, top, dragging, valueText, disabled, onPointerDown, onPointerMove, onPointerUp, onPointerCancel, onLostPointerCapture, onKeyDown, onKeyUp }: {
  trackRef?: RefObject<HTMLDivElement | null>
  count: number; position: number; top: boolean; dragging: boolean; valueText: string; disabled: boolean
  onPointerDown: (event: PointerEvent<HTMLDivElement>) => void; onPointerMove: (event: PointerEvent<HTMLDivElement>) => void; onPointerUp: (event: PointerEvent<HTMLDivElement>) => void
  onPointerCancel: (event: PointerEvent<HTMLDivElement>) => void; onLostPointerCapture: (event: PointerEvent<HTMLDivElement>) => void
  onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void; onKeyUp: (event: KeyboardEvent<HTMLDivElement>) => void
}) {
  const edge = knobInset('neutral')
  const at = (value: number) => `calc(${edge}px + (100% - ${edge * 2}px) * ${count < 2 ? 0 : value / (count - 1)})`
  return <div ref={trackRef} className={`effort-track${top ? ' max' : ''}${dragging ? ' dragging' : ''}`} role="slider" tabIndex={disabled ? -1 : 0} aria-label="Conversation effort" aria-valuemin={0} aria-valuemax={Math.max(0, count - 1)} aria-valuenow={Math.round(position)} aria-valuetext={valueText} aria-disabled={disabled || undefined}
    style={{ ['--effort-x' as string]: at(position), ['--effort-fill' as string]: Math.round(position) >= count - 1 ? '100%' : `calc(${at(position)} + 7px)` }}
    onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp} onPointerCancel={onPointerCancel} onLostPointerCapture={onLostPointerCapture} onKeyDown={onKeyDown} onKeyUp={onKeyUp}>
    <span className="effort-track-fill" aria-hidden="true" />
    {Array.from({ length: count }, (_, index) => <i key={index} className="effort-track-dot session-effort-tick" style={{ left: at(index) }} aria-hidden="true" />)}
    <span className="effort-track-thumb" aria-hidden="true" />
  </div>
}

type Spark = { x: number; y: number; r: number; vy: number; sway: number; phase: number; life: number; span: number }

/** Rising sparks that fill their parent, drawn while `on`. Nothing runs when motion is reduced. */
export function EffortSparks({ on, className = '' }: { on: boolean; className?: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = canvasRef.current
    if (!on || !canvas || window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    const colour = getComputedStyle(canvas).color || '#b39dff'
    let raf = 0, last = performance.now(), w = 0, h = 0, due = 0, pending = false
    const parts: Spark[] = []
    const resize = () => {
      const rect = canvas.getBoundingClientRect()
      const dpr = Math.min(2, window.devicePixelRatio || 1)
      w = rect.width; h = rect.height
      canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr)
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    }
    const spawn = (burst: boolean) => parts.push({ x: Math.random() * w, y: burst ? h * (0.2 + Math.random() * 0.8) : h + 3, r: 0.9 + Math.random() * 1.1, vy: -(7 + Math.random() * 9), sway: (Math.random() - 0.5) * 10, phase: Math.random() * Math.PI * 2, life: burst ? Math.random() * 1.2 : 0, span: 2.4 + Math.random() * 2.4 })
    const tick = (now: number) => {
      if (pending) { pending = false; resize() }
      const dt = Math.min(0.05, (now - last) / 1000)
      last = now
      due += dt
      const cap = Math.max(10, Math.min(36, Math.round(w / 18)))
      while (due > 0.14) { due -= 0.14; if (parts.length < cap) spawn(false) }
      ctx.clearRect(0, 0, w, h)
      ctx.fillStyle = colour; ctx.shadowColor = colour; ctx.shadowBlur = 6
      for (let i = parts.length - 1; i >= 0; i -= 1) {
        const p = parts[i]!
        p.life += dt
        if (p.life > p.span) { parts.splice(i, 1); continue }
        const k = p.life / p.span
        const twinkle = 0.7 + 0.3 * Math.sin(now / 160 + p.phase)
        p.y += p.vy * dt
        if (p.y < -4) { p.y = h + 3; p.x = Math.random() * w }
        const fade = Math.min(1, Math.max(0, p.y / 14), Math.max(0, (h - p.y) / 14))
        ctx.globalAlpha = Math.min(1, Math.sin(k * Math.PI) * 0.9 * twinkle) * fade
        ctx.beginPath()
        ctx.arc(p.x + Math.sin(now / 900 + p.phase) * p.sway, p.y, p.r * twinkle, 0, Math.PI * 2)
        ctx.fill()
      }
      raf = requestAnimationFrame(tick)
    }
    resize()
    for (let i = 0; i < 18; i += 1) spawn(true)
    const observer = new ResizeObserver(() => { pending = true })
    observer.observe(canvas)
    raf = requestAnimationFrame(tick)
    return () => { cancelAnimationFrame(raf); observer.disconnect(); ctx.clearRect(0, 0, w, h) }
  }, [on])
  return <canvas ref={canvasRef} className={`effort-sparks${on ? ' on' : ''} ${className}`} aria-hidden="true" />
}
