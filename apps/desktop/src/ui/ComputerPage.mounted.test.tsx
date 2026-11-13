// @vitest-environment happy-dom
import { act, type ReactNode } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const rpc = vi.fn()
const opened = vi.fn()
vi.mock('../tauri', () => ({
  rpc: (method: string, params: unknown) => rpc(method, params),
  listenForDaemonEvents: async () => () => undefined,
}))
vi.mock('./openSafeLink', () => ({ openSafeLink: (url: string) => { opened(url); return Promise.resolve() } }))

const { ComputerPage } = await import('./ComputerPage')
const { parseComputerViewAddress } = await import('./ComputerView')
const { COMPUTER_SETUP_PROMPT } = await import('./computerSetup')
const { addressProblem, parseComputerStatus, parseRoutines, scheduleText, sizeText, spanSince, stateWord, tidyAddress } = await import('./computer')

let root: Root | null = null
let host: HTMLDivElement | null = null
afterEach(() => { if (root) act(() => root!.unmount()); root = null; host?.remove(); host = null; rpc.mockReset(); opened.mockReset() })

async function mount(node: ReactNode) {
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  await act(async () => { root!.render(node) })
  await settle()
  return host
}
const settle = () => act(async () => { for (let turn = 0; turn < 4; turn += 1) await Promise.resolve() })
const button = (view: HTMLElement, name: string) => [...view.ownerDocument.querySelectorAll('button')].find((item) => item.textContent?.trim() === name || item.getAttribute('aria-label') === name) as HTMLButtonElement | undefined
const click = async (element: Element | undefined) => { expect(element).toBeTruthy(); await act(async () => { (element as HTMLElement).click() }); await settle() }
const type = async (input: HTMLInputElement, value: string) => {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!
  await act(async () => { setter.call(input, value); input.dispatchEvent(new Event('input', { bubbles: true })) })
}

const unlinked = { linked: false, enabled: false }
const awake = { linked: true, enabled: false, address: 'https://dash.example.com', name: 'retinue-01', reachable: true, state: 'awake', since: new Date(Date.now() - 2 * 3_600_000).toISOString(), restarts: 0, cpuPercent: 7, memoryPercent: 31, memoryText: '1.24 GiB / 4 GiB', pids: 180, diskBytes: 2_340_000_000, limits: { cpu: '2', memory: '4g' }, desktopUrl: 'https://dash.example.com/harbor/computer/view/', checkedAt: new Date().toISOString(), problem: null }
const routines = { routines: [{ id: 'rt_1', name: 'Morning check', cron: '0 8 * * *', command: 'node check.mjs', enabled: true, lastRunAt: null, lastOk: null, lastOutput: null }] }

