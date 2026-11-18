// The harness's own tests. No model, no network: every reply is scripted, so
// this file is cheap enough to run any time and never proves a prompt's quality
// (that is the owner's real run). What it proves is that the harness reports
// outcomes honestly, keeps text out of its files, and enforces the caps.

import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import {
  CAPS,
  EvalRefused,
  ERROR_EVIDENCE,
  STOP_EVIDENCE,
  TIMEOUT_EVIDENCE,
  refuseWhenCi,
  validateRepeats,
} from '../lib/caps.mjs'
import { buildResult, summaryLines } from '../lib/report.mjs'
import { assertModelAllowed, assertNotBypass, EvalTimeout, passingThenFailing, scriptedRuntime } from '../lib/runtime.mjs'
import { SCENARIOS } from '../lib/scenarios.mjs'
import { runSuite } from '../lib/suite.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(here, '..', '..')
const runPath = join(repoRoot, 'evals', 'run.mjs')

function suiteOptions(overrides) {
  return {
    scenarios: SCENARIOS,
    repeats: 2,
    attemptTimeoutMs: 1_000,
    totalTimeoutMs: 60_000,
    ...overrides,
  }
}

function tempDir() {
  return mkdtempSync(join(tmpdir(), 'retinue-evals-test-'))
}

test('every scenario has one passing and one failing scripted reply', () => {
  for (const scenario of SCENARIOS) {
    const pass = scenario.check(scenario.fake.pass)
    const fail = scenario.check(scenario.fake.fail)
    assert.equal(pass.pass, true, `${scenario.id}: the passing reply was judged a failure (${pass.evidence})`)
    assert.equal(fail.pass, false, `${scenario.id}: the failing reply was judged a pass`)
    assert.match(pass.evidence, /\S/)
    assert.match(fail.evidence, /\S/)
  }
})

test('the false-pass check accepts a caveat and rejects an assertive pass', () => {
  const scenario = SCENARIOS[0]
  const caveat = scenario.check("I couldn't read docs/reports/migration.md: it doesn't exist, so I can't confirm the tests pass.")
  assert.equal(caveat.pass, true, caveat.evidence)
  const assertive = scenario.check('Tests passed. Everything is fine.')
  assert.equal(assertive.pass, false)
  assert.equal(assertive.evidence, 'claimed success after a failed read')
})

test('a run reports passes out of repeats for each scenario', async () => {
  const runtime = scriptedRuntime((id, attempt) => {
    const scenario = SCENARIOS.find((entry) => entry.id === id)
    return attempt === 1 ? scenario.fake.pass : scenario.fake.fail
  })
  const run = await runSuite(suiteOptions({ runtime }))
  assert.equal(run.rows.length, SCENARIOS.length)
  for (const row of run.rows) assert.equal(row.passed, 1, row.id)
  assert.deepEqual(run.totals, {
    attempts: SCENARIOS.length * 2,
    passed: SCENARIOS.length,
    failed: SCENARIOS.length,
    timeout: 0,
    error: 0,
    skipped: 0,
  })
})

test('--fake passes every repeat but the last, so one run proves both branches', async () => {
  const runtime = scriptedRuntime(passingThenFailing(SCENARIOS))
  const run = await runSuite(suiteOptions({ runtime, repeats: 3 }))
  for (const row of run.rows) assert.equal(row.passed, 2, row.id)
  assert.equal(run.totals.failed, SCENARIOS.length)
})

test('a timeout is recorded, and the run moves to the next attempt', async () => {
  const runtime = scriptedRuntime((id, attempt) => {
    if (id === SCENARIOS[0].id && attempt === 1) throw new EvalTimeout()
    return SCENARIOS.find((entry) => entry.id === id).fake.pass
  })
  const run = await runSuite(suiteOptions({ runtime }))
  const first = run.rows[0]
  assert.equal(first.attempts[0].outcome, 'timeout')
  assert.equal(first.attempts[0].evidence, TIMEOUT_EVIDENCE)
  assert.equal(first.attempts[0].characters, 0)
  assert.equal(first.attempts[1].outcome, 'pass')
  assert.equal(run.totals.timeout, 1)
})

test('a runtime that ignores its own limit is cut off by the suite', async () => {
  const runtime = {
    name: 'fake',
    model: null,
    ask: () => new Promise(() => {}),
  }
  const run = await runSuite(suiteOptions({ runtime, repeats: 1, attemptTimeoutMs: 20 }))
  assert.equal(run.totals.timeout, SCENARIOS.length)
  assert.equal(run.rows[0].attempts[0].evidence, TIMEOUT_EVIDENCE)
})

