// Puts the chat background picture's palette on the document while Haze matches it (owner, 16 November 2026).
// ui/themes/haze.css turns the --picture-* colours into the mist and base; the accent goes through blob/retinue.ts.
import { useEffect } from 'react'
import { setAccentOverride, useThemeStyle } from '../blob/retinue'
import type { ThemeStyle } from './appearance'
import { useChatBackgroundPrefs, type ChatBackgroundPrefs } from './chatBackgroundStore'

const NAMES = ['d1', 'd2', 'd3', 'd4', 'base'] as const

/** True when Haze should wear the picture's colours. */
export const picturePaletteActive = (style: ThemeStyle, prefs: ChatBackgroundPrefs) => style === 'haze' && prefs.hasImage && prefs.matchColors && !!prefs.palette

export function applyPicturePalette(root: HTMLElement, style: ThemeStyle, prefs: ChatBackgroundPrefs) {
  const palette = picturePaletteActive(style, prefs) ? prefs.palette : null
  for (const name of NAMES) {
    if (palette) root.style.setProperty(`--picture-${name}`, palette[name])
    else root.style.removeProperty(`--picture-${name}`)
  }
  if (palette) root.dataset.picturePalette = 'on'
  else delete root.dataset.picturePalette
  setAccentOverride(palette?.accent ?? '')
}

/** The main window only: the companion and dictation keep their own look. */
export function usePicturePalette(enabled: boolean) {
  const style = useThemeStyle()
  const prefs = useChatBackgroundPrefs()
  useEffect(() => {
    if (!enabled) return
    const root = document.documentElement
    applyPicturePalette(root, style, prefs)
  }, [enabled, style, prefs])
}
