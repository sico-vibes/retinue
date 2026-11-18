// Which CI checks a change needs (wave 3, `docs/plans/nod-adoption-plan.md`,
// release safety; the design is `docs/research/n8n-nod/07-architecture.md`
// idea 4).
//
// The single 90-minute job this replaces made the whole gate binary: one known
// red test closed every check behind it, and a docs-only change waited an hour
// for the answer. This table says which job a changed path can reach, and it
// fails closed: a path it does not know, a lockfile, a toolchain file, a build
// script under `.github/` or `scripts/`, or a listing it could not read, runs
// everything. `checks` in the workflow is the one required job, and it fails
// unless every selected check passed, so a bad table cannot let a change
// through untested (`.github/workflows/ci.yml`).
//
// A new gate belongs here in the same change: add its name to CHECKS, a rule
// that selects it, and a job in the workflow. `scripts/ci-scope.test.mjs` pins
// that every check is selected by something and that unknown paths run
// everything.
//
// Usage:
//   node scripts/ci-scope.mjs --paths-file changed.txt
//   node scripts/ci-scope.mjs --paths a/b.ts c.rs
//   node scripts/ci-scope.mjs --full                     # a nightly run: everything
//   node scripts/ci-scope.mjs --paths-file changed.txt --github-output

import { appendFileSync, readFileSync } from 'node:fs'

/** Every check the workflow has a job for, in the order the workflow runs them. */
export const CHECKS = ['hygiene', 'types', 'uiTests', 'build', 'daemonTests', 'clippy', 'docs', 'perf']

/**
 * Paths that make the whole gate run, whatever else changed. These are the
 * files whose effect cannot be attributed to one job: the dependency and
 * toolchain locks (every job), the workflow and the checks themselves (the
 * selection), and the native shell and packaging, which no CI job builds.
 */
const EVERYTHING = [
  /^package\.json$/,
  /^package-lock\.json$/,
  /^Cargo\.toml$/,
  /^Cargo\.lock$/,
  /^rust-toolchain\.toml$/,
  /^perf-budgets\.json$/,
  /^\.github\//,
  /^scripts\//,
  /^apps\/desktop\/src-tauri\//,
  /^packaging\//,
  /^\.signpath\//,
]

