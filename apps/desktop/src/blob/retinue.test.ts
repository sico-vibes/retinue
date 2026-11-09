// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { robotStateFor } from './retinue'
import { BOT_SHAPES, defaultBotShape, normalizeBot, randomBot, resolveBot, sameBot } from './look/bot'
import { agentLook, normalizeLook } from './look'

/** Retinue reads its cache at module load, so each case imports a fresh copy. */
async function loadModule() {
  vi.resetModules()
  return import('./retinue')
}

describe('theme style switch', () => {
  beforeEach(() => { localStorage.clear() })

  // The owner made Haze the primary look on 24 October 2026; a new install starts in it.
  it('defaults to haze and marks the document', async () => {
    const retinue = await loadModule()
    expect(retinue.themeStyle()).toBe('haze')
    expect(retinue.retinueEnabled()).toBe(false)
    expect(document.documentElement.dataset.style).toBe('haze')
    expect(document.documentElement.dataset.retinue).toBeUndefined()
    // Wallpaper and glass belonged to Frost, retired on 27 October 2026.
    expect(document.documentElement.dataset.wallpaper).toBeUndefined()
    expect(document.documentElement.dataset.glass).toBeUndefined()
  })

  it('switches between the two looks and keeps the retired pixel marker absent', async () => {
    const retinue = await loadModule()
    for (const look of ['stone', 'haze'] as const) {
      retinue.setThemeStyle(look)
      expect(document.documentElement.dataset.style).toBe(look)
      expect(document.documentElement.dataset.retinue).toBeUndefined()
      expect(localStorage.getItem('retinue.style')).toBe(look)
    }
    expect(retinue.retinueEnabled()).toBe(false)
  })

  // Frost (stored as frost) was retired on 27 October 2026, after Legacy, Pop and Dot.
  it.each(['frost', 'legacy', 'pop', 'dot'])('migrates a removed cached look %s to Haze', async (look) => {
    localStorage.setItem('retinue.style', look)
    const retinue = await loadModule()
    expect(retinue.themeStyle()).toBe('haze')
    expect(localStorage.getItem('retinue.style')).toBe('haze')
    expect(document.documentElement.dataset.retinue).toBeUndefined()
  })

  it('reads the new cache key first', async () => {
    localStorage.setItem('retinue.style', 'stone')
    const retinue = await loadModule()
    expect(retinue.themeStyle()).toBe('stone')
    expect(document.documentElement.dataset.style).toBe('stone')
  })

  it('starts on the default look when nothing is cached', async () => {
    expect((await loadModule()).themeStyle()).toBe('haze')
  })

  it('applies and clears the user accent on the document', async () => {
    const retinue = await loadModule()
    retinue.setAccentColor('#E17A56')
    const root = document.documentElement
    expect(retinue.accentColor()).toBe('#e17a56')
    expect(root.dataset.accent).toBe('custom')
    expect(root.style.getPropertyValue('--user-accent')).toBe('#e17a56')
    expect(root.style.getPropertyValue('--user-accent-pressed')).toBe('#c66b4c')
    expect(root.style.getPropertyValue('--user-accent-soft')).toBe('rgba(225,122,86,.14)')
    expect(root.style.getPropertyValue('--user-accent-ink')).toBe('#0f0f10')
    expect(localStorage.getItem('retinue.accent')).toBe('#e17a56')

    retinue.setAccentColor('')
    expect(retinue.accentColor()).toBe('')
    expect(root.style.getPropertyValue('--user-accent')).toBe('')
    expect(root.style.getPropertyValue('--user-accent-pressed')).toBe('')
    expect(root.style.getPropertyValue('--user-accent-soft')).toBe('')
    expect(root.style.getPropertyValue('--user-accent-ink')).toBe('')
    expect(root.dataset.accent).toBeUndefined()
    expect(localStorage.getItem('retinue.accent')).toBeNull()
  })

  it('applies a cached accent at module load so first paint matches', async () => {
    localStorage.setItem('retinue.accent', '#3b82f6')
    const retinue = await loadModule()
    expect(retinue.accentColor()).toBe('#3b82f6')
    expect(document.documentElement.dataset.accent).toBe('custom')
    expect(document.documentElement.style.getPropertyValue('--user-accent')).toBe('#3b82f6')
  })

  it('ignores an invalid accent', async () => {
    const retinue = await loadModule()
    retinue.setAccentColor('not-a-colour')
    expect(retinue.accentColor()).toBe('')
    expect(document.documentElement.dataset.accent).toBeUndefined()
  })
})

describe('robot styling', () => {
  it('maps every blob mood onto one of the nine robot faces', () => {
    const faces = new Set(['idle', 'thinking', 'searching', 'listening', 'speaking', 'working', 'happy', 'error', 'sleeping'])
    for (const mood of ['idle', 'online', 'thinking', 'working', 'tool_activity', 'file_activity', 'file_drop', 'file_preparing', 'file_ready', 'file_sending', 'file_error', 'permission', 'success', 'error', 'offline', 'listening', 'rate_limited', 'sleeping'] as const) {
      expect(faces.has(robotStateFor(mood)), mood).toBe(true)
    }
    expect(robotStateFor('permission')).toBe('listening')
    expect(robotStateFor('success')).toBe('happy')
    expect(robotStateFor('tool_activity')).toBe('searching')
  })

  it('only ever offers the four outlines', () => {
    expect([...BOT_SHAPES]).toEqual(['rectangle', 'square', 'circle', 'hexagon'])
    expect(normalizeBot({ shape: 'triangle' })).toBeUndefined()
    expect(normalizeBot({ shape: 'circle', antenna: true, speed: 4 })).toEqual({ shape: 'circle' })
  })

  it('keeps only valid hex colours and lowercases them', () => {
    expect(normalizeBot({ shell: '#2B49A3', trim: 'steel', screen: '#FFFFFF' })).toEqual({ shell: '#2b49a3', screen: '#ffffff' })
    expect(normalizeBot('nope')).toBeUndefined()
  })

  it('fills the gaps from the blob: its silhouette and its colour', () => {
    expect(defaultBotShape('round')).toBe('circle')
    expect(defaultBotShape('hexagon')).toBe('hexagon')
    expect(defaultBotShape('mystery')).toBe('rectangle')
    expect(resolveBot({ shape: 'boxy' }, '#aabbcc')).toMatchObject({ shape: 'square', shell: '#aabbcc' })
    expect(resolveBot({ shape: 'boxy', bot: { shape: 'hexagon', shell: '#112233' } }, '#aabbcc')).toMatchObject({ shape: 'hexagon', shell: '#112233' })
  })

  it('travels inside the stored look and is compared by value', () => {
    const look = normalizeLook({ shape: 'round', seed: 'x', bot: { shape: 'square', screen: '#59d7ff' } })
    expect(look?.bot).toEqual({ shape: 'square', screen: '#59d7ff' })
    expect(agentLook({ id: 'a', look }).bot).toEqual({ shape: 'square', screen: '#59d7ff' })
    expect(sameBot({ shape: 'circle' }, { shape: 'circle' })).toBe(true)
    expect(sameBot({ shape: 'circle' }, undefined)).toBe(false)
  })

  it('surprise me stays inside the palettes', () => {
    const bot = randomBot('hexagon', () => 0.5)
    expect(bot.shape).toBe('hexagon')
    expect(bot.shell).toMatch(/^#[0-9a-f]{6}$/)
  })
})
