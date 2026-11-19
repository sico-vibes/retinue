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
  return <div className="brand-lockup painting">
    <span className="brand-logo-stack">
      <img className="brand-logo theme-on-dark" src={retinueTileDark} alt="Retinue logo" />
      <img className="brand-logo theme-on-light" src={retinueTileLight} alt="" />
      <MetallicPaintOverlay key={light ? 'light' : 'dark'} playing light={light} />
    </span>
    <strong>Retinue</strong>
  </div>
}
