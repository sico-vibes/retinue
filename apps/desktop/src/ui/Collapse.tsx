// Animated open and close for a list under a folding header (owner, 8 November
// 2026: "a smooth UI when I expand or collapse these things"). The content
// stays mounted only while it is open or closing, so closed trees cost nothing.
// Height animates with grid rows 0fr <-> 1fr; reduced motion is instant.
import { useEffect, useState, type ReactNode } from 'react'
import './collapse.css'

/** Matches the transition in collapse.css. */
export const COLLAPSE_MS = 180

export function Collapse({ open, children, className = '' }: { open: boolean; children: ReactNode; className?: string }) {
  const [mounted, setMounted] = useState(open)
  const [shown, setShown] = useState(open)
  useEffect(() => {
    if (open) {
      setMounted(true)
      // Mount closed first, then open on the next frame so the rows animate.
      const frame = requestAnimationFrame(() => requestAnimationFrame(() => setShown(true)))
      return () => cancelAnimationFrame(frame)
    }
    setShown(false)
    const timer = window.setTimeout(() => setMounted(false), COLLAPSE_MS + 20)
    return () => window.clearTimeout(timer)
  }, [open])
  if (!mounted) return null
  return <div className={`collapse${shown ? ' open' : ''}${className ? ` ${className}` : ''}`} aria-hidden={open ? undefined : true}>
    <div className="collapse-inner">{children}</div>
  </div>
}
