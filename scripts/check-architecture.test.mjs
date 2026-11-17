// The architecture check's own tests: a rule that cannot be shown to fail is a
// rule nobody should trust. Run with `node --test scripts/` (or `npm test`).
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import assert from 'node:assert/strict'
import { test } from 'node:test'

import { checkArchitecture } from './check-architecture.mjs'

const repository = fileURLToPath(new URL('..', import.meta.url))

/** A throwaway tree with the given files (`path` → `text`), removed afterwards. */
function fixture(files, run) {
  const root = mkdtempSync(join(tmpdir(), 'retinue-architecture-'))
  try {
    for (const [path, text] of Object.entries(files)) {
      const full = join(root, path)
      mkdirSync(dirname(full), { recursive: true })
      writeFileSync(full, text)
    }
    return run(root)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

const ruleOf = (failures, rule) => failures.filter((failure) => failure.rule === rule)

test('the repository passes every boundary', () => {
  const { failures } = checkArchitecture({ root: repository })
  assert.deepEqual(failures, [])
})

test('a crate may not name a crate outside its row', () => {
  const manifest = '[package]\nname = "retinue-storage"\n\n[dependencies]\nretinue-usage = { path = "../retinue-usage" }\nretinue-daemon = { path = "../retinue-daemon" }\n'
  fixture({ 'crates/retinue-storage/Cargo.toml': manifest }, (root) => {
    const { failures } = checkArchitecture({ root })
    const problems = ruleOf(failures, 'crate-table')
    assert.equal(problems.length, 1)
    assert.match(problems[0].message, /retinue-daemon/)
    assert.equal(problems[0].path, 'crates/retinue-storage')
  })
})

test('a new crate has to be written into the table', () => {
  fixture({ 'crates/retinue-newcomer/Cargo.toml': '[package]\nname = "retinue-newcomer"\n' }, (root) => {
    const problems = ruleOf(checkArchitecture({ root }).failures, 'crate-table')
    assert.equal(problems.length, 1)
    assert.match(problems[0].message, /new crate/)
  })
})

test('an HTTP client outside the daemon, speech and the shell fails', () => {
  fixture({ 'crates/retinue-usage/Cargo.toml': '[package]\nname = "retinue-usage"\n\n[dependencies]\nreqwest = "0.12"\n' }, (root) => {
    const problems = ruleOf(checkArchitecture({ root }).failures, 'http-client')
    assert.equal(problems.length, 1)
    assert.match(problems[0].message, /reqwest/)
  })
})

test('an allowlisted module keeps its client, a new one does not', () => {
  fixture(
    {
      'crates/retinue-daemon/src/browser.rs': 'fn client() -> reqwest::Client { reqwest::Client::new() }\n',
      'crates/retinue-daemon/src/newcomer.rs': 'fn client() -> reqwest::Client { reqwest::Client::new() }\n',
    },
    (root) => {
      const problems = ruleOf(checkArchitecture({ root }).failures, 'daemon-network')
      assert.equal(problems.length, 1)
      assert.match(problems[0].path, /newcomer\.rs$/)
    },
  )
})

test('test files may hold a client of their own', () => {
  fixture({ 'crates/retinue-daemon/src/example_tests.rs': 'fn client() -> reqwest::Client { reqwest::Client::new() }\n' }, (root) => {
    assert.deepEqual(ruleOf(checkArchitecture({ root }).failures, 'daemon-network'), [])
  })
})

test('production code may not open a WebSocket of its own', () => {
  fixture(
    {
      'apps/desktop/src/ui/own.ts': "export const socket = () => new WebSocket('ws://127.0.0.1:1')\n",
      'apps/desktop/src/ui/own.test.ts': "export const socket = () => new WebSocket('ws://127.0.0.1:1')\n",
    },
    (root) => {
      const problems = ruleOf(checkArchitecture({ root }).failures, 'websocket')
      assert.equal(problems.length, 1)
      assert.match(problems[0].path, /ui\/own\.ts$/)
    },
  )
})

test('production code may not import the preview harness', () => {
  fixture(
    {
      'apps/desktop/src/ui/leaks.ts': "import { bots } from '../preview/fixtureBridge'\nexport const count = bots.length\n",
      'apps/desktop/src/ui/counts.test.ts': "import { bots } from '../preview/fixtureBridge'\nexport const count = bots.length\n",
      'apps/desktop/src/preview/fixtureBridge.ts': 'export const bots: unknown[] = []\n',
    },
    (root) => {
      const problems = ruleOf(checkArchitecture({ root }).failures, 'preview-leak')
      assert.equal(problems.length, 1)
      assert.match(problems[0].path, /ui\/leaks\.ts$/)
    },
  )
})

test('an allowlist entry that matches nothing fails', () => {
  fixture({}, (root) => {
    const problems = ruleOf(checkArchitecture({ root }).failures, 'stale-allowlist')
    // Every exception is reported when nothing in the tree needs it.
    assert.ok(problems.length > 0)
    assert.match(problems[0].message, /matched nothing/)
  })
})
