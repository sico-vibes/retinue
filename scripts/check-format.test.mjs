// The format check's own tests: each rule is shown catching the mistake it
// exists for, and the allowlist is shown staying honest.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { checkFormat } from './check-format.mjs'

const rules = (failures) => failures.map((failure) => failure.rule)
// A subset never judges the allowlist; only a whole-tree pass does.
const subset = (files) => checkFormat({ files, full: false })

test('a clean file passes', () => {
  const bytes = Buffer.from('const a = 1\n')
  assert.deepEqual(subset([{ path: 'src/a.ts', bytes }]).failures, [])
})

test('a byte-order mark fails', () => {
  const bytes = Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from('const a = 1\n')])
  assert.deepEqual(rules(subset([{ path: 'src/a.ts', bytes }]).failures), ['bom'])
})

test('trailing whitespace fails, and the line is named', () => {
  const bytes = Buffer.from('let a = 1; \nlet b = 2;\n')
  const { failures } = subset([{ path: 'crates/x/src/a.rs', bytes }])
  assert.deepEqual(rules(failures), ['trailing-whitespace'])
  assert.match(failures[0].message, /line 1/)
})

test('mixed line endings fail', () => {
  const bytes = Buffer.from('let a = 1;\r\nlet b = 2;\n')
  assert.deepEqual(rules(subset([{ path: 'crates/x/src/a.rs', bytes }]).failures), ['line-endings'])
})

test('bytes that are not UTF-8 fail', () => {
  const bytes = Buffer.concat([Buffer.from('let a = '), Buffer.from([0xff]), Buffer.from(';\n')])
  assert.deepEqual(rules(subset([{ path: 'crates/x/src/a.rs', bytes }]).failures), ['encoding'])
})

test('a NUL byte fails, except in the one file that tests for one', () => {
  const bytes = Buffer.from("let a = '\u0000';\n")
  assert.deepEqual(rules(subset([{ path: 'crates/x/src/a.rs', bytes }]).failures), ['nul'])
  const allowed = subset([{ path: 'crates/retinue-storage/src/lib.rs', bytes }]).failures
  assert.deepEqual(allowed, [])
})

test('markdown keeps its trailing spaces, and is not read at all', () => {
  const bytes = Buffer.from('a hard break  \nnext line\n')
  assert.deepEqual(subset([{ path: 'docs/guide.md', bytes }]).failures, [])
})

test('an allowlist entry that matched nothing fails a whole-tree pass', () => {
  const bytes = Buffer.from('const a = 1\n')
  const full = checkFormat({ files: [{ path: 'src/a.ts', bytes }] })
  assert.deepEqual(rules(full.failures), ['stale-allowlist'])
  // The same subset under `full: false` says nothing about the allowlist.
  assert.deepEqual(subset([{ path: 'src/a.ts', bytes }]).failures, [])
})
