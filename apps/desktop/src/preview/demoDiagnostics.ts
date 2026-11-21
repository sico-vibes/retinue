const unknownMethods = new Set<string>()
let methods: readonly string[] = []
const listeners = new Set<() => void>()

export function recordUnknownRpc(method: string) {
  if (unknownMethods.has(method)) return
  unknownMethods.add(method)
  methods = [...unknownMethods].sort()
  console.warn(`Retinue preview: RPC "${method}" is not demoed yet.`)
  for (const listener of listeners) listener()
}

export function unknownRpcMethods() { return methods }

export function subscribeToUnknownRpcs(listener: () => void) {
  listeners.add(listener)
  return () => { listeners.delete(listener) }
}

