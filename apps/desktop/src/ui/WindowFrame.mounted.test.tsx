// @vitest-environment happy-dom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'

const windowApi = vi.hoisted(() => ({
  minimize: vi.fn(async () => undefined), toggleMaximize: vi.fn(async () => undefined), close: vi.fn(async () => undefined),
  isMaximized: vi.fn(async () => false), onResized: vi.fn(async () => () => undefined),
}))
vi.mock('../transport/nativeBridge', () => ({ isNative: () => false, currentWindow: () => windowApi }))
const platform = vi.hoisted(() => ({ value: { kind: 'desktop', os: 'linux' } as { kind: string; os: string } }))
vi.mock('../transport/platform', () => ({ usePlatform: () => platform.value }))

import { WindowFrame, drawsOwnFrame } from './WindowFrame'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

async function mount() {
  const host = document.createElement('div')
  document.body.append(host)
  const root = createRoot(host)
  await act(async () => { root.render(<WindowFrame />) })
  return { host, unmount: () => act(() => root.unmount()) }
}

afterEach(() => { document.body.innerHTML = ''; vi.clearAllMocks() })

describe('the drawn title bar', () => {
  it('is drawn on Linux only, never on Windows or in a plain browser', () => {
    expect(drawsOwnFrame({ os: 'linux', kind: 'desktop' })).toBe(true)
    expect(drawsOwnFrame({ os: 'linux', kind: 'preview' })).toBe(true)
    expect(drawsOwnFrame({ os: 'windows', kind: 'desktop' })).toBe(false)
    expect(drawsOwnFrame({ os: 'linux', kind: 'browser' })).toBe(false)
  })

  it('moves the window with its buttons and makes room for itself', async () => {
    platform.value = { kind: 'desktop', os: 'linux' }
    const { host, unmount } = await mount()
    expect(document.documentElement.dataset.frame).toBe('drawn')
    expect(host.querySelector('.window-frame')?.hasAttribute('data-tauri-drag-region')).toBe(true)
    for (const [label, call] of [['Minimise', windowApi.minimize], ['Maximise', windowApi.toggleMaximize], ['Close', windowApi.close]] as const) {
      await act(async () => { (host.querySelector(`button[aria-label="${label}"]`) as HTMLButtonElement).click() })
      expect(call).toHaveBeenCalledTimes(1)
    }
    await unmount()
    expect(document.documentElement.dataset.frame).toBeUndefined()
  })

  it('draws nothing on Windows, which keeps the system title bar', async () => {
    platform.value = { kind: 'desktop', os: 'windows' }
    const { host } = await mount()
    expect(host.querySelector('.window-frame')).toBeNull()
    expect(document.documentElement.dataset.frame).toBeUndefined()
  })
})
