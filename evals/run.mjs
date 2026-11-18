#!/usr/bin/env node
// Retinue's behavioural evals. Started by the owner, never by CI, a schedule or
// the app. See evals/README.md and docs/reports/wave3-evals.md.
//
//   node evals/run.mjs --fake
//   node evals/run.mjs --provider claude --repeats 3 --yes

import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import { CAPS, refuseWhenCi, validateMinutes, validateRepeats, validateSeconds } from './lib/caps.mjs'
import { assertModelAllowed, cliRuntime, passingThenFailing, scriptedRuntime } from './lib/runtime.mjs'
import { SCENARIOS } from './lib/scenarios.mjs'
import { runSuite } from './lib/suite.mjs'
import { buildResult, summaryLines } from './lib/report.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const PROVIDERS = ['claude', 'codex', 'opencode']
const INSTRUCTIONS_LIMIT = 200_000

const HELP = `Retinue behavioural evals

Usage: node evals/run.mjs --provider <claude|codex|opencode> [options]
       node evals/run.mjs --fake

  --fake                 scripted replies, no model, no usage; always allowed
  --provider <id>        claude (default), codex or opencode
  --model <id>           passed to the CLI; required for opencode
  --repeats <n>          repeats per scenario (default ${CAPS.repeats}, 1 to 10)
  --timeout <seconds>    limit per scenario attempt (default ${CAPS.attemptTimeoutMs / 1000})
  --total-minutes <n>    limit for the whole run (default ${CAPS.totalTimeoutMs / 60_000})
  --instructions <file>  wrap each scenario in this prompt text (never stored or printed)
  --out <dir>            where results go (default evals/results)
  --stop-file <path>     create this file to stop a run (default evals/.stop)
  --yes                  required for a real run: it spends the owner's usage
  --list                 print the scenarios and exit
  --help                 this text

Exit codes: 0 every attempt passed, 1 something failed, 2 refused or stopped early.`

function fail(message) {
  console.error(message)
  process.exit(2)
}

let parsed
try {
  parsed = parseArgs({
    options: {
      fake: { type: 'boolean', default: false },
      provider: { type: 'string' },
      model: { type: 'string' },
      repeats: { type: 'string' },
      timeout: { type: 'string' },
      'total-minutes': { type: 'string' },
      instructions: { type: 'string' },
      out: { type: 'string' },
      'stop-file': { type: 'string' },
      yes: { type: 'boolean', default: false },
      list: { type: 'boolean', default: false },
      help: { type: 'boolean', default: false },
    },
    allowPositionals: false,
  }).values
} catch (error) {
  fail(`${error.message}\n\n${HELP}`)
}

if (parsed.help) {
  console.log(HELP)
  process.exit(0)
}

if (parsed.list) {
  for (const scenario of SCENARIOS) console.log(`${scenario.id.padEnd(34)}${scenario.title}`)
  process.exit(0)
}

try {
  refuseWhenCi(process.env)
} catch {
  fail('Refusing to run: CI is set. These evals spend the owner\'s usage and never run in CI.')
}

let repeats
let attemptTimeoutMs
let totalTimeoutMs
try {
  repeats = validateRepeats(parsed.repeats ?? CAPS.repeats)
  attemptTimeoutMs = validateSeconds(parsed.timeout ?? CAPS.attemptTimeoutMs / 1000)
  totalTimeoutMs = validateMinutes(parsed['total-minutes'] ?? CAPS.totalTimeoutMs / 60_000)
} catch (error) {
  fail(`Refusing to run: ${error.code ?? 'a cap'} is outside the allowed range.\n\n${HELP}`)
}

const provider = parsed.fake ? 'fake' : (parsed.provider ?? 'claude')
if (!parsed.fake && !PROVIDERS.includes(provider)) fail(`Unknown provider "${provider}". Use ${PROVIDERS.join(', ')} or --fake.`)
if (!parsed.fake && provider === 'opencode' && !parsed.model) {
  fail('OpenCode needs --model: its default may be a pay-as-you-go Zen model, which Retinue never uses. Pass an opencode-go/ or -free model.')
}
try {
  assertModelAllowed(provider, parsed.model ?? null)
} catch {
  fail('Refusing to run: that OpenCode model is pay-as-you-go Zen. Use opencode-go/… or a -free model.')
}

const instructions = parsed.instructions ? readInstructions(resolve(parsed.instructions)) : null
const outDir = resolve(parsed.out ?? join(here, 'results'))
const stopFile = resolve(parsed['stop-file'] ?? join(here, '.stop'))
if (existsSync(stopFile)) fail('A stop file is present (evals/.stop by default).\nDelete it to run again.')

if (!parsed.fake && !parsed.yes) {
  const model = parsed.model ?? 'default model'
  console.log(
    `This would run ${SCENARIOS.length} scenarios x ${repeats} repeats on ${provider} (${model}) through your own CLI, spending your usage.`,
  )
  console.log('Add --yes to run it, or --fake to prove the harness without a model.')
  process.exit(2)
}

function readInstructions(path) {
  let text
  try {
    text = readFileSync(path, 'utf8')
  } catch {
    fail('Cannot read the instructions file: check the --instructions path.')
  }
  if (text.length > INSTRUCTIONS_LIMIT) fail(`The instructions file is over ${INSTRUCTIONS_LIMIT} characters; that is not a prompt.`)
  return text
}

const runtime = parsed.fake
  ? scriptedRuntime(passingThenFailing(SCENARIOS))
  : cliRuntime({ provider, model: parsed.model ?? null })

let interrupted = false
process.on('SIGINT', () => {
  if (interrupted) process.exit(2)
  interrupted = true
  console.log('\nStopping after the current attempt. Press Ctrl+C again to exit at once.')
  try {
    writeFileSync(stopFile, 'stop\n', 'utf8')
  } catch {
    // the suite's total cap still ends the run
  }
})

const caps = { attemptTimeoutMs, totalTimeoutMs, concurrency: CAPS.concurrency, repeats }
const run = await runSuite({
  scenarios: SCENARIOS,
  runtime,
  repeats,
  attemptTimeoutMs,
  totalTimeoutMs,
  stopFile,
  instructions,
  onAttempt: (row, attempt) => {
    process.stdout.write(`${row.id} ${attempt.attempt}/${row.repeats} ${attempt.outcome}\n`)
  },
})
if (interrupted) {
  try {
    rmSync(stopFile, { force: true })
  } catch {
    // leave it; the next run says it is there
  }
}

const result = buildResult({ run, caps })
const stamp = result.startedAt.replace(/[:.]/g, '-')
const json = `${JSON.stringify(result, null, 2)}\n`
const text = `${summaryLines(result).join('\n')}\n`
mkdirSync(outDir, { recursive: true })
for (const name of [`${stamp}.json`, 'latest.json']) writeFileSync(join(outDir, name), json, 'utf8')
for (const name of [`${stamp}.txt`, 'latest.txt']) writeFileSync(join(outDir, name), text, 'utf8')

console.log('')
process.stdout.write(text)
const shown = (file) => relative(process.cwd(), file) || file
console.log(`\nResults: ${shown(join(outDir, 'latest.json'))} and ${shown(join(outDir, 'latest.txt'))}`)

process.exitCode = result.stopped ? 2 : result.totals.failed + result.totals.timeout + result.totals.error > 0 ? 1 : 0
