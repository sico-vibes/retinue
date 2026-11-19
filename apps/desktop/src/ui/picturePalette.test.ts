// @vitest-environment happy-dom
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { normalizePicturePalette, picturePalette } from './picturePalette'
import { applyPicturePalette, picturePaletteActive } from './picturePaletteApply'
import { DEFAULT_CHAT_BACKGROUND } from './chatBackgroundStore'
import { WALLPAPER_PRESETS } from './wallpaperPresets'

/** A 40×20 picture: red on the left half, blue on the right, with a vivid yellow square in the middle. */
function picture() {
  const width = 40, height = 20, data = new Uint8ClampedArray(width * height * 4)
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const i = (y * width + x) * 4
    const yellow = x >= 18 && x < 22 && y >= 8 && y < 12
    const [r, g, b] = yellow ? [250, 220, 20] : x < 20 ? [170, 50, 50] : [40, 70, 170]
    data[i] = r; data[i + 1] = g; data[i + 2] = b; data[i + 3] = 255
  }
  return { width, height, data }
}
const hue = (hex: string) => { const [r, , b] = [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16)); return r! > b! ? 'warm' : 'cool' }

describe('picture palette', () => {
  it('takes each mist colour from the part of the picture it sits over', () => {
    const palette = picturePalette(picture())
    expect(hue(palette.d1)).toBe('warm')
    expect(hue(palette.d2)).toBe('cool')
    for (const value of Object.values(palette)) expect(value).toMatch(/^#[0-9a-f]{6}$/)
  })

  it('keeps only a complete, well-formed palette', () => {
    const palette = picturePalette(picture())
    expect(normalizePicturePalette(palette)).toEqual(palette)
    expect(normalizePicturePalette({ ...palette, accent: 'red' })).toBeNull()
    expect(normalizePicturePalette(null)).toBeNull()
  })

  it('wears the palette only in Haze, with a picture, while matching is on', () => {
    const palette = picturePalette(picture())
    const on = { ...DEFAULT_CHAT_BACKGROUND, hasImage: true, palette }
    expect(picturePaletteActive('haze', on)).toBe(true)
    expect(picturePaletteActive('stone', on)).toBe(false)
    expect(picturePaletteActive('haze', { ...on, matchColors: false })).toBe(false)
    const root = document.createElement('html')
    applyPicturePalette(root, 'haze', on)
    expect(root.dataset.picturePalette).toBe('on')
    expect(root.style.getPropertyValue('--picture-d1')).toBe(palette.d1)
    applyPicturePalette(root, 'haze', { ...on, matchColors: false })
    expect(root.dataset.picturePalette).toBeUndefined()
    expect(root.style.getPropertyValue('--picture-d1')).toBe('')
  })

  it('ships every preset picture it lists', () => {
    for (const preset of WALLPAPER_PRESETS) expect(existsSync(join(__dirname, '../../public', preset.url)), preset.url).toBe(true)
  })
})