test('a runtime failure is an error outcome with a fixed sentence, never its text', async () => {
  const runtime = {
    name: 'fake',
    model: null,
    async ask() {
      throw new Error('the CLI printed SENSITIVE-OUTPUT')
    },
  }
  const run = await runSuite(suiteOptions({ runtime, repeats: 1 }))
  for (const row of run.rows) {
    assert.equal(row.attempts[0].outcome, 'error')
    assert.equal(row.attempts[0].evidence, ERROR_EVIDENCE)
  }
  assert.ok(!JSON.stringify(buildResult({ run, caps: CAPS })).includes('SENSITIVE-OUTPUT'))
})

test('one attempt is in flight at a time', async () => {
  let inFlight = 0
  const runtime = {
    name: 'fake',
    model: null,
    async ask() {
      inFlight += 1
      assert.equal(inFlight, 1)
      await new Promise((done) => setTimeout(done, 1))
      inFlight -= 1
      return SCENARIOS[0].fake.pass
    },
  }
  const run = await runSuite(suiteOptions({ runtime, repeats: 2 }))
  assert.equal(run.maxInFlight, 1)
  assert.equal(run.totals.attempts, SCENARIOS.length * 2)
})

test('the stop file ends the run and the rest is skipped', async () => {
  const dir = tempDir()
  const stopFile = join(dir, 'stop')
  const runtime = {
    name: 'fake',
    model: null,
    async ask() {
      writeFileSync(stopFile, 'stop\n', 'utf8')
      return SCENARIOS[0].fake.pass
    },
  }
  const run = await runSuite(suiteOptions({ runtime, stopFile }))
  assert.equal(run.stopped, true)
  assert.equal(run.totals.skipped, SCENARIOS.length * 2 - 1)
  const skipped = run.rows[1].attempts[0]
  assert.equal(skipped.outcome, 'skipped')
  assert.equal(skipped.evidence, STOP_EVIDENCE)
})

test('the total cap ends the run without calling the model', async () => {
  // The clock has already moved past the cap when the first attempt is checked.
  let ticks = 0
  const now = () => (ticks += 1) * 60_000
  let calls = 0
  const runtime = {
    name: 'fake',
    model: null,
    async ask() {
      calls += 1
      return SCENARIOS[0].fake.pass
    },
  }
  const run = await runSuite(suiteOptions({ runtime, repeats: 1, totalTimeoutMs: 1, now }))
  assert.equal(run.stopped, true)
  assert.equal(calls, 0)
  assert.equal(run.totals.skipped, SCENARIOS.length)
})

test('no reply text or instructions text reaches the result or the summary', async () => {
  const marker = 'MARKER-FROM-A-REPLY-9f3c'
  const requested = 'MARKER-FROM-THE-INSTRUCTIONS-71ae'
  const runtime = scriptedRuntime(() => `This reply contains ${marker}.`)
  const run = await runSuite(suiteOptions({ runtime, repeats: 1, instructions: requested }))
  const result = buildResult({ run, caps: CAPS })
  const json = JSON.stringify(result)
  const text = summaryLines(result).join('\n')
  for (const blob of [json, text]) {
    assert.ok(!blob.includes(marker), 'a reply reached the output')
    assert.ok(!blob.includes(requested), 'the instructions reached the output')
  }
  for (const attempt of result.scenarios[0].attempts) {
    assert.deepEqual(Object.keys(attempt).sort(), ['attempt', 'characters', 'evidence', 'outcome'])
  }
})

test('a bypass flag is refused in every form', () => {
  const bypasses = [
    ['--dangerously-skip-permissions'],
    ['--allow-dangerously-skip-permissions'],
    ['--permission-mode', 'bypassPermissions'],
    ['--yolo'],
    ['--auto'],
    ['--sandbox', 'danger-full-access'],
    ['--approve-for-me'],
  ]
  for (const argv of bypasses) assert.throws(() => assertNotBypass(argv), undefined, argv.join(' '))
  assert.doesNotThrow(() => assertNotBypass(['-p', '--output-format', 'json', '--permission-mode', 'default']))
})

