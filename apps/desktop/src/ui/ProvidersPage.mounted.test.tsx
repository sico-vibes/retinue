// @vitest-environment happy-dom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const h = vi.hoisted(() => ({ rpc: vi.fn() }))
vi.mock('../tauri', () => ({ rpc: h.rpc, inDesktop: true }))

import { ProvidersPage, parseProviders, providerLine } from './ProvidersPage'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const LIST = { providers: [
  { provider: 'claude', displayName: 'Claude Code', main: true, enabled: true, found: true, version: '2.1.322' },
  { provider: 'codex', displayName: 'Codex', main: true, enabled: true, found: true, version: null },
  { provider: 'cursor', displayName: 'Cursor', main: false, enabled: false, found: true, version: '1.9.0' },
  { provider: 'antigravity', displayName: 'Antigravity', main: false, enabled: false, found: false, installHint: 'Install Google Antigravity, then scan again.' },
  { displayName: 'No id' },
] }

let root: Root
let host: HTMLDivElement
beforeEach(() => { h.rpc.mockReset(); host = document.createElement('div'); document.body.appendChild(host); root = createRoot(host) })
afterEach(() => { act(() => root.unmount()); host.remove() })
const settle = async () => { await act(async () => { await Promise.resolve(); await Promise.resolve() }) }

describe('providers page', () => {
  it('reads only well-formed rows and never guesses what is installed', () => {
    const rows = parseProviders(LIST)
    expect(rows.map((row) => row.provider)).toEqual(['claude', 'codex', 'cursor', 'antigravity'])
    expect(providerLine(rows[0])).toBe('Installed · 2.1.322')
    expect(providerLine(rows[1])).toBe('Installed')
    expect(providerLine(rows[2])).toBe('Installed · 1.9.0 · Off: not offered to employees')
    expect(providerLine(rows[3])).toBe('Not installed. Install Google Antigravity, then scan again.')
    expect(parseProviders(null)).toEqual([])
  })

  it('shows the main coding agents as always on, with no switch', async () => {
    h.rpc.mockResolvedValue(LIST)
    await act(async () => { root.render(<ProvidersPage disabled={['cursor', 'antigravity']} onChange={vi.fn()} />) })
    await settle()
    const fixed = [...host.querySelectorAll('.provider-row-fixed')].map((node) => node.textContent)
    expect(fixed).toEqual(['Always on', 'Always on'])
    expect(host.querySelector('[aria-label="Use Claude Code"]')).toBeNull()
    expect(host.querySelector<HTMLButtonElement>('[aria-label="Use Cursor"]')?.getAttribute('aria-checked')).toBe('false')
  })

  it('switches an optional coding agent on by taking its id off the stored list', async () => {
    h.rpc.mockResolvedValue(LIST)
    const onChange = vi.fn().mockResolvedValue(null)
    await act(async () => { root.render(<ProvidersPage disabled={['cursor', 'antigravity', 'runtime-x']} onChange={onChange} />) })
    await settle()
    await act(async () => { host.querySelector<HTMLButtonElement>('[aria-label="Use Cursor"]')!.click() })
    await settle()
    // Other entries (another provider, one installed copy turned off) are kept.
    expect(onChange).toHaveBeenCalledWith(['antigravity', 'runtime-x'])
    // The list is read again afterwards, so the page shows what the daemon says is true.
    expect(h.rpc.mock.calls.filter(([method]) => method === 'provider.list').length).toBe(2)
  })

  it('says so when the list cannot be read', async () => {
    h.rpc.mockRejectedValue(new Error('offline'))
    await act(async () => { root.render(<ProvidersPage disabled={[]} onChange={vi.fn()} />) })
    await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('The list of coding agents could not be read.')
  })
})
