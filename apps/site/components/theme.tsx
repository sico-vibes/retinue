'use client'
import { useSyncExternalStore } from 'react'

export type Theme = 'dark' | 'light'

const read = (): Theme => (typeof document !== 'undefined' && document.documentElement.dataset.theme === 'light' ? 'light' : 'dark')

/** The page's theme, kept current when the toggle changes it. */
function subscribe(onChange: () => void) {
  const observer = new MutationObserver(onChange)
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
  return () => observer.disconnect()
}

export function useTheme(): Theme {
  return useSyncExternalStore(subscribe, read, () => 'dark')
}

export function setTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme
  try { localStorage.setItem('retinue-theme', theme) } catch { /* private mode */ }
}

export function ThemeToggle({ className = '' }: { className?: string }) {
  const theme = useTheme()
  const next = theme === 'dark' ? 'light' : 'dark'
  return <button type="button" onClick={() => setTheme(next)} aria-label={`Switch to ${next} mode`} title={`Switch to ${next} mode`}
    className={`grid size-9 place-items-center rounded-full border border-line text-ink-2 transition hover:border-line-strong hover:text-ink ${className}`}>
    {theme === 'dark'
      ? <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" /></svg>
      : <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z" /></svg>}
  </button>
}

/** An image with a dark and a light version; the page shows the one that matches its theme. */
export function Themed({ dark, light, alt, className = '', ...rest }: { dark: string; light: string; alt: string; className?: string } & Omit<React.ImgHTMLAttributes<HTMLImageElement>, 'src'>) {
  return <>
    <img src={dark} alt={alt} className={`theme-dark ${className}`} {...rest} />
    <img src={light} alt="" aria-hidden="true" className={`theme-light ${className}`} {...rest} />
  </>
}
