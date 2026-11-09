import assert from 'node:assert/strict'
import test from 'node:test'
import { MARKERS, UNKNOWN, patchBundleType } from './patch-bundle-type.mjs'

const exe = (...parts) => Buffer.concat(parts.map((part) => (typeof part === 'string' ? Buffer.from(part) : part)))

test('marks the executable as NSIS in place, keeping its length', () => {
  const input = exe('MZ head ', UNKNOWN, ' tail')
  const out = patchBundleType(input)
  assert.equal(out.length, input.length)
  assert.ok(out.includes(MARKERS.nsis))
  assert.ok(!out.includes(UNKNOWN))
  assert.ok(input.includes(UNKNOWN), 'the input buffer is not changed')
})

test('can mark an MSI build', () => {
  assert.ok(patchBundleType(exe('x', UNKNOWN), 'msi').includes(MARKERS.msi))
})

test('refuses an executable that is already marked', () => {
  assert.throws(() => patchBundleType(exe('x', MARKERS.nsis)), /already marked/)
})

test('refuses a file without the marker', () => {
  assert.throws(() => patchBundleType(exe('not tauri')), /not found/)
})

test('refuses a repeated marker and an unknown type', () => {
  assert.throws(() => patchBundleType(exe(UNKNOWN, UNKNOWN)), /more than once/)
  assert.throws(() => patchBundleType(exe(UNKNOWN), 'dmg'), /Unknown bundle type/)
})
