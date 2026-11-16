import { useSyncExternalStore } from 'react'
import { getMediaScope } from './MediaPreview'

/** A project file shown in the document panel beside the chat, tied to the conversation it was opened from. */
export type OpenDocument = { path: string; title: string; sessionId: string }

let current: OpenDocument | null = null
let panels = 0
const listeners = new Set<() => void>()
const notify = () => listeners.forEach((listener) => listener())

/** The main window mounts one panel; the companion has none, so its links keep the dialog. */
export function registerDocumentPanel(): () => void {
  panels += 1
  return () => { panels -= 1; if (panels === 0 && current) { current = null; notify() } }
}

/** Opens `path` in the panel. Returns false when there is no panel or no open conversation, so the caller can fall back. */
export function openDocument(path: string, title?: string): boolean {
  const sessionId = getMediaScope()?.sessionId
  if (panels === 0 || !sessionId) return false
  current = { path, title: title?.trim() || path.split(/[\\/]/).pop() || path, sessionId }
  notify()
  return true
}

export function getOpenDocument(): OpenDocument | null { return current }

export function closeDocument() {
  if (!current) return
  current = null
  notify()
}

export function useOpenDocument(): OpenDocument | null {
  return useSyncExternalStore((listener) => { listeners.add(listener); return () => { listeners.delete(listener) } }, () => current)
}
