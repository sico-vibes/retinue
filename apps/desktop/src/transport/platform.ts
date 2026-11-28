// What the current shell can do. The React UI reads these flags instead of
// branching on `inDesktop`, so the same components run in the desktop app, the
// browser preview and (later) the remote client with a different transport.
import { useSyncExternalStore } from 'react'
import { isNative } from './nativeBridge'
import { previewMode } from '../tauri'

export type PlatformKind = 'desktop' | 'preview' | 'browser'
/** The operating system the desktop shell runs on. Copy that names the system (the secret store, the file manager, startup) reads it. */
export type HostOs = 'windows' | 'linux' | 'other'

export interface PlatformCapabilities {
  kind: PlatformKind
  os: HostOs
  /** Floating companion window. */
  companion: boolean
  tray: boolean
  /** Open/save dialogs, folder picker. */
  nativeDialogs: boolean
  /** cpal capture and sherpa dictation. */
  nativeSpeech: boolean
  autostart: boolean
  notifications: 'native' | 'web' | 'none'
  /** Open in editor, reveal in explorer. */
  editor: boolean
  updater: boolean
  fileDrop: 'paths' | 'files' | 'none'
  keepAwake: boolean
}

/** The Tauri desktop app: everything is available. */
export const DESKTOP_PLATFORM: PlatformCapabilities = {
  kind: 'desktop',
  os: 'windows',
  companion: true,
  tray: true,
  nativeDialogs: true,
  nativeSpeech: true,
  autostart: true,
  notifications: 'native',
  editor: true,
  updater: true,
  fileDrop: 'paths',
  keepAwake: true,
}

/** The browser preview: behaves like desktop so previews do not change. */
export const PREVIEW_PLATFORM: PlatformCapabilities = { ...DESKTOP_PLATFORM, kind: 'preview' }

/** A plain browser build with no native features; used by the remote client. */
export const BROWSER_PLATFORM: PlatformCapabilities = {
  kind: 'browser',
  os: 'other',
  companion: false,
  tray: false,
  nativeDialogs: false,
  nativeSpeech: false,
  autostart: false,
  notifications: 'web',
  editor: false,
  updater: false,
  fileDrop: 'files',
  keepAwake: false,
}

/**
 * Detected once at module load: inside Tauri the desktop capabilities apply;
 * under tests or the fixture-aliased preview (`previewMode`) the preview
 * capabilities apply; otherwise a plain browser.
 */
function detectPlatform(): PlatformCapabilities {
  if (isNative()) return { ...DESKTOP_PLATFORM, os: osFromUserAgent(globalThis.navigator?.userAgent ?? '') }
  // The preview stands in for Windows unless the demo menu asks for Linux (`?demo-os=linux`).
  if (import.meta.env.MODE === 'test' || previewMode) return { ...PREVIEW_PLATFORM, os: previewOs() }
  return BROWSER_PLATFORM
}

/** WebView2 says "Windows NT"; WebKitGTK says "X11; Linux" (under Wayland too). Android is never the desktop shell. */
export function osFromUserAgent(userAgent: string): HostOs {
  if (/Windows/i.test(userAgent)) return 'windows'
  if (/Linux/i.test(userAgent) && !/Android/i.test(userAgent)) return 'linux'
  return 'other'
}

function previewOs(): HostOs {
  try { return new URLSearchParams(globalThis.location?.search ?? '').get('demo-os') === 'linux' ? 'linux' : 'windows' } catch { return 'windows' }
}

let current: PlatformCapabilities = detectPlatform()
const listeners = new Set<() => void>()

/** The capabilities in force right now. */
export function platform(): PlatformCapabilities {
  return current
}

/** Replaces the capabilities; the remote client calls this at start-up. */
export function setPlatform(next: Partial<PlatformCapabilities> & { kind: PlatformKind }): void {
  current = { ...current, ...next }
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => { listeners.delete(listener) }
}

/** React binding for `platform()`. */
export function usePlatform(): PlatformCapabilities {
  return useSyncExternalStore(subscribe, platform, platform)
}
