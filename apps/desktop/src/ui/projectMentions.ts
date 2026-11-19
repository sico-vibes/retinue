// `#Project` in a sent message (owner, 15 November 2026): shown as the project's
// icon and name, the way `@Employee` shows a photo and a name.
import { createContext } from 'react'
import type { Project } from '../types'

/** The projects a message can name; App provides them around the transcript. */
export const ProjectsContext = createContext<readonly Project[]>([])

export type ProjectSegment = { text: string; project?: Project }

/** Splits text into plain parts and `#Name` project tags (longest names win, whole words only). */
export function splitProjectTags(text: string, projects: readonly Project[]): ProjectSegment[] {
  const named = projects.filter((project) => project.name?.trim()).sort((a, b) => b.name.length - a.name.length)
  if (!named.length || !text.includes('#')) return [{ text }]
  const lower = text.toLowerCase()
  const segments: ProjectSegment[] = []
  let plainStart = 0
  let index = 0
  while (index < text.length) {
    const at = text.indexOf('#', index)
    if (at < 0) break
    const before = at === 0 ? '' : text[at - 1]!
    const project = /[\p{L}\p{N}_]/u.test(before) ? undefined : named.find((candidate) => {
      const name = candidate.name.toLowerCase()
      if (!lower.startsWith(name, at + 1)) return false
      const after = text[at + 1 + name.length]
      return after === undefined || !/[\p{L}\p{N}_]/u.test(after)
    })
    if (!project) { index = at + 1; continue }
    if (at > plainStart) segments.push({ text: text.slice(plainStart, at) })
    const end = at + 1 + project.name.length
    segments.push({ text: text.slice(at, end), project })
    plainStart = index = end
  }
  if (plainStart < text.length) segments.push({ text: text.slice(plainStart) })
  return segments
}
