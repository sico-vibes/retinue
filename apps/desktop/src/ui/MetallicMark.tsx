// Retinue's mark in React Bits' Metallic Paint (owner, 14 November 2026): it
// plays on the launch screen and, always on, over the logo at the top left. The
// plain PNG stays underneath, so with no WebGL 2 or with reduced motion the mark
// is simply the picture.
import { lazy, Suspense, useMemo } from 'react'
import markWhite from '../assets/brand/retinue-mark-white.png'
import markBlack from '../assets/brand/retinue-mark-black.png'
import './metallicMark.css'

const MetallicPaint = lazy(() => import('./reactbits/MetallicPaint'))

// The WebGL 2 probe runs once per window and gives its context straight back (15 November 2026): it used to
// create a fresh context on every call and keep it, and the launch screen calls this on every render, so a
// window ran out of WebGL contexts and Chromium replaced the canvases with sad-face squares.
let webgl2: boolean | null = null
function hasWebgl2() {
  if (webgl2 !== null) return webgl2
  try {
    const gl = document.createElement('canvas').getContext('webgl2')
    webgl2 = !!gl && !gl.isContextLost()
    gl?.getExtension('WEBGL_lose_context')?.loseContext()
  } catch { webgl2 = false }
  return webgl2
}

export function canPaintMetal() {
  try { return !window.matchMedia('(prefers-reduced-motion: reduce)').matches && hasWebgl2() } catch { return false }
}

export function MetallicMark({ size, playing, intro = false, className = '', light = false }: { size: number; playing: boolean; intro?: boolean; className?: string; light?: boolean }) {
  const able = useMemo(canPaintMetal, [])
  return <span className={`metallic-mark${playing && able ? ' playing' : ''} ${className}`} style={{ width: size, height: size }} aria-hidden="true">
    <img className="metallic-mark-png on-dark" src={markWhite} alt="" draggable={false} />
    <img className="metallic-mark-png on-light" src={markBlack} alt="" draggable={false} />
    {able && playing && <Suspense fallback={null}><span className="metallic-mark-paint"><MetallicPaint imageSrc={light ? markBlack : markWhite} intro={intro} lightMode={light} /></span></Suspense>}
  </span>
}

/** Only the paint, laid over a picture its parent already shows (the top-left logo). */
export function MetallicPaintOverlay({ playing, light = false, onUnavailable }: { playing: boolean; light?: boolean; onUnavailable?: () => void }) {
  const able = useMemo(canPaintMetal, [])
  if (!able || !playing) return null
  return <Suspense fallback={null}><span className="metallic-overlay" aria-hidden="true"><MetallicPaint imageSrc={light ? markBlack : markWhite} lightMode={light} onUnavailable={onUnavailable} /></span></Suspense>
}
