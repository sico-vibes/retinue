import { useState } from 'react'
import retinueTileDark from '../assets/brand/retinue-tile-dark.png'
import retinueTileLight from '../assets/brand/retinue-tile-light.png'
import { MetallicPaintOverlay } from './MetallicMark'

/** The logo at the top left: the PNG tile, painted metal while the pointer is over it (owner, 14 November 2026). */
export function BrandLockup() {
  const [hover, setHover] = useState(false)
  return <div className={`brand-lockup${hover ? ' painting' : ''}`} onPointerEnter={() => setHover(true)} onPointerLeave={() => setHover(false)}>
    <span className="brand-logo-stack">
      <img className="brand-logo theme-on-dark" src={retinueTileDark} alt="Retinue logo" />
      <img className="brand-logo theme-on-light" src={retinueTileLight} alt="" />
      <MetallicPaintOverlay playing={hover} light={document.documentElement.dataset.theme === 'light'} />
    </span>
    <strong>Retinue</strong>
  </div>
}
