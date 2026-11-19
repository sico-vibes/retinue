import { describe, expect, it } from 'vitest'
import { splitProjectTags } from './projectMentions'
import type { Project } from '../types'

const projects = [{ id: 'p1', name: 'Retinue' }, { id: 'p2', name: 'Retinue site' }] as Project[]

describe('project tags in a message', () => {
  it('finds whole project names after #, longest first', () => {
    expect(splitProjectTags('@Iris #Retinue site look at this', projects).map((part) => [part.text, part.project?.id ?? null])).toEqual([['@Iris ', null], ['#Retinue site', 'p2'], [' look at this', null]])
    expect(splitProjectTags('#Retinue, please', projects)[0]).toMatchObject({ project: { id: 'p1' } })
  })
  it('leaves a # inside a word or an unknown name as text', () => {
    expect(splitProjectTags('issue#Retinue and #Other', projects)).toEqual([{ text: 'issue#Retinue and #Other' }])
    expect(splitProjectTags('#Retinues', projects)).toEqual([{ text: '#Retinues' }])
  })
})
