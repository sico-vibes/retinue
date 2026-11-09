// @vitest-environment happy-dom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { BlobCanvas } from './BlobCanvas'
import { PROFILE_PHOTOS } from './photos'

const mounted: Array<{ root: Root; host: HTMLDivElement }> = []
function mount(element: React.ReactElement) {
  const host = document.createElement('div'); document.body.append(host)
  const root = createRoot(host)
  act(() => root.render(element))
  mounted.push({ root, host })
  return host
}

beforeEach(() => { vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true) })
afterEach(() => { for (const { root, host } of mounted.splice(0)) { act(() => root.unmount()); host.remove() } vi.useRealTimers(); vi.unstubAllGlobals() })

describe('an employee mark is a profile photo', () => {
  it('draws the chosen portrait in a circle at the asked size', () => {
    const photo = PROFILE_PHOTOS[0]!
    const host = mount(<BlobCanvas color="#8794a3" size={36} look={{ shape: 'circle', photo: photo.id }} label="Ada" />)
    const mark = host.querySelector<HTMLElement>('.profile-photo-mark')!
    expect(mark.style.width).toBe('36px')
    expect(mark.getAttribute('aria-label')).toBe('Ada')
    expect(mark.querySelector('.profile-photo-clip img')?.getAttribute('src')).toBe(photo.url)
    expect(host.querySelector('canvas')).toBeNull()
  })

  it('draws the blank silhouette, in the employee colour, without a photo', () => {
    const host = mount(<BlobCanvas color="#ff7a59" size={32} look={{ shape: 'circle' }} label="New hire" />)
    const clip = host.querySelector<HTMLElement>('.profile-photo-clip.is-blank')!
    expect(clip.style.getPropertyValue('--photo-tint')).toBe('#ff7a59')
    expect(clip.querySelector('svg.profile-photo-blank')).not.toBeNull()
  })

  it('draws an uploaded picture and ignores an unsafe one', () => {
    const uploaded = 'data:image/webp;base64,UklGRg=='
    expect(mount(<BlobCanvas color="#000" look={{ shape: 'circle', photo: uploaded }} />).querySelector('img')?.getAttribute('src')).toBe(uploaded)
    const unsafe = mount(<BlobCanvas color="#000" look={{ shape: 'circle', photo: 'javascript:alert(1)' as never }} />)
    expect(unsafe.querySelector('img')).toBeNull()
    expect(unsafe.querySelector('.profile-photo-clip.is-blank')).not.toBeNull()
  })

  it('dims a resting employee and hides decorative marks from assistive tech', () => {
    const host = mount(<BlobCanvas decorative color="#000" mood="offline" look={{ shape: 'circle' }} />)
    const mark = host.querySelector('.profile-photo-mark')!
    expect(mark.classList.contains('is-resting')).toBe(true)
    expect(mark.getAttribute('aria-hidden')).toBe('true')
  })

  it('finishes a greeting once, on time, so the companion is never left waiting', () => {
    vi.useFakeTimers()
    const done = vi.fn()
    mount(<BlobCanvas color="#000" look={{ shape: 'circle' }} greeting onGreetingComplete={done} />)
    expect(done).not.toHaveBeenCalled()
    act(() => { vi.advanceTimersByTime(1300) })
    expect(done).toHaveBeenCalledOnce()
  })
})
