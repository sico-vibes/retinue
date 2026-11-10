// @vitest-environment happy-dom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { BlobLook } from '../blob/look'
import { PROFILE_PHOTOS } from '../blob/photos'
import { LookEditor } from './LookEditor'

vi.mock('../blob/BlobCanvas', () => ({
  BlobCanvas: ({ look, label }: { look?: BlobLook; label?: string }) => <span data-photo={look?.photo ?? 'blank'} data-label={label} />,
}))

const mounted: Array<{ root: Root; host: HTMLDivElement }> = []
function mount(look: BlobLook, locked = false) {
  const host = document.createElement('div'); document.body.append(host)
  const root = createRoot(host)
  const onLookChange = vi.fn()
  const onColorChange = vi.fn()
  act(() => root.render(<LookEditor look={look} color="mint" colorHex="#9be7c4" name="Pololo" locked={locked} onLookChange={onLookChange} onColorChange={onColorChange} />))
  mounted.push({ root, host })
  return { host, onLookChange, onColorChange }
}
const click = (element: Element | null | undefined) => act(() => { (element as HTMLElement).click() })

beforeEach(() => { vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true) })
afterEach(() => { for (const { root, host } of mounted.splice(0)) { act(() => root.unmount()); host.remove() } vi.unstubAllGlobals(); vi.restoreAllMocks() })

describe('look editor (profile photos)', () => {
  it('offers no photo, every portrait and an upload, and nothing else', () => {
    const { host } = mount({ shape: 'circle' })
    // Upload comes first, then No photo, then the portraits, a page at a time (owner, 9 November 2026).
    const options = [...host.querySelectorAll<HTMLButtonElement>('.profile-photo-option')]
    expect(options[0]?.getAttribute('aria-label')).toBe('Upload your own photo')
    expect(options[1]?.getAttribute('aria-label')).toBe('No photo')
    const seen = new Set<string>()
    for (let page = 0; page < 20; page += 1) {
      for (const option of host.querySelectorAll<HTMLButtonElement>('.profile-photo-option')) { const label = option.getAttribute('aria-label') ?? ''; if (label.startsWith('Profile photo ')) seen.add(label) }
      const next = host.querySelector<HTMLButtonElement>('[aria-label="More photos"]')
      if (!next || next.disabled) break
      act(() => next.click())
    }
    expect(seen.size).toBe(PROFILE_PHOTOS.length)
    expect(host.querySelector('input[type="file"]')?.getAttribute('accept')).toContain('image/png')
    // The mascot and art controls are gone.
    expect(host.querySelector('.look-style-option, .haze-character-grid, .look-moods')).toBeNull()
  })

  it('starts on no photo with the colour shown, and picks a portrait', () => {
    const { host, onLookChange } = mount({ shape: 'circle', seed: 'a' })
    expect(host.querySelector('[aria-label="No photo"]')?.getAttribute('aria-checked')).toBe('true')
    expect(host.querySelector('.look-colour')).not.toBeNull()
    click(host.querySelector('[aria-label="Profile photo 2"]'))
    expect(onLookChange).toHaveBeenLastCalledWith({ shape: 'circle', seed: 'a', photo: PROFILE_PHOTOS[1]!.id })
  })

  it('marks the chosen portrait, hides the colour, and goes back to no photo', () => {
    const photo = PROFILE_PHOTOS[0]!.id
    const { host, onLookChange } = mount({ shape: 'circle', photo })
    expect(host.querySelector('[aria-label="Profile photo 1"]')?.getAttribute('aria-checked')).toBe('true')
    expect(host.querySelector('.look-colour')).toBeNull()
    click(host.querySelector('[aria-label="No photo"]'))
    expect(onLookChange).toHaveBeenLastCalledWith({ shape: 'circle' })
  })

  it('keeps one tab stop in the photo grid', () => {
    const { host } = mount({ shape: 'circle', photo: PROFILE_PHOTOS[2]!.id })
    const radios = [...host.querySelectorAll<HTMLButtonElement>('.profile-photo-grid [role="radio"]')]
    expect(radios.filter((radio) => radio.tabIndex === 0)).toHaveLength(1)
  })

  it('shows an uploaded photo as its own chosen tile', () => {
    const uploaded = 'data:image/webp;base64,UklGRg=='
    const { host } = mount({ shape: 'circle', photo: uploaded })
    const tile = host.querySelector<HTMLButtonElement>('[aria-label="Your photo"]')
    expect(tile?.getAttribute('aria-checked')).toBe('true')
    expect(tile?.querySelector('img')?.getAttribute('src')).toBe(uploaded)
  })

  it('makes every control inert while the employee is locked', () => {
    const { host, onLookChange, onColorChange } = mount({ shape: 'circle' }, true)
    const buttons = [...host.querySelectorAll<HTMLButtonElement>('.profile-photo-option')]
    expect(buttons.length).toBeGreaterThan(2)
    expect(buttons.every((button) => button.disabled)).toBe(true)
    click(host.querySelector('[aria-label="Profile photo 1"]'))
    expect(onLookChange).not.toHaveBeenCalled()
    expect(onColorChange).not.toHaveBeenCalled()
  })
})
