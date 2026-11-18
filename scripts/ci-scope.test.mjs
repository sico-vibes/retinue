// The CI selection's own tests. The table decides which checks a change runs,
// so a rule that is never exercised, or a path that runs nothing, is the way a
// green run stops meaning anything (`.github/workflows/ci.yml`).
//
// The last two tests are the ones worth keeping: every path filter must name a
// folder that exists in this repository, and every check the workflow runs must
// be one this table can select.
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { test } from 'node:test'

import { CHECKS, selectChecks } from './ci-scope.mjs'

/**
 * A file as text with LF endings. The repository stores workflows with CRLF, and
 * `$` does not anchor before `\r`, so every pattern below would miss.
 */
function lines(path) {
  return readFileSync(new URL(path, import.meta.url), 'utf8').replaceAll('\r\n', '\n')
}

const selected = (paths) => CHECKS.filter((check) => selectChecks({ paths }).checks[check])

test('a desktop change needs the app jobs and not the Rust ones', () => {
  assert.deepEqual(selected(['apps/desktop/src/ui/App.tsx']), ['hygiene', 'types', 'uiTests', 'build', 'perf'])
})

test('a crate change needs the Rust jobs and not the app ones', () => {
  assert.deepEqual(selected(['crates/retinue-daemon/src/main.rs']), ['hygiene', 'daemonTests', 'clippy', 'perf'])
})

test('a documentation change needs only the fast checks and the library', () => {
  assert.deepEqual(selected(['docs/reference/invariants.md']), ['hygiene', 'docs'])
  assert.deepEqual(selected(['README.md']), ['hygiene', 'docs'])
})

test('the fast checks always run', () => {
  for (const paths of [['docs/README.md'], ['apps/desktop/src/index.html'], ['assets/icon/icon.ico']]) {
    assert.ok(selectChecks({ paths }).checks.hygiene, paths.join(' '))
  }
})

test('a lockfile, a toolchain, a workflow or a script runs everything', () => {
  for (const path of [
    'package-lock.json',
    'Cargo.lock',
    'Cargo.toml',
    'package.json',
    'rust-toolchain.toml',
    'perf-budgets.json',
    '.github/workflows/ci.yml',
    'scripts/ci-scope.mjs',
    'apps/desktop/src-tauri/tauri.conf.json',
    'packaging/msix/build-msix.ps1',
  ]) {
    const selection = selectChecks({ paths: [path] })
    assert.equal(selection.full, true, path)
    assert.deepEqual(selected([path]), CHECKS, path)
  }
})

test('an unknown path runs everything, never nothing', () => {
  const stranger = selectChecks({ paths: ['elsewhere/file.txt'] })
  assert.equal(stranger.full, true)
  assert.deepEqual(stranger.unknown, ['elsewhere/file.txt'])
  assert.deepEqual(selected(['elsewhere/file.txt']), CHECKS)
  // A path under a folder the table does know stays in that folder's jobs.
  const known = selectChecks({ paths: ['tools/retinue-hook/some-file.rs'] })
  assert.equal(known.full, false)
  assert.deepEqual(selected(['tools/retinue-hook/some-file.rs']), ['hygiene', 'daemonTests', 'clippy', 'perf'])
})

test('a full run, an empty run and an unreadable listing run everything', () => {
  for (const input of [{ paths: [], full: false }, { paths: [], full: true }, { paths: ['docs/a.md'], full: true }]) {
    const selection = selectChecks(input)
    assert.equal(selection.full, true, JSON.stringify(input))
    assert.deepEqual(Object.values(selection.checks), CHECKS.map(() => true))
  }
})

test('a selection names every check exactly once', () => {
  const selection = selectChecks({ paths: ['apps/desktop/src/ui/App.tsx'] })
  assert.deepEqual(Object.keys(selection.checks), CHECKS)
  for (const check of CHECKS) assert.equal(typeof selection.checks[check], 'boolean', check)
})

