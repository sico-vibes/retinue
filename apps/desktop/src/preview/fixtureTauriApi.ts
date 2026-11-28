// Development-only stand-ins for @tauri-apps/api/event and /window.
const eventListeners = new Map<string, Set<(event: { payload: unknown }) => void>>()
export async function listen<T>(name: string, handler: (event: { payload: T }) => void) {
  const listeners = eventListeners.get(name) ?? new Set()
  listeners.add(handler as (event: { payload: unknown }) => void)
  eventListeners.set(name, listeners)
  return () => listeners.delete(handler as (event: { payload: unknown }) => void)
}
export async function emit(name: string, payload?: unknown) {
  for (const handler of eventListeners.get(name) ?? []) handler({ payload })
}
export function getCurrentWindow() {
  // The drawn Linux title bar's buttons do nothing in the preview; there is no window to move.
  return {
    onDragDropEvent: async () => () => undefined, requestUserAttention: async () => undefined,
    startDragging: async () => undefined, minimize: async () => undefined, toggleMaximize: async () => undefined,
    close: async () => undefined, isMaximized: async () => false, onResized: async () => () => undefined,
  }
}
