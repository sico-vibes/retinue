// Multi-device and wave-3 screen helpers (17 November 2026): how another machine's freshness, OS and
// conversations read, the new-project dialog's names, the suggestion cards and the connector sign-in line.
import { describe, expect, it } from 'vitest'
import { machineFreshness, machineKindWord, normalizeMachineSessions, normalizeMachines } from './machines'
import { leafOf, repoNameOf } from './MachineRemote'
import { suggestionsFrom } from './ProjectSuggestions'
import { signInLine } from './ConnectorsPage'

describe('another machine', () => {
  it('reads kind and live from the link row, and defaults them for an older daemon', () => {
    const [current, older] = normalizeMachines({ machines: [
      { id: 'vps', name: 'VPS', state: 'online', kind: 'linux', live: true, bots: [] },
      { id: 'pc', name: 'Home PC', state: 'online', bots: [] },
    ] })
    expect(current).toMatchObject({ kind: 'linux', live: true })
    expect(older).toMatchObject({ kind: null, live: false })
  })

  it('says how fresh its rows are', () => {
    expect(machineFreshness({ state: 'online', live: true })).toBe('Live')
    expect(machineFreshness({ state: 'online', live: false })).toBe('Every 20 s')
    expect(machineFreshness({ state: 'offline', live: false })).toBe('Offline')
    expect(machineFreshness({ state: 'revoked', live: false })).toBe('Needs pairing again')
    expect(machineKindWord('macos')).toBe('macOS')
    expect(machineKindWord(null)).toBeNull()
  })

  it('keeps only complete conversation rows', () => {
    const rows = normalizeMachineSessions({ sessions: [
      { machineId: 'pc', machineName: 'Home PC', id: 's1', title: 'Fix it', agentId: 'a', state: 'working', archived: false },
      { machineId: 'pc', title: 'no id' },
      { id: 's2', title: 'no machine' },
    ] })
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ machineId: 'pc', id: 's1', title: 'Fix it', agentName: null, archived: false })
  })
})

describe('a project on another machine', () => {
  it('names a clone and a folder the way the machine will', () => {
    expect(repoNameOf('https://github.com/team/repo.git')).toBe('repo')
    expect(repoNameOf('git@github.com:team/korus.git')).toBe('korus')
    expect(repoNameOf('https://github.com/team/site/')).toBe('site')
    expect(leafOf('/home/owner/projects/retinue')).toBe('retinue')
    expect(leafOf('C:\\Users\\me\\Projects\\korus')).toBe('korus')
  })
})

describe('first tasks from the project', () => {
  it('keeps at most three complete suggestions', () => {
    const rows = suggestionsFrom({ suggestions: [
      { id: 'readme', kind: 'readme', title: 'Get oriented', prompt: 'Walk me through it.', source: { file: 'README.md', detail: 'heading: Korus' } },
      { id: 'bad', kind: 'script', title: 'No prompt' },
      { id: 'script', kind: 'script', title: 'Run the checks', prompt: 'Run them.', source: null },
      { id: 'commit', kind: 'commit', title: 'Pick up the last change', prompt: 'Read it.', source: null },
      { id: 'static-1', kind: 'static', title: 'First look', prompt: 'Read it.', source: null },
    ] })
    expect(rows.map((row) => row.id)).toEqual(['readme', 'script', 'commit'])
    expect(suggestionsFrom(null)).toEqual([])
  })
})

describe('connector sign-in', () => {
  const base = { id: 'c', auth: 'oauth' as const, detail: null, expiresAt: null }
  it('reads the state in words, with the daemon detail for an error', () => {
    expect(signInLine(null)).toBe('Checking the sign-in…')
    expect(signInLine({ ...base, status: 'idle' })).toBe('Not signed in yet')
    expect(signInLine({ ...base, status: 'pending' })).toMatch(/in your browser/)
    expect(signInLine({ ...base, status: 'authorized' })).toBe('Signed in')
    expect(signInLine({ ...base, status: 'error', detail: 'The sign-in was not completed in time. Try again.' })).toBe('The sign-in was not completed in time. Try again.')
  })
})
