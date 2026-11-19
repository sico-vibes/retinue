import { describe, expect, it } from 'vitest'
import { applyEffect, CHAT_BACKGROUND_EFFECTS } from './chatBackgroundEffects'
import { ONBOARDING_WALLPAPER, WALLPAPER_CATEGORIES, WALLPAPER_PRESETS } from './wallpaperPresets'
import { CHAT_BACKGROUND_OPACITY_MAX, CHAT_BACKGROUND_OPACITY_MIN, DEFAULT_CHAT_BACKGROUND, normalizeChatBackgroundPrefs } from './chatBackgroundStore'

const source = (width = 12, height = 16) => {
  const data = new Uint8ClampedArray(width * height * 4)
  for (let pixel = 0; pixel < width * height; pixel += 1) data.set([200, 120, 40, 255], pixel * 4)
  return { width, height, data }
}

describe('chat background effects', () => {
  it('keeps the size and alpha, and changes the pixels, for every effect that draws', () => {
    for (const { value } of CHAT_BACKGROUND_EFFECTS) {
      const input = source()
      const output = applyEffect(input, value, false)
      expect(output).toHaveLength(input.data.length)
      for (let alpha = 3; alpha < output.length; alpha += 4) expect(output[alpha]).toBe(255)
      if (value === 'none' || value === 'haze') expect(output).toBe(input.data)
      else expect(Array.from(output)).not.toEqual(Array.from(input.data))
    }
  })

  it('never writes into the source image', () => {
    const input = source()
    const before = Array.from(input.data)
    for (const { value } of CHAT_BACKGROUND_EFFECTS) applyEffect(input, value, true)
    expect(Array.from(input.data)).toEqual(before)
  })

  it('uses a light paper in the light theme', () => {
    const input = source()
    expect(Array.from(applyEffect(input, 'scanlines', true))).not.toEqual(Array.from(applyEffect(input, 'scanlines', false)))
  })
})

describe('chat background settings', () => {
  it('is off until an image is chosen', () => {
    expect(normalizeChatBackgroundPrefs(undefined)).toEqual(DEFAULT_CHAT_BACKGROUND)
    expect(DEFAULT_CHAT_BACKGROUND.hasImage).toBe(false)
    expect(DEFAULT_CHAT_BACKGROUND.effect).toBe('none')
  })

  it('puts an unknown effect or an out-of-range strength back in bounds', () => {
    const prefs = normalizeChatBackgroundPrefs({ hasImage: true, effect: 'plasma', scope: 'nowhere', emptyOpacity: 9, sessionOpacity: -1 })
    expect(prefs).toEqual({ hasImage: true, preset: null, palette: null, matchColors: true, effect: 'none', scope: 'all', emptyOpacity: CHAT_BACKGROUND_OPACITY_MAX, sessionOpacity: CHAT_BACKGROUND_OPACITY_MIN })
  })

  it('keeps a known preset only while an image is set', () => {
    expect(normalizeChatBackgroundPrefs({ hasImage: true, preset: 'space-2' }).preset).toBe('space-2')
    expect(normalizeChatBackgroundPrefs({ hasImage: true, preset: 'space-99' }).preset).toBeNull()
    expect(normalizeChatBackgroundPrefs({ hasImage: false, preset: 'space-2' }).preset).toBeNull()
  })
})

describe('wallpaper presets', () => {
  it('lists the pictures of each category, served from public/background', () => {
    for (const { value, count } of WALLPAPER_CATEGORIES) expect(WALLPAPER_PRESETS.filter((preset) => preset.category === value)).toHaveLength(count)
    expect(new Set(WALLPAPER_PRESETS.map((preset) => preset.id)).size).toBe(WALLPAPER_PRESETS.length)
    for (const preset of WALLPAPER_PRESETS) expect(preset.url).toBe(`/background/${preset.id}.webp`)
    expect(ONBOARDING_WALLPAPER.id).toBe('samurai-2')
  })
})