/** `[check, pattern, why]`. The first `why` that matched is the job's reason. */
const RULES = [
  ['types', /^apps\/desktop\/(?:src\/|index\.html$|vite[^/]*\.config\.ts$|tsconfig[^/]*\.json$|package\.json$)/, 'the desktop app'],
  ['uiTests', /^apps\/desktop\/(?:src\/|index\.html$|vite[^/]*\.config\.ts$|tsconfig[^/]*\.json$|package\.json$)/, 'the desktop app'],
  ['build', /^apps\/desktop\/(?:src\/|index\.html$|vite[^/]*\.config\.ts$|tsconfig[^/]*\.json$|package\.json$)/, 'the desktop app'],
  ['build', /^assets\//, 'the app icons'],
  ['daemonTests', /^(?:crates|tools)\//, 'the Rust crates'],
  ['clippy', /^(?:crates|tools)\//, 'the Rust crates'],
  ['perf', /^(?:crates|tools)\//, 'the Rust crates'],
  ['perf', /^apps\/desktop\/src\//, 'the desktop app'],
  ['docs', /^docs\//, 'the documentation'],
  ['docs', /^[^/]+\.md$/, 'a document at the repository root'],
  ['docs', /^\.claude\//, 'the skill'],
  // No gate covers the website apps; the library check is the one that runs.
  ['docs', /^apps\/(?:site|website)\//, 'a website app'],
]

/**
 * The checks a set of changed paths needs.
 *
 * `full` is a scheduled, manual or first run, or a listing that could not be
 * read; either way every check runs. An empty list is `full` too: a run that
 * cannot say what changed must not let it through with less.
 */
export function selectChecks({ paths, full = false } = {}) {
  const list = (paths ?? []).map((path) => path.replaceAll('\\', '/').trim()).filter(Boolean)
  if (full || list.length === 0) {
    return {
      full: true,
      checks: Object.fromEntries(CHECKS.map((check) => [check, true])),
      reason: full ? 'a full run was asked for' : 'no changed paths were given',
      unknown: [],
      everything: [],
    }
  }
  const everything = list.filter((path) => EVERYTHING.some((pattern) => pattern.test(path)))
  if (everything.length) {
    return {
      full: true,
      checks: Object.fromEntries(CHECKS.map((check) => [check, true])),
      reason: `${everything[0]} reaches every job`,
      unknown: [],
      everything,
    }
  }
  const checks = Object.fromEntries(CHECKS.map((check) => [check, false]))
  // The fast checks cost about two minutes and need no toolchain; they always run.
  checks.hygiene = true
  const reasons = new Set(['the fast checks always run'])
  const unknown = []
  for (const path of list) {
    const matched = RULES.filter(([, pattern]) => pattern.test(path))
    if (!matched.length) {
      unknown.push(path)
      continue
    }
    for (const [check, , why] of matched) {
      checks[check] = true
      reasons.add(why)
    }
  }
  if (unknown.length) {
    return {
      full: true,
      checks: Object.fromEntries(CHECKS.map((check) => [check, true])),
      reason: `${unknown[0]} is not in the table, so everything runs`,
      unknown,
      everything: [],
    }
  }
  return { full: false, checks, reason: [...reasons].join('; '), unknown: [], everything: [] }
}

function parseArgs(argv) {
  const args = { paths: [], full: false, githubOutput: false, summary: false, json: false }
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index]
    if (token === '--full') args.full = true
    else if (token === '--github-output') args.githubOutput = true
    else if (token === '--summary') args.summary = true
    else if (token === '--json') args.json = true
    else if (token === '--paths-file') {
      const value = argv[index + 1]
      if (value === undefined) throw new Error('Missing value for --paths-file')
      let text
      try {
        text = readFileSync(value, 'utf8')
      } catch {
        // Fail closed: an unreadable listing runs everything.
        return { ...args, full: true }
      }
      args.paths.push(...text.split(/\r?\n/))
      index += 1
    } else if (token === '--paths') {
      index += 1
      while (index < argv.length && !argv[index].startsWith('--')) {
        args.paths.push(argv[index])
        index += 1
      }
      index -= 1
    } else {
      throw new Error(`Unknown argument: ${token}`)
    }
  }
  return args
}

function main() {
  const args = parseArgs(process.argv.slice(2))
  const selection = selectChecks({ paths: args.paths, full: args.full })
  if (args.githubOutput) {
    const output = process.env.GITHUB_OUTPUT
    if (!output) throw new Error('GITHUB_OUTPUT is not set.')
    const lines = [`full=${selection.full}`, `reason=${selection.reason}`]
    for (const check of CHECKS) lines.push(`${check}=${selection.checks[check]}`)
    appendFileSync(output, `${lines.join('\n')}\n`)
  }
  if (args.summary) {
    const summary = process.env.GITHUB_STEP_SUMMARY
    if (summary) {
      const rows = CHECKS.map((check) => `| ${check} | ${selection.checks[check] ? 'run' : 'skipped'} |`)
      appendFileSync(summary, `### CI selection\n\n${selection.reason}\n\n| Check | This run |\n| --- | --- |\n${rows.join('\n')}\n\n`)
    }
  }
  if (args.json) process.stdout.write(`${JSON.stringify(selection, null, 2)}\n`)
  else {
    const selected = CHECKS.filter((check) => selection.checks[check])
    console.log(`${selection.reason}: ${selected.join(', ')}`)
  }
}

const invokedDirectly = process.argv[1]?.replaceAll('\\', '/').endsWith('scripts/ci-scope.mjs')
if (invokedDirectly) {
  try {
    main()
  } catch (error) {
    // A broken selection must not look like a short one.
    console.error(error instanceof Error ? error.message : String(error))
    process.exitCode = 1
  }
}