test('every path rule names something that exists here', () => {
  // One real path per rule. A rule that matches nothing in this repository is
  // a rule nobody will notice rotting, and a move that leaves it behind would
  // run the wrong checks silently.
  const probes = [
    'apps/desktop/src/ui/App.tsx',
    'apps/desktop/index.html',
    'apps/desktop/vite.preview.config.ts',
    'apps/desktop/package.json',
    'assets/icon/icon.ico',
    'crates/retinue-daemon/src/main.rs',
    'tools/retinue-hook/src/main.rs',
    'docs/plans/nod-adoption-plan.md',
    'README.md',
    '.claude/skills/retinue-development/SKILL.md',
    'apps/site/package.json',
    'apps/website/README.md',
  ]
  for (const path of probes) {
    const selection = selectChecks({ paths: [path] })
    assert.equal(selection.unknown.length, 0, `${path} is not covered by any rule`)
    assert.deepEqual(selection.everything, [], `${path} should reach its own job, not all of them`)
    const root = path.split('/')[0]
    assert.ok(existsSync(new URL(`../${root}`, import.meta.url)), `${path}: ${root} does not exist`)
  }
})

test('the workflow runs exactly the checks this table can select', () => {
  const workflow = lines('../.github/workflows/ci.yml')
  for (const check of CHECKS) {
    assert.match(workflow, new RegExp(`^  ${check}:$`, 'm'), `ci.yml has no ${check} job`)
    assert.ok(workflow.includes(`needs.scope.outputs.${check} == 'true'`), `the ${check} job is not selected by scope`)
  }
})

test('every script and workflow file the workflows name exists', () => {
  // A renamed or deleted script is a workflow that fails on the runner, twenty
  // minutes after somebody pushed. This catches it locally instead.
  const files = ['.github/workflows/ci.yml', '.github/workflows/release.yml']
  const text = files.map((path) => lines(`../${path}`)).join('\n')
  const named = new Set()
  for (const match of text.matchAll(/(?:node|\.\/| )((?:scripts|packaging)\/[\w./-]+\.(?:mjs|ps1|sh|cmd))/g)) named.add(match[1])
  assert.ok(named.size >= 5, `only ${named.size} scripts were found in the workflows`)
  for (const script of named) {
    assert.ok(existsSync(new URL(`../${script}`, import.meta.url)), `${script} is named in a workflow but does not exist`)
  }
})

test('the workflow files are shaped the way a runner needs', () => {
  for (const path of ['ci.yml', 'release.yml']) {
    const text = lines(`../.github/workflows/${path}`)
    // A tab in YAML indentation is a parse error on the runner and invisible here.
    assert.equal(/^\t/m.test(text), false, `${path} indents with a tab`)
    assert.match(text, /^jobs:$/m, `${path} has no jobs`)
    // Every job says where it runs; one without `runs-on` is a workflow error.
    const jobsText = text.slice(text.indexOf('\njobs:\n'))
    const jobs = [...jobsText.matchAll(/^  ([A-Za-z][\w-]*):$/gm)].map((match) => match[1])
    assert.ok(jobs.length >= 2, `${path} has only ${jobs.length} jobs`)
    for (const job of jobs) {
      const block = jobsText.slice(jobsText.indexOf(`\n  ${job}:\n`))
      const next = block.slice(2).search(/^  [A-Za-z][\w-]*:$/m)
      const own = next === -1 ? block : block.slice(0, next + 2)
      assert.match(own, /runs-on:/, `${path}: job ${job} has no runs-on`)
    }
  }
})

test('the required check waits for every other job', () => {
  const workflow = lines('../.github/workflows/ci.yml')
  const block = workflow.slice(workflow.indexOf('\n  checks:\n'))
  for (const check of CHECKS) assert.ok(block.includes(check), `checks does not wait for ${check}`)
  assert.match(block, /if: always\(\)/, 'checks must run even when a job it waits for failed')
})