describe('the computer on the owner\'s server', () => {
  it('reads only a well-formed status, and keeps a screen link only on the linked server', () => {
    expect(parseComputerStatus(null)).toBeNull()
    expect(parseComputerStatus({ enabled: true })).toBeNull()
    expect(parseComputerStatus({ linked: false, enabled: true, address: 'https://x.example' })).toMatchObject({ linked: false, enabled: false, address: null })
    const status = parseComputerStatus(awake)!
    expect(status).toMatchObject({ linked: true, state: 'awake', cpuPercent: 7, desktopUrl: awake.desktopUrl })
    expect(parseComputerStatus({ ...awake, desktopUrl: 'https://elsewhere.example/view/' })!.desktopUrl).toBeNull()
    expect(parseComputerStatus({ ...awake, desktopUrl: 'javascript:alert(1)' })!.desktopUrl).toBeNull()
    expect(parseComputerStatus({ ...awake, state: 'on fire' })!.state).toBe('unknown')
    // A server that did not answer has no state, whatever was sent with it.
    expect(parseComputerStatus({ ...awake, reachable: false })!.state).toBe('unknown')
    expect(parseComputerStatus({ ...awake, memoryPercent: 400 })!.memoryPercent).toBe(100)
    expect(JSON.stringify(parseComputerStatus({ ...awake, token: 'hrb_secret' }))).not.toContain('hrb_secret')
  })

  it('says the state in one word and schedules in plain words', () => {
    expect(stateWord({ reachable: true, state: 'awake' })).toBe('Awake')
    expect(stateWord({ reachable: true, state: 'asleep' })).toBe('Asleep')
    expect(stateWord({ reachable: true, state: 'missing' })).toBe('Not set up')
    expect(stateWord({ reachable: false, state: 'awake' })).toBe('Not reachable')
    expect(scheduleText('0 8 * * *')).toBe('Every day at 08:00')
    expect(scheduleText('30 2 * * 1-5')).toBe('Weekdays at 02:30')
    expect(scheduleText('0 9 * * 1')).toBe('Every Monday at 09:00')
    expect(scheduleText('*/30 * * * *')).toBe('Every 30 minutes')
    expect(scheduleText('15 * * * *')).toBe('Every hour at :15')
    expect(scheduleText('0 */6 * * *')).toBe('Every 6 hours')
    expect(scheduleText('0 8 1 * *')).toBe('0 8 1 * *')
    expect(scheduleText('')).toBe('No schedule')
    expect(sizeText(2_340_000_000)).toBe('2.2 GB')
    expect(sizeText(null)).toBeNull()
    const now = Date.parse('2026-11-13T12:00:00Z')
    expect(spanSince('2026-11-13T07:00:00Z', now)).toBe('5 h')
    expect(spanSince('2026-11-10T12:00:00Z', now)).toBe('3 days')
    expect(spanSince('nonsense', now)).toBeNull()
  })

  it('checks an address before it is sent and drops malformed routines', () => {
    expect(addressProblem('dash.example.com')).toBeNull()
    expect(addressProblem('https://me:pw@dash.example.com')).toBe('Leave the sign-in out of the address.')
    expect(addressProblem('ftp://dash.example.com')).toBe('Use an https address.')
    expect(tidyAddress(' dash.example.com/ ')).toBe('https://dash.example.com')
    expect(tidyAddress('http://100.64.0.2:8080')).toBe('http://100.64.0.2:8080')
    expect(parseRoutines({ routines: [{ id: 'a', command: 'ls' }, { id: 'b' }, null, 'x'] })).toEqual([{ id: 'a', name: 'Routine', cron: '', command: 'ls', enabled: true, lastRunAt: null, lastOk: null, lastOutput: null }])
    expect(parseRoutines(null)).toEqual([])
  })

  it('links with the address and token, sends the token once, and never shows it', async () => {
    rpc.mockImplementation(async (method: string) => {
      if (method === 'computer.status') return unlinked
      if (method === 'computer.link') return awake
      if (method === 'computer.routines') return routines
      throw new Error(`unexpected ${method}`)
    })
    const view = await mount(<ComputerPage connected />)
    expect(view.textContent).toContain('Set one up')
    const [address, token] = [...view.querySelectorAll('input')] as [HTMLInputElement, HTMLInputElement]
    expect(token.type).toBe('password')
    expect(button(view, 'Link computer')!.disabled).toBe(true)
    await type(address, 'dash.example.com/')
    await type(token, 'hrb_test_value_1234')
    await click(button(view, 'Link computer'))
    expect(rpc).toHaveBeenCalledWith('computer.link', { address: 'https://dash.example.com', token: 'hrb_test_value_1234', confirm: true })
    expect(view.textContent).toContain('retinue-01')
    expect(view.textContent).toContain('Awake for 2 h')
    expect(view.innerHTML).not.toContain('hrb_test_value_1234')
    // Linked, but employees do not have it until the owner says so.
    expect(button(view, 'Let employees use this computer')!.getAttribute('aria-checked')).toBe('false')
  })

  it('shows the refusal in the daemon\'s words and keeps nothing', async () => {
    rpc.mockImplementation(async (method: string) => {
      if (method === 'computer.status') return unlinked
      if (method === 'computer.link') throw new Error('Harbor refused that token.')
      throw new Error(`unexpected ${method}`)
    })
    const view = await mount(<ComputerPage connected />)
    const [address, token] = [...view.querySelectorAll('input')] as [HTMLInputElement, HTMLInputElement]
    await type(address, 'https://dash.example.com')
    await type(token, 'hrb_wrong_value')
    await click(button(view, 'Link computer'))
    expect(view.querySelector('[role="alert"]')?.textContent).toBe('Harbor refused that token.')
    expect(view.textContent).toContain('Link a computer')
  })

  it('gives employees the computer only after the owner confirms in a dialog', async () => {
    rpc.mockImplementation(async (method: string, params: Record<string, unknown>) => {
      if (method === 'computer.status') return awake
      if (method === 'computer.routines') return routines
      if (method === 'computer.setEnabled') return { ...awake, enabled: params.enabled === true }
      throw new Error(`unexpected ${method}`)
    })
    const view = await mount(<ComputerPage connected />)
    await click(button(view, 'Let employees use this computer'))
    expect(rpc).not.toHaveBeenCalledWith('computer.setEnabled', expect.anything())
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain('never on the server itself or on this PC')
    await click(button(view, 'Turn on'))
    expect(rpc).toHaveBeenCalledWith('computer.setEnabled', { enabled: true, confirm: true })
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(button(view, 'Let employees use this computer')!.getAttribute('aria-checked')).toBe('true')
    // Off needs no dialog.
    await click(button(view, 'Let employees use this computer'))
    expect(rpc).toHaveBeenCalledWith('computer.setEnabled', { enabled: false })
  })

  it('sleeps, wakes and runs a routine, and only offers what the state allows', async () => {
    let state = 'awake'
    rpc.mockImplementation(async (method: string, params: Record<string, unknown>) => {
      if (method === 'computer.status') return { ...awake, state }
      if (method === 'computer.routines') return routines
      if (method === 'computer.action') { state = params.action === 'sleep' ? 'asleep' : 'awake'; return { ...awake, state, since: state === 'awake' ? awake.since : null } }
      if (method === 'computer.routine.run') return { ok: false, output: 'exit 2' }
      throw new Error(`unexpected ${method}`)
    })
    const view = await mount(<ComputerPage connected />)
    expect(button(view, 'Wake')).toBeUndefined()
    await click(button(view, 'Run now'))
    expect(rpc).toHaveBeenCalledWith('computer.routine.run', { id: 'rt_1' })
    expect(view.textContent).toContain('failed just now')
    await click(button(view, 'Output'))
    expect(view.querySelector('.computer-output')?.textContent).toBe('exit 2')
    await click(button(view, 'Sleep'))
    expect(rpc).toHaveBeenCalledWith('computer.action', { action: 'sleep' })
    expect(button(view, 'Sleep')).toBeUndefined()
    expect(button(view, 'Open the screen')).toBeUndefined()
    expect(button(view, 'Run now')!.disabled).toBe(true)
    await click(button(view, 'Wake'))
    expect(rpc).toHaveBeenCalledWith('computer.action', { action: 'wake' })
  })

  it('offers a setup prompt that names every route the daemon calls, and copies it', async () => {
    for (const piece of ['/api/computer', '/action/wake', '/action/stop', '/action/restart', '/api/computer/logs', '/api/computer/exec', '/api/computer/routines', '/computer/view/', 'Authorization: Bearer', 'hrb_', '127.0.0.1 only']) expect(COMPUTER_SETUP_PROMPT).toContain(piece)
    const written = vi.fn(async () => undefined)
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: written } })
    rpc.mockImplementation(async (method: string) => { if (method === 'computer.status') return unlinked; throw new Error(`unexpected ${method}`) })
    const view = await mount(<ComputerPage connected />)
    await click(button(view, 'Copy the prompt'))
    expect(written).toHaveBeenCalledWith(COMPUTER_SETUP_PROMPT)
    expect(view.textContent).toContain('Copied')
  })

  it('frames only an address on this PC', () => {
    expect(parseComputerViewAddress({ url: 'http://127.0.0.1:48611/computer/view/abc/' })).toEqual({ url: 'http://127.0.0.1:48611/computer/view/abc/', reason: null })
    expect(parseComputerViewAddress({ url: 'https://dash.example.com/harbor/computer/view/' }).url).toBeNull()
    expect(parseComputerViewAddress({ url: 'javascript:alert(1)' }).url).toBeNull()
    expect(parseComputerViewAddress({ url: null, reason: 'Not yet.' })).toEqual({ url: null, reason: 'Not yet.' })
    expect(parseComputerViewAddress(null)).toEqual({ url: null, reason: null })
  })

  it('opens the screen watching, lets nothing through until Take control, and falls back to the browser', async () => {
    let offered = true
    rpc.mockImplementation(async (method: string) => {
      if (method === 'computer.status') return awake
      if (method === 'computer.routines') return routines
      if (method === 'computer.view') return offered ? { url: 'http://127.0.0.1:48611/computer/view/abc/', reason: null } : { url: null, reason: 'Your server does not offer its screen to Retinue yet.' }
      throw new Error(`unexpected ${method}`)
    })
    const view = await mount(<ComputerPage connected />)
    await click(button(view, 'Open the screen'))
    const dialog = document.querySelector('.computer-view')!
    expect(dialog.querySelector('iframe')?.getAttribute('src')).toBe('http://127.0.0.1:48611/computer/view/abc/')
    expect(dialog.querySelector('.computer-view-shield')).toBeTruthy()
    expect(dialog.querySelector('iframe')?.tabIndex).toBe(-1)
    await click([...dialog.querySelectorAll('[role="radio"]')].find((item) => item.textContent === 'Take control'))
    expect(dialog.querySelector('.computer-view-shield')).toBeNull()
    expect(dialog.textContent).toContain('Your mouse and keyboard now reach the computer')
    await click([...dialog.querySelectorAll('[role="radio"]')].find((item) => item.textContent === 'Watching'))
    expect(dialog.querySelector('.computer-view-shield')).toBeTruthy()
    await click(dialog.querySelector('[aria-label="Close"]') ?? undefined)
    expect(document.querySelector('.computer-view')).toBeNull()
    // A server that does not offer its screen: say why and open the browser instead.
    offered = false
    await click(button(view, 'Open the screen'))
    const fallback = document.querySelector('.computer-view')!
    expect(fallback.querySelector('iframe')).toBeNull()
    expect(fallback.textContent).toContain('does not offer its screen')
    await click([...fallback.querySelectorAll('button')].find((item) => item.textContent === 'Open in your browser'))
    expect(opened).toHaveBeenCalledWith(awake.desktopUrl)
  })

  it('says so plainly when the server does not answer, and can still unlink', async () => {
    rpc.mockImplementation(async (method: string) => {
      if (method === 'computer.status') return { ...awake, reachable: false, state: 'unknown', problem: 'Your server did not answer in time.' }
      if (method === 'computer.unlink') return unlinked
      throw new Error(`unexpected ${method}`)
    })
    const view = await mount(<ComputerPage connected />)
    expect(view.textContent).toContain('Not reachable')
    expect(view.textContent).toContain('Your server did not answer in time. Nothing was changed.')
    expect(view.textContent).not.toContain('Routines')
    expect(rpc).not.toHaveBeenCalledWith('computer.routines', expect.anything())
    await click(button(view, 'Unlink'))
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain('revoke it in Harbor')
    await click([...document.querySelectorAll('[role="dialog"] button')].find((item) => item.textContent === 'Unlink'))
    expect(rpc).toHaveBeenCalledWith('computer.unlink', {})
    expect(view.textContent).toContain('Link a computer')
  })
})
