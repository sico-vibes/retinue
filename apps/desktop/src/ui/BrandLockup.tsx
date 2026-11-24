import { useEffect, useState } from 'react'
import retinueTileDark from '../assets/brand/retinue-tile-dark.png'
import retinueTileLight from '../assets/brand/retinue-tile-light.png'
import { MetallicPaintOverlay } from './MetallicMark'

/** The logo at the top left: the PNG tile with the metal paint always on (owner, 15 November 2026; it played only on hover before). */
/** Follows `<html data-theme>`, so a theme switch repaints the metal in the right ink without a hover or a reload. */
function useLightTheme() {
  const read = () => document.documentElement.dataset.theme === 'light'
  const [light, setLight] = useState(read)
  useEffect(() => {
    const observer = new MutationObserver(() => setLight(read()))
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
    return () => observer.disconnect()
  }, [])
  return light
}

export function BrandLockup() {
  const light = useLightTheme()
  // The PNG tile stays hidden only while the paint can draw; a lost or missing WebGL canvas gives the tile back (22 November 2026: Chromium's sad-face square replaced the logo).
  const [failed, setFailed] = useState(false)
  return <div className={`brand-lockup${failed ? '' : ' painting'}`}>
    <span className="brand-logo-stack">
      <img className="brand-logo theme-on-dark" src={retinueTileDark} alt="Retinue logo" />
      <img className="brand-logo theme-on-light" src={retinueTileLight} alt="" />
      {!failed && <MetallicPaintOverlay key={light ? 'light' : 'dark'} playing light={light} onUnavailable={() => setFailed(true)} />}
    </span>
    <strong>Retinue</strong>
  </div>
}
