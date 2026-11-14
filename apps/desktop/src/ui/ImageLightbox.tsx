// The one enlarged-image viewer (owner, 14 November 2026): drawn over the whole
// window with everything behind it blurred, and the image can be zoomed (wheel,
// buttons, +/-, double-click) and moved by dragging once it is larger than fits.
import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type WheelEvent as ReactWheelEvent } from 'react'
import { createPortal } from 'react-dom'
import { Minus, Plus, RotateCcw, X } from 'lucide-react'
import './imageLightbox.css'

const MIN = 1
const MAX = 8
const clamp = (value: number) => Math.min(MAX, Math.max(MIN, value))

export function ImageLightbox({ src, name, onClose }: { src: string; name: string; onClose: () => void }) {
  const closeRef = useRef<HTMLButtonElement>(null)
  const stageRef = useRef<HTMLDivElement>(null)
  const [zoom, setZoom] = useState(1)
  const [offset, setOffset] = useState({ x: 0, y: 0 })
  const drag = useRef<{ id: number; x: number; y: number; ox: number; oy: number; moved: boolean } | null>(null)

  const zoomTo = useCallback((next: number, at?: { x: number; y: number }) => {
    setZoom((current) => {
      const target = clamp(next)
      if (target === 1) { setOffset({ x: 0, y: 0 }); return 1 }
      // Keep the point under the cursor still while zooming.
      const stage = stageRef.current?.getBoundingClientRect()
      if (at && stage) {
        const cx = at.x - (stage.left + stage.width / 2)
        const cy = at.y - (stage.top + stage.height / 2)
        setOffset((o) => ({ x: cx - (cx - o.x) * (target / current), y: cy - (cy - o.y) * (target / current) }))
      } else setOffset((o) => ({ x: o.x * (target / current), y: o.y * (target / current) }))
      return target
    })
  }, [])
  const reset = useCallback(() => { setZoom(1); setOffset({ x: 0, y: 0 }) }, [])

  useEffect(() => {
    requestAnimationFrame(() => closeRef.current?.focus())
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); onClose() }
      else if (event.key === '+' || event.key === '=') { event.preventDefault(); zoomTo(zoomRef.current * 1.25) }
      else if (event.key === '-' || event.key === '_') { event.preventDefault(); zoomTo(zoomRef.current / 1.25) }
      else if (event.key === '0') { event.preventDefault(); reset() }
    }
    document.addEventListener('keydown', onKeyDown, true)
    const previous = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    return () => { document.removeEventListener('keydown', onKeyDown, true); document.body.style.overflow = previous }
  }, [onClose, reset, zoomTo])
  const zoomRef = useRef(zoom)
  zoomRef.current = zoom

  const onWheel = (event: ReactWheelEvent) => { event.stopPropagation(); zoomTo(zoom * (event.deltaY < 0 ? 1.15 : 1 / 1.15), { x: event.clientX, y: event.clientY }) }
  const onPointerDown = (event: ReactPointerEvent) => {
    if (event.button !== 0) return
    drag.current = { id: event.pointerId, x: event.clientX, y: event.clientY, ox: offset.x, oy: offset.y, moved: false }
    if (zoom > 1) (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  }
  const onPointerMove = (event: ReactPointerEvent) => {
    const d = drag.current
    if (!d || d.id !== event.pointerId || zoom <= 1) return
    const dx = event.clientX - d.x, dy = event.clientY - d.y
    if (Math.abs(dx) + Math.abs(dy) > 3) d.moved = true
    setOffset({ x: d.ox + dx, y: d.oy + dy })
  }
  const moved = useRef(false)
  const onPointerUp = () => { moved.current = drag.current?.moved ?? false; drag.current = null }
  // A plain click on the backdrop (not the image, not the end of a drag) closes.
  const onBackdropClick = (event: { target: EventTarget; currentTarget: EventTarget }) => { if (event.target === event.currentTarget && !moved.current) onClose(); moved.current = false }

  return createPortal(<div className="image-lightbox message-image-viewer" role="dialog" aria-modal="true" aria-label={`${name}, enlarged`} onClick={onBackdropClick}>
    <div ref={stageRef} className={`image-lightbox-stage${zoom > 1 ? ' zoomed' : ''}`} onClick={onBackdropClick} onWheel={onWheel} onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp} onPointerCancel={() => { drag.current = null }}>
      <img src={src} alt={name} draggable={false} style={{ transform: `translate(${offset.x}px, ${offset.y}px) scale(${zoom})` }} onDoubleClick={(event) => { if (zoom > 1) reset(); else zoomTo(2.5, { x: event.clientX, y: event.clientY }) }} />
    </div>
    <div className="image-lightbox-bar">
      <span className="image-lightbox-name" title={name}>{name}</span>
      <span className="image-lightbox-zoom" role="group" aria-label="Zoom">
        <button type="button" aria-label="Zoom out" title="Zoom out (−)" disabled={zoom <= MIN} onClick={() => zoomTo(zoom / 1.25)}><Minus size={14} aria-hidden="true" /></button>
        <output aria-live="polite">{Math.round(zoom * 100)}%</output>
        <button type="button" aria-label="Zoom in" title="Zoom in (+)" disabled={zoom >= MAX} onClick={() => zoomTo(zoom * 1.25)}><Plus size={14} aria-hidden="true" /></button>
        <button type="button" aria-label="Fit to window" title="Fit (0)" disabled={zoom === 1} onClick={reset}><RotateCcw size={14} aria-hidden="true" /></button>
      </span>
    </div>
    <button ref={closeRef} type="button" className="image-lightbox-close" aria-label="Close image" onClick={onClose}><X size={16} aria-hidden="true" /></button>
  </div>, document.body)
}
