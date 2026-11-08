// A single line of text that ends in an ellipsis while it is at rest and,
// when `active` (a selected row) or hovered inside a `.marquee-host`, slowly
// slides left to show the rest, pauses, and returns (owner, 8 November 2026).
// Only text that overflows moves. Reduced motion keeps the ellipsis.
import { useLayoutEffect, useRef, useState, type CSSProperties } from 'react'
import './marquee.css'

/** Pixels per second the text travels. */
const SPEED = 28
/** Seconds of pause at each end, spread over the cycle in the keyframes. */
const PAUSES = 3.2

export function marqueeTiming(shift: number): number {
  return Math.round((PAUSES + (2 * shift) / SPEED) * 10) / 10
}

export function Marquee({ text, active = false, className = '' }: { text: string; active?: boolean; className?: string }) {
  const outer = useRef<HTMLSpanElement>(null)
  const inner = useRef<HTMLSpanElement>(null)
  const [shift, setShift] = useState(0)
  useLayoutEffect(() => {
    const box = outer.current
    const content = inner.current
    if (!box || !content) return
    const measure = () => setShift(Math.max(0, Math.ceil(content.scrollWidth - box.clientWidth)))
    measure()
    if (typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(measure)
    observer.observe(box)
    return () => observer.disconnect()
  }, [text])
  const moving = shift > 1
  const style = moving ? ({ '--marquee-shift': `-${shift}px`, '--marquee-time': `${marqueeTiming(shift)}s` } as CSSProperties) : undefined
  return <span ref={outer} className={`marquee${moving ? ' overflowing' : ''}${active ? ' active' : ''}${className ? ` ${className}` : ''}`} title={moving ? text : undefined} style={style}>
    <span ref={inner} className="marquee-text">{text}</span>
  </span>
}
