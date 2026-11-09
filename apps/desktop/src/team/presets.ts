// The preset employees Retinue ships (owner, 8 November 2026). The same JSON
// is read by the daemon (`include_str!`), so the catalogue has one source.
import catalogue from './presets.json'
import { asPhoto } from '../blob/photos'

export type Department = { id: string; name: string }
export type Preset = {
  id: string
  nickname: string
  portrait: string
  /** The profile photo the employee wears (`photo-NN`). */
  photo: string
  department: string
  role: string
  color: string
  tags: string[]
  description: string
  instructions: string[]
  /** From `preset.list`: the employee made from this preset, when hired. */
  hiredAgentId?: string | null
}

export const DEPARTMENTS: readonly Department[] = catalogue.departments
export const PRESETS: readonly Preset[] = catalogue.presets

/** The look a preset employee wears before and after hiring: its profile photo. */
export function presetLook(preset: Pick<Preset, 'photo'>) {
  return { shape: 'circle' as const, photo: asPhoto(preset.photo) }
}

/** Presets grouped by department, in catalogue order. */
export function presetsByDepartment(presets: readonly Preset[], departments: readonly Department[] = DEPARTMENTS): Array<{ department: Department; presets: Preset[] }> {
  return departments.map((department) => ({ department, presets: presets.filter((preset) => preset.department === department.id) })).filter((group) => group.presets.length > 0)
}

/** A case-insensitive match on nickname, role, department name, tags and description. */
export function presetMatches(preset: Preset, query: string, departments: readonly Department[] = DEPARTMENTS): boolean {
  const needle = query.trim().toLowerCase()
  if (!needle) return true
  const department = departments.find((item) => item.id === preset.department)?.name ?? ''
  return [preset.nickname, preset.role, department, preset.description, ...preset.tags].some((value) => value.toLowerCase().includes(needle))
}
