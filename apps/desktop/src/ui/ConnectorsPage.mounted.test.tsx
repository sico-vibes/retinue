// @vitest-environment happy-dom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const rpc = vi.hoisted(() => vi.fn())
vi.mock('../tauri', () => ({ inDesktop: true, previewMode: false, rpc, listenForDaemonEvents: async () => () => undefined }))

import { ConnectorsPage, connectorStatusLine, connectorUrlProblem, type Connector } from './ConnectorsPage'

// Owner request, 16 November 2026: MCP connectors the daemon calls (docs/plans/nod-adoption-plan.md, D).
const linear: Connector = { id: 'c1', name: 'Linear', url: 'https://mcp.linear.app/mcp', auth: 'token', enabled: true, status: 'ok', detail: null, toolCount: 2, unattended: 'off', lastCheckedAt: null }
const tools = [
  { name: 'list_issues', title: 'List issues', description: 'Find issues.', readOnly: true, rule: 'allow' },
  { name: 'create_issue', title: 'Create issue', description: 'Open an issue.', readOnly: false, rule: 'ask' },
]

let host: HTMLDivElement
let root: Root
beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true
  rpc.mockReset()
  rpc.mockImplementation(async (method: string) => {
    if (method === 'connector.list') return { connectors: [linear], catalogue: [{ id: 'notion', name: 'Notion', url: 'https://mcp.notion.com/mcp', docsUrl: 'https://www.notion.so/profile/integrations', auth: 'token', tokenHint: 'Create an integration.' }, { id: 'linear', name: 'Linear', url: linear.url, docsUrl: 'https://linear.app', auth: 'token', tokenHint: '' }] }
    if (method === 'connector.tools') return { tools }
    return { ok: true }
  })
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
})
afterEach(() => { act(() => root.unmount()); host.remove() })
const flush = () => act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)) })

describe('connectors page', () => {
  it('checks addresses and describes status', () => {
    expect(connectorUrlProblem('https://mcp.linear.app/mcp')).toBeNull()
    expect(connectorUrlProblem('http://localhost:3000/mcp')).toBeNull()
    expect(connectorUrlProblem('http://example.com/mcp')).toMatch(/https/)
    expect(connectorUrlProblem('linear')).toMatch(/full address/)
    expect(connectorStatusLine(linear)).toBe('Connected · 2 tools')
    expect(connectorStatusLine({ ...linear, enabled: false })).toBe('Turned off')
    expect(connectorStatusLine({ ...linear, status: 'error', detail: 'Refused (401).' })).toBe('Refused (401).')
  })

  it('lists connectors, hides catalogue apps already added, and saves a tool rule', async () => {
    act(() => root.render(<ConnectorsPage />))
    await flush()
    expect(host.textContent).toContain('Linear')
    const suggestions = [...host.querySelectorAll('.connector-suggestion')].map((button) => button.textContent)
    expect(suggestions).toEqual(['Notion'])
    act(() => (host.querySelector('.connector-row-main button[aria-expanded]') as HTMLButtonElement).click())
    await flush()
    expect(host.textContent).toContain('Create issue')
    expect(host.textContent).toContain('Read only')
    const never = [...host.querySelectorAll('.connector-tool')][1]!.querySelectorAll('button')
    act(() => ([...never].find((button) => button.textContent === 'Never') as HTMLButtonElement).click())
    expect(rpc).toHaveBeenCalledWith('connector.tool.rule', { id: 'c1', tool: 'create_issue', rule: 'deny' })
  })

  it('adds a catalogue app with its address filled in', async () => {
    act(() => root.render(<ConnectorsPage />))
    await flush()
    act(() => (host.querySelector('.connector-suggestion') as HTMLButtonElement).click())
    const url = host.querySelector('input[inputmode="url"]') as HTMLInputElement
    expect(url.value).toBe('https://mcp.notion.com/mcp')
    act(() => (host.querySelector('.connector-form') as HTMLFormElement).requestSubmit())
    await flush()
    expect(rpc).toHaveBeenCalledWith('connector.add', { name: 'Notion', url: 'https://mcp.notion.com/mcp' })
  })
})
