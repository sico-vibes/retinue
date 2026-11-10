// @vitest-environment happy-dom
// The hire dialog (owner, 9 November 2026): a preset can be renamed, given a personality (its
// prompt editable) and skills switched on or off, with toggles, before `project.hire` is called.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const rpc = vi.fn()
vi.mock('../tauri', () => ({
  rpc: (method: string, params: unknown) => rpc(method, params),
  listenForDaemonEvents: async () => () => undefined,
}))

const { TeamPage } = await import('./TeamPage')
const { PRESETS } = await import('../team/presets')

let root: Root | null = null
let host: HTMLDivElement | null = null
afterEach(() => { if (root) act(() => root!.unmount()); root = null; host?.remove(); host = null; rpc.mockReset(); document.body.innerHTML = '' })
const settle = () => act(async () => { for (let index = 0; index < 4; index += 1) await Promise.resolve() })
const click = async (element: Element | null | undefined) => { if (!element) throw new Error('missing element'); await act(async () => { (element as HTMLElement).click() }) }

describe('hire dialog', () => {
  it('hires a renamed preset with its personality, chosen skills and no project, using toggles', async () => {
    rpc.mockImplementation(async (method: string) => {
      if (method === 'project.hire') return { agentId: 'new-ada', created: true }
      if (method === 'runtime.models') throw new Error('offline')
      return {}
    })
    const runtimes = [{ id: 'rt-codex', provider: 'codex', status: 'online', capabilities: { newSession: true } }]
    host = document.createElement('div'); document.body.append(host); root = createRoot(host)
    await act(async () => { root!.render(<TeamPage agents={[]} projects={[]} sessions={[]} runtimes={runtimes as never} connected initialTab="hire" onBack={() => undefined} onOpenAgent={() => undefined} onEditAgent={() => undefined} onCreateOwn={() => undefined} onError={(message) => { throw new Error(message) }} />) })
    await settle()
    const ada = PRESETS.find((preset) => preset.id === 'ada')!
    const card = [...document.querySelectorAll('.hire-card')].find((item) => item.textContent?.includes(ada.nickname))
    await click(card?.querySelector('button'))
    await settle()
    const dialog = document.querySelector('.hire-dialog')!
    expect(dialog.querySelector('input[type="checkbox"]')).toBeNull()
    const name = dialog.querySelector<HTMLInputElement>('input[aria-label="Name"]')!
    await act(async () => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(name, 'Adaline'); name.dispatchEvent(new Event('input', { bubbles: true })) })
    await click([...dialog.querySelectorAll('.hire-personality')].find((item) => item.textContent?.includes('Mentor')))
    expect(dialog.querySelector('.hire-example-reply')?.textContent).toContain('path.join')
    const skillToggles = [...dialog.querySelectorAll<HTMLButtonElement>('.hire-row [role="switch"]')]
    expect(skillToggles.length).toBe(ada.skills.default.length + ada.skills.optional.length)
    await click(skillToggles[0])
    await click([...dialog.querySelectorAll('button')].find((button) => button.textContent?.startsWith('Hire Adaline')))
    await settle()
    const call = rpc.mock.calls.find(([method]) => method === 'project.hire')!
    expect(call[1]).toMatchObject({ projectId: null, presetId: 'ada', name: 'Adaline', personality: { id: 'mentor' }, runtimeId: 'rt-codex' })
    expect((call[1] as { skills: string[] }).skills).toEqual(ada.skills.default.slice(1))
  })
})
