// The pre-commit hook the Director installs with `npm run hooks:install`.
//
// Two checks run here, both fast enough for a commit:
//   1. the formatting check over the files being committed (from the index, so
//      what is committed is what was checked);
//   2. `gitleaks` over the same files, when it is installed. A machine without
//      it still commits, because CI scans the whole history and the working
//      tree; the hook is the fast first line, not the only one.
//
// Nothing here commits, pushes or fixes files: it says what is wrong and exits.

import { spawnSync } from 'node:child_process'

import { checkFormat } from './check-format.mjs'

const run = (command, args) => spawnSync(command, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 })

console.log('pre-commit: formatting')
const staged = run('git', ['diff', '--cached', '--name-only', '--diff-filter=ACMR', '-z'])
if (staged.status !== 0) {
  console.error('pre-commit: could not read the staged files')
  console.error(staged.stderr)
  process.exit(1)
}
const paths = staged.stdout.split('\0').filter(Boolean)
const files = []
for (const path of paths) {
  const blob = spawnSync('git', ['show', `:${path}`], { maxBuffer: 64 * 1024 * 1024 })
  if (blob.status === 0) files.push({ path, bytes: blob.stdout })
}
const { failures } = checkFormat({ files, full: false })
if (failures.length) {
  for (const failure of failures) console.error(`  ${failure.path}: ${failure.message}`)
  console.error('pre-commit: fix the formatting above (node scripts/check-format.mjs explains each rule)')
  process.exit(1)
}
console.log(`pre-commit: formatting passed (${files.length} ${files.length === 1 ? 'file' : 'files'})`)

const gitleaks = process.env.GITLEAKS_PATH || 'gitleaks'
const probe = run(gitleaks, ['version'])
if (probe.status !== 0) {
  console.log(`pre-commit: ${gitleaks} is not installed, so the staged secret scan is skipped (CI still scans every commit)`)
  process.exit(0)
}
const scan = spawnSync(
  gitleaks,
  ['git', '--pre-commit', '--staged', '--config', '.gitleaks.toml', '--redact', '--no-banner', '--verbose'],
  { stdio: 'inherit' },
)
if (scan.status === 1) {
  console.error('pre-commit: a secret was found in the staged files; nothing is committed')
  process.exit(1)
}
if (scan.status !== 0) {
  console.error(`pre-commit: the secret scan could not run (${scan.status})`)
  process.exit(1)
}
process.exit(0)
