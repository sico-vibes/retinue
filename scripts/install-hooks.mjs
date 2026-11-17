// Installs (or removes) the repository's git hooks:
//
//   npm run hooks:install
//   node scripts/install-hooks.mjs --remove
//
// The hook files live in `.git`, not in the tree, because a tracked hook would
// arrive with CRLF line endings on a Windows checkout and `sh` would refuse to
// run it ("bad interpreter"). What is tracked is the script it calls, so a review
// sees the check. Nothing else about the repository is changed: no
// `core.hooksPath`, no global git configuration.
//
// Lanes never commit (AGENTS.md); this is for the Director's own commits. It is
// opt-in on purpose: a machine without `gitleaks` still works, and CI scans
// every commit whether or not this was ever installed.

import { chmodSync, existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'

const hooks = ['pre-commit', 'pre-push']
const remove = process.argv.includes('--remove')

const where = spawnSync('git', ['rev-parse', '--git-path', 'hooks'], { encoding: 'utf8' })
if (where.status !== 0) {
  console.error(`not a git checkout: ${where.stderr.trim()}`)
  process.exit(1)
}
const directory = where.stdout.trim()

if (remove) {
  for (const hook of hooks) {
    const file = join(directory, hook)
    if (existsSync(file)) {
      rmSync(file)
      console.log(`removed ${file}`)
    }
  }
  process.exit(0)
}

mkdirSync(directory, { recursive: true })
for (const hook of hooks) {
  const file = join(directory, hook)
  if (existsSync(file)) {
    console.log(`kept ${file} (it already exists; remove it first to replace it)`)
    continue
  }
  // Written with an explicit `\n`: this file must not carry CRLF.
  writeFileSync(file, `#!/bin/sh\n# Installed by scripts/install-hooks.mjs. Remove with: node scripts/install-hooks.mjs --remove\nexec node scripts/${hook}.mjs\n`, 'utf8')
  try {
    chmodSync(file, 0o755)
  } catch {
    // Windows has no executable bit; git runs the hook through `sh` anyway.
  }
  console.log(`installed ${file}`)
}
console.log(`Git keeps its hooks in ${directory}, shared by every worktree of this clone.`)
console.log('The pre-commit hook checks formatting and scans the staged files for secrets; the pre-push hook runs the architecture and format checks and the script tests.')