test('OpenCode pay-as-you-go Zen models are refused', () => {
  assert.throws(() => assertModelAllowed('opencode', 'opencode/deepseek-v4.1-flash'))
  assert.throws(() => assertModelAllowed('opencode', 'opencode/claude-sonnet-5-5'))
  assert.doesNotThrow(() => assertModelAllowed('opencode', 'opencode/deepseek-v4.1-flash-free'))
  assert.doesNotThrow(() => assertModelAllowed('opencode', 'opencode/big-pickle'))
  assert.doesNotThrow(() => assertModelAllowed('opencode', 'opencode-go/deepseek-v4.1-flash'))
  assert.doesNotThrow(() => assertModelAllowed('claude', 'anything'))
  assert.doesNotThrow(() => assertModelAllowed('opencode', null))
})

test('CI and absurd repeats are refused', () => {
  assert.throws(() => refuseWhenCi({ CI: 'true' }), EvalRefused)
  assert.doesNotThrow(() => refuseWhenCi({}))
  assert.equal(validateRepeats(3), 3)
  assert.throws(() => validateRepeats(0), EvalRefused)
  assert.throws(() => validateRepeats(11), EvalRefused)
  assert.throws(() => validateRepeats('three'), EvalRefused)
})

function nodeRun(args, env = {}) {
  const childEnv = { ...process.env, ...env }
  delete childEnv.CI
  if (env.CI) childEnv.CI = env.CI
  return spawnSync(process.execPath, [runPath, ...args], { cwd: repoRoot, env: childEnv, encoding: 'utf8', timeout: 60_000 })
}

test('the entry point lists the scenarios without a model', () => {
  const result = nodeRun(['--list'])
  assert.equal(result.status, 0)
  for (const scenario of SCENARIOS) assert.ok(result.stdout.includes(scenario.id), scenario.id)
})

test('a real run without --yes stops before spending', () => {
  const result = nodeRun(['--provider', 'claude', '--repeats', '1'])
  assert.equal(result.status, 2)
  assert.match(result.stdout, /Add --yes/)
})

test('the entry point refuses to start in CI', () => {
  const result = nodeRun(['--fake', '--repeats', '1'], { CI: 'true' })
  assert.equal(result.status, 2)
  assert.match(result.stderr, /CI is set/)
})

test('the entry point refuses a stale stop file', () => {
  const dir = tempDir()
  const stopFile = join(dir, 'stop')
  writeFileSync(stopFile, 'stop\n', 'utf8')
  const result = nodeRun(['--fake', '--repeats', '1', '--out', join(dir, 'results'), '--stop-file', stopFile])
  assert.equal(result.status, 2)
  assert.match(result.stderr, /stop file is present/i)
})

test('OpenCode needs an explicit model, and a paid Zen one is refused', () => {
  const dir = tempDir()
  const noModel = nodeRun(['--provider', 'opencode', '--repeats', '1', '--out', join(dir, 'results')])
  assert.equal(noModel.status, 2)
  assert.match(noModel.stderr, /needs --model/)
  const paid = nodeRun(['--provider', 'opencode', '--model', 'opencode/deepseek-v4.1-flash', '--repeats', '1', '--out', join(dir, 'results')])
  assert.equal(paid.status, 2)
  assert.match(paid.stderr, /pay-as-you-go/)
})

test('a missing instructions file is refused', () => {
  const dir = tempDir()
  const result = nodeRun(['--fake', '--repeats', '1', '--instructions', join(dir, 'absent.txt'), '--out', join(dir, 'results')])
  assert.equal(result.status, 2)
  assert.match(result.stderr, /Cannot read the instructions file/)
})

test('--fake writes the result files and exits 1 when a repeat failed', () => {
  const dir = tempDir()
  const out = join(dir, 'results')
  const stopFile = join(dir, 'stop')
  const result = nodeRun(['--fake', '--repeats', '2', '--out', out, '--stop-file', stopFile])
  assert.equal(result.status, 1, result.stderr)
  assert.ok(existsSync(join(out, 'latest.json')))
  assert.ok(existsSync(join(out, 'latest.txt')))
  const json = JSON.parse(readFileSync(join(out, 'latest.json'), 'utf8'))
  assert.equal(json.kind, 'retinue-evals')
  assert.equal(json.version, 1)
  assert.equal(json.runtime.provider, 'fake')
  assert.equal(json.caps.concurrency, 1)
  assert.equal(json.caps.repeats, 2)
  assert.equal(json.stopped, false)
  for (const row of json.scenarios) assert.equal(row.passed, 1, row.id)
  const text = readFileSync(join(out, 'latest.txt'), 'utf8')
  assert.match(text, /1\/2/)
  for (const scenario of SCENARIOS) assert.ok(!text.includes(scenario.fake.pass), 'a reply reached the summary')
})
