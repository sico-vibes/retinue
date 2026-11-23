// @vitest-environment happy-dom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const opened: string[] = []
vi.mock('./openSafeLink', () => ({ openSafeLink: async (url: string) => { opened.push(url) } }))
vi.mock('../appRelease', () => ({ appVersion: '9.9.9-test' }))

const { AboutPage, ABOUT_LINKS } = await import('./AboutPage')

let root: Root | null = null
let host: HTMLDivElement | null = null
afterEach(() => { if (root) act(() => root!.unmount()); root = null; host?.remove(); host = null; opened.length = 0 })

async function mount(canUpdate = true) {
  const onOpenPage = vi.fn()
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  await act(async () => { root!.render(<AboutPage canUpdate={canUpdate} onOpenPage={onOpenPage} />) })
  return { view: host, onOpenPage }
}
const buttonText = (view: HTMLElement, text: string) => [...view.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent?.trim() === text)

describe('Settings › About', () => {
  it('shows the build version and links only to the project on GitHub', async () => {
    const { view } = await mount()
    expect(view.querySelector('.about-version')?.textContent).toBe('Version 9.9.9-test')
    expect(ABOUT_LINKS.every(({ url }) => url.startsWith('https://github.com/sico-vibes/retinue'))).toBe(true)
    await act(async () => { buttonText(view, 'Release notes')!.click() })
    expect(opened).toEqual(['https://github.com/sico-vibes/retinue/releases'])
  })

  it('opens the software pages it lists, and leaves Updates out where the build cannot update itself', async () => {
    const { view, onOpenPage } = await mount()
    const opens = [...view.querySelectorAll<HTMLButtonElement>('.settings-row button')].filter((button) => button.textContent?.includes('Open'))
    expect(opens).toHaveLength(3)
    await act(async () => { opens[1]!.click() })
    expect(onOpenPage).toHaveBeenCalledWith('Health')
    root!.unmount(); root = null; host!.remove()
    const second = await mount(false)
    expect(second.view.textContent).not.toContain('Updates')
  })
})
