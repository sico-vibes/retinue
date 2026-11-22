// @vitest-environment happy-dom
import { act, type ReactNode } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const rpc = vi.fn()
vi.mock('../tauri', () => ({
  rpc: (method: string, params: unknown) => rpc(method, params),
  listenForDaemonEvents: async () => () => undefined,
}))

const { ExternalClients, normalizeExternalClients, joinNames } = await import('./ExternalClients')

let root: Root | null = null
let host: HTMLDivElement | null = null
afterEach(() => { if (root) act(() => root!.unmount()); root = null; host?.remove(); host = null; rpc.mockReset() })

async function mount(node: ReactNode) {
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  await act(async () => { root!.render(node) })
  await settle()
  return host
}
const settle = () => act(async () => { for (let i = 0; i < 6; i += 1) await Promise.resolve() })
const button = (view: HTMLElement, label: string) => view.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)
const buttonText = (view: HTMLElement, text: string) => [...view.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent === text)

type Row = { id: string; name: string; installed: boolean; state: string; configPath: string | null; addedByRetinue: boolean; error: string | null }
const rows = (): Row[] => [
  { id: 'claude-code', name: 'Claude Code', installed: true, state: 'connected', configPath: 'C:/Users/you/.claude.json', addedByRetinue: true, error: null },
  { id: 'codex', name: 'Codex', installed: true, state: 'not_connected', configPath: 'C:/Users/you/.codex/config.toml', addedByRetinue: false, error: null },
  { id: 'opencode', name: 'OpenCode', installed: true, state: 'not_connected', configPath: null, addedByRetinue: false, error: null },
  { id: 'cursor', name: 'Cursor', installed: true, state: 'different', configPath: 'C:/Users/you/.cursor/mcp.json', addedByRetinue: false, error: null },
  { id: 'qwen', name: 'Qwen Code', installed: false, state: 'not_connected', configPath: null, addedByRetinue: false, error: null },
]

/** A small daemon: connect and disconnect change the stored rows, as the real one does. */
function daemon(list = rows()) {
  rpc.mockImplementation(async (method: string, params: Record<string, unknown>) => {
    const row = list.find((item) => item.id === params?.id)
    if (method === 'external.clients.list') return { clients: list.map((item) => ({ ...item })) }
    if (method === 'external.clients.connect') { Object.assign(row!, { state: 'connected', addedByRetinue: true }); return { client: { ...row } } }
    if (method === 'external.clients.disconnect') { Object.assign(row!, { state: 'not_connected', addedByRetinue: false }); return { client: { ...row } } }
    throw new Error(`unexpected ${method}`)
  })
  return list
}

