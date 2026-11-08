import { describe, expect, it } from 'vitest'
import characters from '../blob/haze/characters.json'
import { AGENT_SWATCHES } from '../ui/agentColor'
import { DEPARTMENTS, PRESETS, presetMatches, presetsByDepartment } from './presets'

describe('preset employees', () => {
  it('fit the limits the daemon checks', () => {
    const portraits = new Set(characters.map((character) => character.id))
    const swatches = new Set<string>(AGENT_SWATCHES.map((swatch) => swatch.key))
    const departments = new Set(DEPARTMENTS.map((department) => department.id))
    expect(new Set(PRESETS.map((preset) => preset.id)).size).toBe(PRESETS.length)
    expect(new Set(PRESETS.map((preset) => preset.nickname.toLowerCase())).size).toBe(PRESETS.length)
    for (const preset of PRESETS) {
      expect(portraits.has(preset.portrait), preset.id).toBe(true)
      expect(swatches.has(preset.color), preset.id).toBe(true)
      expect(departments.has(preset.department), preset.id).toBe(true)
      expect(preset.role.length, preset.id).toBeLessThanOrEqual(40)
      expect(preset.description.length, preset.id).toBeLessThanOrEqual(255)
      expect(preset.tags.length, preset.id).toBeLessThanOrEqual(8)
      for (const tag of preset.tags) expect(tag, preset.id).toMatch(/^[a-z0-9][a-z0-9-]{0,23}$/)
      expect(preset.instructions.length, preset.id).toBeGreaterThan(2)
    }
  })

  it('uses each portrait at most once, leaving Kratos for the CEO', () => {
    expect(new Set(PRESETS.map((preset) => preset.portrait)).size).toBe(PRESETS.length)
    expect(PRESETS.some((preset) => preset.portrait === 'char-kratos')).toBe(false)
  })

  it('groups by department and searches roles and tags', () => {
    const groups = presetsByDepartment(PRESETS)
    expect(groups.map((group) => group.department.id)).toEqual(DEPARTMENTS.map((department) => department.id))
    expect(groups.reduce((sum, group) => sum + group.presets.length, 0)).toBe(PRESETS.length)
    const security = PRESETS.filter((preset) => presetMatches(preset, 'security'))
    expect(security.map((preset) => preset.id)).toContain('jo')
    expect(PRESETS.filter((preset) => presetMatches(preset, '  ')).length).toBe(PRESETS.length)
  })
})
