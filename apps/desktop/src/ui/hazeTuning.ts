// Haze tuning (owner, 9 November 2026): how see-through, how strong and what colour the misty
// backdrop is, plus how dark the panels sit over it. It stays in this window's own storage, like
// the chat background, and is applied as CSS variables on <html> (ui/themes/haze.css reads them).
import { useSyncExternalStore } from 'react'
import { normalizeAccent } from './appearance'

export type HazeTuning = {
  /** How opaque the backdrop is: 0.4 lets a lot of the desktop through, 1 none. */
  opacity: number
  /** How strong the coloured mist is: 0 is the plain base, 1 the default, 1.6 vivid. */
  strength: number
  /** How dark (or light, in the light theme) the panels sit over the haze: 0 to 0.6. */
  shade: number
  /** The mist's own colour; '' follows the accent. */
  tint: string
}

export const DEFAULT_HAZE: HazeTuning = { opacity: 0.8, strength: 1, shade: 0, tint: '' }
export const HAZE_LIMITS = { opacity: [0.4, 1], strength: [0, 1.6], shade: [0, 0.6] } as const

const KEY = 'retinue.hazeTuning'
const clamp = (value: unknown, [min, max]: readonly [number, number], fallback: number) => typeof value === 'number' && Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback

export function normalizeHazeTuning(value: unknown): HazeTuning {
  const raw = value && typeof value === 'object' ? value as Record<string, unknown> : {}
  return {
    opacity: clamp(raw.opacity, HAZE_LIMITS.opacity, DEFAULT_HAZE.opacity),
    strength: clamp(raw.strength, HAZE_LIMITS.strength, DEFAULT_HAZE.strength),
    shade: clamp(raw.shade, HAZE_LIMITS.shade, DEFAULT_HAZE.shade),
    tint: normalizeAccent(raw.tint),
  }
}

function read(): HazeTuning {
  try { return normalizeHazeTuning(JSON.parse(localStorage.getItem(KEY) ?? '{}')) } catch { return DEFAULT_HAZE }
}

/** Write the tuning onto the document; the defaults leave the stylesheet's own values in place. */
export function applyHazeTuning(root: HTMLElement, tuning: HazeTuning) {
  const set = (name: string, value: string | null) => { if (value === null) root.style.removeProperty(name); else root.style.setProperty(name, value) }
  set('--haze-opacity', tuning.opacity === DEFAULT_HAZE.opacity ? null : String(tuning.opacity))
  set('--haze-strength', tuning.strength === DEFAULT_HAZE.strength ? null : String(tuning.strength))
  set('--haze-shade', tuning.shade ? String(tuning.shade) : null)
  set('--haze-tint', tuning.tint || null)
  if (tuning.tint) root.dataset.hazeTint = 'custom'
  else delete root.dataset.hazeTint
}

let tuning = read()
const listeners = new Set<() => void>()
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener) } }

export function updateHazeTuning(change: Partial<HazeTuning>) {
  tuning = normalizeHazeTuning({ ...tuning, ...change })
  try { localStorage.setItem(KEY, JSON.stringify(tuning)) } catch { /* It lasts until the window closes. */ }
  if (typeof document !== 'undefined') applyHazeTuning(document.documentElement, tuning)
  for (const listener of listeners) listener()
}

export const resetHazeTuning = () => updateHazeTuning(DEFAULT_HAZE)
export const useHazeTuning = () => useSyncExternalStore(subscribe, () => tuning)

if (typeof document !== 'undefined') applyHazeTuning(document.documentElement, tuning)
