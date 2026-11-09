'use client'

import { useSyncExternalStore } from 'react'

type Theme = 'dark' | 'light'

const subscribe = (notify: () => void) => {
  const observer = new MutationObserver(notify)
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
  return () => observer.disconnect()
}
const read = (): Theme => (document.documentElement.dataset.theme === 'light' ? 'light' : 'dark')

export function ThemeToggle() {
  const theme = useSyncExternalStore<Theme>(subscribe, read, () => 'dark')
  const next: Theme = theme === 'dark' ? 'light' : 'dark'
  return (
    <button
      type="button"
      aria-label={`Switch to ${next} theme`}
      onClick={() => {
        document.documentElement.dataset.theme = next
        try { localStorage.setItem('retinue-theme', next) } catch { /* A private window may refuse; the choice just won't persist. */ }
      }}
      className="grid h-11 w-11 place-items-center rounded-lg text-ink-2 transition-colors duration-150 hover:text-ink"
    >
      <svg viewBox="0 0 18 18" width="17" height="17" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
        {theme === 'dark'
          ? <><circle cx="9" cy="9" r="3.4" /><path d="M9 1.6v1.8M9 14.6v1.8M1.6 9h1.8M14.6 9h1.8M3.8 3.8l1.3 1.3M12.9 12.9l1.3 1.3M3.8 14.2l1.3-1.3M12.9 5.1l1.3-1.3" /></>
          : <path d="M15.2 10.8A6.6 6.6 0 0 1 7.2 2.8a6.6 6.6 0 1 0 8 8Z" />}
      </svg>
    </button>
  )
}
