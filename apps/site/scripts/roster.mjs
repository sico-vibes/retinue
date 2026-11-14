// Builds content/roster.json and public/people/ from the desktop app's own employee presets, so the
// site always lists the same employees, photos, colours and personalities the app offers.
// Run from apps/site: node scripts/roster.mjs
import { readFileSync, writeFileSync, mkdirSync, copyFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const site = join(dirname(fileURLToPath(import.meta.url)), '..')
const desktop = join(site, '..', 'desktop', 'src')
const presets = JSON.parse(readFileSync(join(desktop, 'team', 'presets.json'), 'utf8'))
const personalities = JSON.parse(readFileSync(join(desktop, 'team', 'personalities.json'), 'utf8')).personalities
const colorSource = readFileSync(join(desktop, 'ui', 'agentColor.ts'), 'utf8')
const colors = Object.fromEntries([...colorSource.matchAll(/key: '([a-z]+)', hex: '(#[0-9A-Fa-f]{6})'/g)].map((match) => [match[1], match[2]]))

mkdirSync(join(site, 'public', 'people'), { recursive: true })
const departments = presets.departments.map((department) => ({ id: department.id, name: department.name }))
const people = presets.presets.map((preset) => {
  const personality = personalities.find((item) => item.id === preset.personality)
  copyFileSync(join(desktop, 'blob', 'photos', `${preset.photo}.webp`), join(site, 'public', 'people', `${preset.id}.webp`))
  return {
    id: preset.id,
    name: preset.nickname,
    role: preset.role,
    department: preset.department,
    color: colors[preset.color] ?? '#89D6B3',
    description: preset.description,
    tags: preset.tags,
    skills: preset.skills?.default ?? [],
    personality: personality ? { name: personality.name, summary: personality.summary, example: personality.example } : null,
    photo: `/people/${preset.id}.webp`,
  }
})
writeFileSync(join(site, 'content', 'roster.json'), JSON.stringify({ departments, people }, null, 2) + '\n')
console.log(`${people.length} employees in ${departments.length} departments`)
