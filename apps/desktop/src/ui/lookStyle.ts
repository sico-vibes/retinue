import { useSyncExternalStore } from 'react'

/** The look on `<html data-style>`, kept current while the owner switches looks. */
function subscribe(onChange: () => void) {
  if (typeof MutationObserver === 'undefined' || typeof document === 'undefined') return () => undefined
  const observer = new MutationObserver(onChange)
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-style'] })
  return () => observer.disconnect()
}
const read = () => (typeof document === 'undefined' ? '' : document.documentElement.dataset.style ?? '')

export function useLookStyle(): string {
  return useSyncExternalStore(subscribe, read, () => '')
}
