// The only TypeScript module that knows the names this app had before Retinue
// (Bloblex and Bitbots). It moves the browser-cache keys saved under them, once,
// so an existing install keeps its look and cached preferences. The daemon's
// settings stay the source of truth; this only avoids a first-paint flash.
// Import it first in any entry point that reads the cache, and delete it when
// no install older than Retinue is left.

const LEGACY_PREFIXES = ['bitbots.', 'bloblex.']
/** The pre-style flag: '0' was Stone, '1' was Frost. It gets its own new key. */
const PRE_STYLE_LEGACY_KEY = 'bloblex.bitbots'
const PRE_STYLE_KEY = 'retinue.preStyle'
/** Stored look ids that were renamed. Retired ids are normalised by the look module. */
const LEGACY_LOOK_IDS: Record<string, string> = { blobots: 'stone', bitbots: 'frost' }

function relabel(value: string): string {
  if (LEGACY_LOOK_IDS[value] !== undefined) return LEGACY_LOOK_IDS[value]
  return value.replace(/"(blobots|bitbots)"/g, (_match, id: string) => `"${LEGACY_LOOK_IDS[id]}"`)
}

export function migrateLegacyLocalStorage(storage: Storage | undefined = globalThis.localStorage): void {
  if (!storage) return
  try {
    const legacy: string[] = []
    for (let index = 0; index < storage.length; index += 1) {
      const key = storage.key(index)
      if (key && LEGACY_PREFIXES.some((prefix) => key.startsWith(prefix))) legacy.push(key)
    }
    for (const oldKey of legacy) {
      const value = storage.getItem(oldKey)
      const newKey = oldKey === PRE_STYLE_LEGACY_KEY ? PRE_STYLE_KEY : `retinue.${oldKey.slice(oldKey.indexOf('.') + 1)}`
      // A value the new name already has wins; the old one is dropped either way.
      if (value !== null && storage.getItem(newKey) === null) storage.setItem(newKey, relabel(value))
      storage.removeItem(oldKey)
    }
  } catch { /* A blocked or full store keeps the old keys; the app still starts. */ }
}

migrateLegacyLocalStorage()
