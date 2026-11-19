// One short line for what a bot is doing right now, from the provider-neutral
// tool record the daemon keeps (`tool_detail` in storage): "Running npm test",
// "Editing App.tsx", "Searching for “retry”". Unknown tools keep their title.

type ToolRecord = Record<string, unknown> & { detail?: Record<string, unknown> | null }

const text = (value: unknown) => (typeof value === 'string' ? value.trim() : '')
const clip = (value: string, limit = 48) => (value.length > limit ? `${value.slice(0, limit - 1)}…` : value)
const baseName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path

function host(url: string) {
  try { return new URL(url).host || url } catch { return url }
}

export function doingNowLabel(tool: ToolRecord): string {
  const detail = tool.detail && typeof tool.detail === 'object' ? tool.detail : {}
  const kind = `${text(tool.kind)} ${text(detail.kind)} ${text(tool.title)}`.toLowerCase()
  const label = text(detail.label)
  if (label.startsWith('Ran ')) return clip(`Running ${label.slice(4)}`)
  const path = text(detail.path)
  const files = typeof detail.fileCount === 'number' && detail.fileCount > 1 ? detail.fileCount : 0
  if (/\b(edit|write|patch|multiedit|create|apply)/.test(kind)) return files ? `Editing ${files} files` : path ? clip(`Editing ${baseName(path)}`) : 'Editing files'
  if (/\b(read|view|open)/.test(kind) && path) return clip(`Reading ${baseName(path)}`)
  const query = text(detail.query) || text(detail.pattern)
  if (/\b(search|grep|glob|find)/.test(kind)) return query ? clip(`Searching for “${clip(query, 32)}”`) : 'Searching'
  const url = text(detail.url)
  if (/\b(fetch|web|browse)/.test(kind) && url) return clip(`Opening ${host(url)}`)
  return clip(text(tool.title) || 'Working')
}

/** What a conversation's running tool is doing, or null when no tool is running (wave 13: assignment cards). */
export function doingNowFor(session: { state?: unknown; tools?: readonly ToolRecord[]; workingNote?: { text?: unknown } | null } | null | undefined): string | null {
  if (!session || !['working', 'starting'].includes(String(session.state ?? ''))) return null
  // The bot's own note (wave 2) says it better than a tool name, so it comes first.
  const note = text(session.workingNote?.text)
  if (note) return clip(note, 96)
  const running = [...(session.tools ?? [])].reverse().find((tool) => ['running', 'pending', 'started'].includes(String(tool.state ?? '').toLowerCase()))
  return running ? doingNowLabel(running) : null
}

/** What kind of step a running tool is, for the working line's icon and motion (owner, 15 November 2026). */
export type DoingKind = 'command' | 'read' | 'edit' | 'search' | 'web' | 'tool'
export function doingNowKind(tool: ToolRecord): DoingKind {
  // Read from the line itself, so the icon always agrees with the words beside it.
  const label = doingNowLabel(tool)
  if (label.startsWith('Running ')) return 'command'
  if (label.startsWith('Reading ')) return 'read'
  if (label.startsWith('Editing ')) return 'edit'
  if (label.startsWith('Searching')) return 'search'
  if (label.startsWith('Opening ')) return 'web'
  const detail = tool.detail && typeof tool.detail === 'object' ? tool.detail : {}
  const kind = `${text(tool.kind)} ${text(detail.kind)}`.toLowerCase()
  if (/(bash|shell|command|exec|execute|terminal)/.test(kind)) return 'command'
  if (/(read|view)/.test(kind)) return 'read'
  return 'tool'
}

/** The running step of a working conversation, as a kind and a line; null while it is thinking. */
export function doingNowStep(session: { state?: unknown; tools?: readonly ToolRecord[] } | null | undefined): { kind: DoingKind; label: string } | null {
  if (!session || !['working', 'starting'].includes(String(session.state ?? ''))) return null
  const running = [...(session.tools ?? [])].reverse().find((tool) => ['running', 'pending', 'started'].includes(String(tool.state ?? '').toLowerCase()))
  return running ? { kind: doingNowKind(running), label: doingNowLabel(running) } : null
}