describe('Connect coding agents', () => {
  it('reads only well-formed rows and joins names for a sentence', () => {
    expect(normalizeExternalClients(null)).toEqual([])
    expect(normalizeExternalClients({ clients: [{ id: 'codex' }, null, { id: 'codex', name: 'Codex', installed: true, state: 'odd' }] }))
      .toEqual([{ id: 'codex', name: 'Codex', installed: true, state: 'not_connected', configPath: null, addedByRetinue: false, error: null }])
    expect(joinNames(['Codex'])).toBe('Codex')
    expect(joinNames(['Codex', 'Cursor'])).toBe('Codex and Cursor')
    expect(joinNames(['Codex', 'Cursor', 'Qwen Code'])).toBe('Codex, Cursor and Qwen Code')
  })

  it('lists the installed agents with the right action each, and names the missing ones', async () => {
    daemon()
    const view = await mount(<ExternalClients enabled onNotice={() => undefined} onProblem={() => undefined} />)
    expect([...view.querySelectorAll('.mcp-client strong')].map((item) => item.textContent)).toEqual(['Claude Code', 'Codex', 'OpenCode', 'Cursor'])
    expect(button(view, 'Disconnect Claude Code')).not.toBeNull()
    expect(button(view, 'Connect Codex')).not.toBeNull()
    // A hand-made entry is never offered for removal or overwriting.
    const cursor = view.querySelector('.mcp-client[data-state="different"]')!
    expect(cursor.querySelector('button')).toBeNull()
    expect(cursor.textContent).toContain('different “retinue” entry')
    expect(view.textContent).toContain('Not found on this computer')
    expect(view.textContent).toContain('Qwen Code')
    expect(view.textContent).toContain('Connected · C:/Users/you/.claude.json')
  })

  it('connects one agent with confirm, and every open one with Connect all', async () => {
    daemon()
    const notices: string[] = []
    const view = await mount(<ExternalClients enabled onNotice={(text) => notices.push(text)} onProblem={() => undefined} />)
    await act(async () => { button(view, 'Connect Codex')!.click() }); await settle()
    expect(rpc).toHaveBeenCalledWith('external.clients.connect', { id: 'codex', confirm: true })
    expect(button(view, 'Disconnect Codex')).not.toBeNull()
    expect(notices.at(-1)).toContain('Connected to Codex')
    expect(notices.at(-1)).toContain('restart any that are open')
    // Only OpenCode is still open, so "Connect all" is gone (it needs two or more).
    expect(buttonText(view, 'Connect all')).toBeUndefined()
  })

  it('connects every open agent at once', async () => {
    daemon()
    const notices: string[] = []
    const view = await mount(<ExternalClients enabled onNotice={(text) => notices.push(text)} onProblem={() => undefined} />)
    await act(async () => { buttonText(view, 'Connect all')!.click() }); await settle()
    const connected = rpc.mock.calls.filter(([method]) => method === 'external.clients.connect').map(([, params]) => (params as { id: string }).id)
    expect(connected).toEqual(['codex', 'opencode'])
    expect(notices.at(-1)).toContain('Connected to Codex and OpenCode')
  })

  it('shows the daemon’s refusal and keeps the row as it was', async () => {
    daemon()
    rpc.mockImplementationOnce(async () => ({ clients: rows() })).mockImplementationOnce(async () => { throw new Error('Codex’s config.toml could not be read.') })
    const problems: Array<string | null> = []
    const view = await mount(<ExternalClients enabled onNotice={() => undefined} onProblem={(text) => problems.push(text)} />)
    await act(async () => { button(view, 'Connect Codex')!.click() }); await settle()
    expect(problems.at(-1)).toBe('Codex’s config.toml could not be read.')
    expect(button(view, 'Connect Codex')).not.toBeNull()
  })

  it('treats a row that comes back with a reason as a refusal, not a success', async () => {
    const list = daemon()
    const base = rpc.getMockImplementation()!
    rpc.mockImplementation(async (method: string, params: Record<string, unknown>) => {
      if (method !== 'external.clients.connect') return base(method, params)
      return { client: { ...list.find((item) => item.id === 'codex')!, error: 'Retinue could not connect Codex.' } }
    })
    const notices: string[] = []
    const problems: Array<string | null> = []
    const view = await mount(<ExternalClients enabled onNotice={(text) => notices.push(text)} onProblem={(text) => problems.push(text)} />)
    await act(async () => { button(view, 'Connect Codex')!.click() }); await settle()
    expect(problems.at(-1)).toBe('Retinue could not connect Codex.')
    expect(notices).toEqual([])
  })

  it('with the MCP off, only offers to remove what Retinue added', async () => {
    const list = daemon()
    const view = await mount(<ExternalClients enabled={false} onNotice={() => undefined} onProblem={() => undefined} />)
    expect(button(view, 'Connect Codex')).toBeNull()
    expect(view.textContent).toContain('Still set up in other apps')
    await act(async () => { buttonText(view, 'Remove')!.click() }); await settle()
    expect(rpc).toHaveBeenCalledWith('external.clients.disconnect', { id: 'claude-code' })
    expect(list.find((item) => item.id === 'claude-code')!.state).toBe('not_connected')
    expect(view.textContent).not.toContain('Still set up in other apps')
  })

  it('offers a retry when the agents could not be checked', async () => {
    rpc.mockRejectedValueOnce(new Error('boom'))
    const view = await mount(<ExternalClients enabled onNotice={() => undefined} onProblem={() => undefined} />)
    expect(view.textContent).toContain('The coding agents could not be checked.')
    daemon()
    await act(async () => { buttonText(view, 'Try again')!.click() }); await settle()
    expect(view.querySelectorAll('.mcp-client')).toHaveLength(4)
  })
})
