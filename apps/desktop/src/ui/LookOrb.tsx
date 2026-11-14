import { useId } from 'react'
import type { ThemeStyle } from './appearance'
import './lookOrb.css'

/** A static, decorative material swatch for one of the app's looks. */
export function LookOrb({ look, size }: { look: ThemeStyle; size: number }) {
  const clipId = `look-orb-${useId().replace(/:/g, '')}`
  return <svg className="look-orb" data-look={look} aria-hidden="true" focusable="false" width={size} height={size} viewBox="0 0 100 100">
    {look === 'haze' && <>
      <defs><radialGradient id={`${clipId}-haze`} cx="30%" cy="24%" r="85%"><stop stopColor="#8a8f96" /><stop offset=".5" stopColor="#3b3d42" /><stop offset="1" stopColor="#1b1c1f" /></radialGradient><linearGradient id={`${clipId}-haze-wave`} x1="0" x2="1"><stop style={{ stopColor: 'var(--user-accent, #c7ccd3)' }} stopOpacity="0" /><stop offset=".55" style={{ stopColor: 'var(--user-accent, #c7ccd3)' }} stopOpacity=".55" /><stop offset="1" style={{ stopColor: 'var(--user-accent, #e4e7ea)' }} stopOpacity=".9" /></linearGradient><clipPath id={`${clipId}-haze-clip`}><circle cx="50" cy="50" r="47" /></clipPath></defs>
      <circle cx="50" cy="50" r="47" fill={`url(#${clipId}-haze)`} />
      <g clipPath={`url(#${clipId}-haze-clip)`}><rect x="10" y="38" width="90" height="24" rx="10" fill={`url(#${clipId}-haze-wave)`} /></g>
      <circle cx="50" cy="50" r="47" fill="none" stroke="#fff" strokeOpacity=".28" strokeWidth="1.5" />
    </>}
    {look === 'stone' && <>
      <defs><radialGradient id={`${clipId}-stone`} cx="31%" cy="25%" r="78%"><stop stopColor="#777" /><stop offset=".52" stopColor="#505050" /><stop offset="1" stopColor="#292929" /></radialGradient><linearGradient id={`${clipId}-stone-band`} x1="0" x2="1"><stop stopColor="#bdbdbd" stopOpacity="0" /><stop offset=".55" stopColor="#bdbdbd" stopOpacity=".38" /><stop offset="1" stopColor="#ececec" stopOpacity=".75" /></linearGradient><clipPath id={`${clipId}-stone-clip`}><circle cx="50" cy="50" r="47" /></clipPath></defs>
      <circle cx="50" cy="50" r="48" fill={`url(#${clipId}-stone)`} />
      <g clipPath={`url(#${clipId}-stone-clip)`}><rect x="10" y="38" width="90" height="24" rx="10" fill={`url(#${clipId}-stone-band)`} /></g>
      <circle cx="50" cy="50" r="47.25" fill="none" stroke="#8b8b8b" strokeOpacity=".42" strokeWidth="1.5" />
    </>}

  </svg>
}

/** The same material as a wide rounded rectangle, for the full-width look cards in Settings (owner, 14 November 2026). */
export function LookSwatch({ look }: { look: ThemeStyle }) {
  const id = `look-swatch-${useId().replace(/:/g, '')}`
  const haze = look === 'haze'
  const stops = haze ? ['#8a8f96', '#3b3d42', '#1b1c1f'] : ['#777', '#505050', '#292929']
  const band = haze ? 'var(--user-accent, #c7ccd3)' : '#bdbdbd'
  return <svg className="look-swatch" data-look={look} aria-hidden="true" focusable="false" viewBox="0 0 240 96" preserveAspectRatio="none">
    <defs>
      <radialGradient id={`${id}-fill`} cx="22%" cy="18%" r="95%"><stop stopColor={stops[0]} /><stop offset=".5" stopColor={stops[1]} /><stop offset="1" stopColor={stops[2]} /></radialGradient>
      <linearGradient id={`${id}-band`} x1="0" x2="1"><stop style={{ stopColor: band }} stopOpacity="0" /><stop offset=".55" style={{ stopColor: band }} stopOpacity={haze ? '.55' : '.38'} /><stop offset="1" style={{ stopColor: band }} stopOpacity={haze ? '.9' : '.75'} /></linearGradient>
      <clipPath id={`${id}-clip`}><rect x="1" y="1" width="238" height="94" rx="14" /></clipPath>
    </defs>
    <rect x="1" y="1" width="238" height="94" rx="14" fill={`url(#${id}-fill)`} />
    <g clipPath={`url(#${id}-clip)`}><rect x="40" y="40" width="200" height="16" rx="8" fill={`url(#${id}-band)`} /></g>
    <rect x="1" y="1" width="238" height="94" rx="14" fill="none" stroke={haze ? '#fff' : '#8b8b8b'} strokeOpacity={haze ? '.22' : '.4'} strokeWidth="1.2" vectorEffect="non-scaling-stroke" />
  </svg>
}
