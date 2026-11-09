import { useSyncExternalStore } from 'react'

const query = '(prefers-reduced-motion: reduce)'
const subscribe = (notify: () => void) => {
  const media = matchMedia(query)
  media.addEventListener('change', notify)
  return () => media.removeEventListener('change', notify)
}

/**
 * Whether the visitor prefers reduced motion. It is false for the first render, matching the
 * static HTML, then updates: reading it earlier would make hydration fail for those visitors.
 */
export function useReducedMotionSafe() {
  return useSyncExternalStore(subscribe, () => matchMedia(query).matches, () => false)
}
