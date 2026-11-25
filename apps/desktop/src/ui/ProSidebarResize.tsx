// A drag handle on the right edge of the sidebar in Professional mode (owner, 25 November 2026). The width is clamped between
// the narrowest that still fits a row (logo, title, time, status, project and place) and the widest that is still useful,
// is kept in this window's storage, and a double click or Home puts it back.
import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react'

const KEY = 'retinue.pro.sidebarWidth'
export const PRO_SIDEBAR_MIN = 300
export const PRO_SIDEBAR_MAX = 480
const clamp = (value: number) => Math.round(Math.min(PRO_SIDEBAR_MAX, Math.max(PRO_SIDEBAR_MIN, value)))

function stored(): number | null { try { const value = Number(window.localStorage.getItem(KEY)); return Number.isFinite(value) && value > 0 ? clamp(value) : null } catch { return null } }
function apply(width: number | null) { const root = document.documentElement; if (width === null) root.style.removeProperty('--pro-user-w'); else root.style.setProperty('--pro-user-w', `${width}px`) }
function keep(width: number | null) { try { if (width === null) window.localStorage.removeItem(KEY); else window.localStorage.setItem(KEY, String(width)) } catch { /* lasts until the window closes */ } }

export function ProSidebarResize() {
  const [width, setWidth] = useState<number | null>(stored)
  const [dragging, setDragging] = useState(false)
  const drag = useRef<{ x: number; width: number } | null>(null)
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => { apply(width); return () => apply(null) }, [width])
  const current = () => ref.current?.parentElement?.getBoundingClientRect().width ?? PRO_SIDEBAR_MIN
  const set = (next: number | null, persist: boolean) => { setWidth(next); apply(next); if (persist) keep(next) }
  const down = (event: PointerEvent<HTMLDivElement>) => { event.currentTarget.setPointerCapture(event.pointerId); drag.current = { x: event.clientX, width: current() }; setDragging(true) }
  const move = (event: PointerEvent<HTMLDivElement>) => { if (drag.current) set(clamp(drag.current.width + event.clientX - drag.current.x), false) }
  const up = (event: PointerEvent<HTMLDivElement>) => { if (!drag.current) return; drag.current = null; setDragging(false); event.currentTarget.releasePointerCapture(event.pointerId); keep(width ?? clamp(current())) }
  const key = (event: KeyboardEvent<HTMLDivElement>) => {
    const base = width ?? clamp(current())
    if (event.key === 'ArrowLeft') { event.preventDefault(); set(clamp(base - 16), true) }
    else if (event.key === 'ArrowRight') { event.preventDefault(); set(clamp(base + 16), true) }
    else if (event.key === 'Home') { event.preventDefault(); set(null, true) }
  }
  return <div ref={ref} className={`pro-resize${dragging ? ' dragging' : ''}`} role="separator" aria-orientation="vertical" aria-label="Resize the sidebar" aria-valuemin={PRO_SIDEBAR_MIN} aria-valuemax={PRO_SIDEBAR_MAX} aria-valuenow={width ?? undefined} tabIndex={0} title="Drag to resize. Double click to reset."
    onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up} onDoubleClick={() => set(null, true)} onKeyDown={key} />
}
