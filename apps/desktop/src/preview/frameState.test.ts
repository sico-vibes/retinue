import { describe, expect, it } from 'vitest'
import { previewBackdrop, productionFrameWanted } from './frameState'

describe('the preview production frame', () => {
  it('wraps the main window only', () => {
    expect(productionFrameWanted('', true, '/')).toBe(true)
    expect(productionFrameWanted('?style=haze', true, '/index.html')).toBe(true)
    expect(productionFrameWanted('?frame=0', true, '/')).toBe(false)
    expect(productionFrameWanted('?companion', true, '/')).toBe(false)
    expect(productionFrameWanted('?dictation', true, '/')).toBe(false)
    expect(productionFrameWanted('', false, '/')).toBe(false)
    expect(productionFrameWanted('', true, '/gallery.html')).toBe(false)
  })
  it('reports the native acrylic only while the frame is on and Haze asks for it', () => {
    expect(previewBackdrop(true, true)).toMatchObject({ effect: 'acrylic', active: true })
    expect(previewBackdrop(false, true)).toMatchObject({ effect: 'none', active: false })
    expect(previewBackdrop(true, false)).toMatchObject({ effect: 'none', active: false })
  })
})
