// The employees the app offers to hire, built from its own presets by scripts/roster.mjs.
import data from './roster.json'

export type Person = (typeof data.people)[number]
export type Department = (typeof data.departments)[number]

export const people: Person[] = data.people
export const departments: Department[] = data.departments
export const byId = (id: string) => people.find((person) => person.id === id) ?? null
export const departmentName = (id: string) => departments.find((department) => department.id === id)?.name ?? ''

/** Skill ids as people would say them: "systematic-debugging" becomes "Systematic debugging". */
export const skillLabel = (id: string) => (id.charAt(0).toUpperCase() + id.slice(1)).replace(/-/g, ' ').replace(/\btdd\b/i, 'TDD').replace(/\bapi\b/i, 'API')
