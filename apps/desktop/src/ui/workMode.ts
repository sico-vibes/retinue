// Professional mode (owner, 24 November 2026): the same conversations and projects without the employee layer, as a flat,
// T3 Code / Synara style list with provider logos. It is a view over the same data: nothing is moved, renamed or turned
// off, so switching back loses nothing. In the installed app since 25 November 2026 (owner): Settings > Appearance toggles it,
// Employees by default. `?mode=professional` forces it in the preview only.
import { useSyncExternalStore } from 'react'
import { previewMode } from '../tauri'

export type WorkMode = 'employees' | 'professional'
const KEY = 'retinue.workMode'
const listeners = new Set<() => void>()

/** Only the browser preview takes the mode from the URL (a test double of the shell may not define the flag at all). */
const inPreview = () => { try { return (previewMode as boolean) === true } catch { return false } }

export function currentWorkMode(): WorkMode {
  try {
    const forced = inPreview() ? new URLSearchParams(window.location.search).get('mode') : null
    if (forced === 'professional' || forced === 'employees') return forced
    return window.localStorage.getItem(KEY) === 'professional' ? 'professional' : 'employees'
  } catch { return 'employees' }
}
export function applyWorkMode(root: HTMLElement = document.documentElement) { root.dataset.mode = currentWorkMode() }
export function setWorkMode(mode: WorkMode) {
  try { window.localStorage.setItem(KEY, mode) } catch { /* the choice lasts until the window closes */ }
  applyWorkMode()
  for (const listener of listeners) listener()
}
export function useWorkMode(): WorkMode {
  return useSyncExternalStore((listener) => { listeners.add(listener); return () => { listeners.delete(listener) } }, currentWorkMode, () => 'employees')
}
