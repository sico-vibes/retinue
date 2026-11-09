import { describe, expect, it } from 'vitest'
import { migrateLegacyLocalStorage } from './legacyNames'

// This file names the old keys on purpose; scripts/rebrand.py leaves it alone.

function fakeStorage(entries: Record<string, string>): Storage {
  const data = new Map(Object.entries(entries))
  return {
    get length() { return data.size },
    key: (index: number) => [...data.keys()][index] ?? null,
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => { data.set(key, String(value)) },
    removeItem: (key: string) => { data.delete(key) },
    clear: () => data.clear(),
  } as Storage
}

function dump(storage: Storage): Record<string, string> {
  const out: Record<string, string> = {}
  for (let index = 0; index < storage.length; index += 1) {
    const key = storage.key(index)!
    out[key] = storage.getItem(key)!
  }
  return out
}

describe('legacy browser-cache keys', () => {
  it('moves old keys to the new prefix and renames the renamed look ids', () => {
    const storage = fakeStorage({ 'bitbots.style': 'blobots', 'bloblex.bitbots': '0', 'bitbots.accent': '#fff0aa' })
    migrateLegacyLocalStorage(storage)
    expect(dump(storage)).toEqual({ 'retinue.style': 'stone', 'retinue.preStyle': '0', 'retinue.accent': '#fff0aa' })
  })

  it('keeps a value the new key already has and drops the old one', () => {
    const storage = fakeStorage({ 'retinue.style': 'haze', 'bitbots.style': 'blobots' })
    migrateLegacyLocalStorage(storage)
    expect(dump(storage)).toEqual({ 'retinue.style': 'haze' })
  })

  it('relabels a look id inside a JSON value', () => {
    const storage = fakeStorage({ 'bloblex.appearance': '{"style":"blobots","accent":""}' })
    migrateLegacyLocalStorage(storage)
    expect(dump(storage)).toEqual({ 'retinue.appearance': '{"style":"stone","accent":""}' })
  })

  it('leaves unrelated keys alone', () => {
    const storage = fakeStorage({ 'other.key': 'value' })
    migrateLegacyLocalStorage(storage)
    expect(dump(storage)).toEqual({ 'other.key': 'value' })
  })
})
