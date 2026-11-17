// The pre-push hook `npm run hooks:install` installs.
//
// The architecture check is here rather than in the commit hook because it reads
// the whole tree (about half a second) and because a boundary crossed in one
// commit is often fixed by the next. The Director's commit gate, CI and
// `npm run check:architecture` all run the same script.

import { spawnSync } from 'node:child_process'

const step = (name, args) => {
  console.log(`pre-push: ${name}`)
  const result = spawnSync(process.execPath, args, { stdio: 'inherit' })
  if (result.status !== 0) {
    console.error(`pre-push: ${name} failed; nothing was pushed`)
    process.exit(1)
  }
}

step('architecture', ['scripts/check-architecture.mjs'])
step('formatting', ['scripts/check-format.mjs'])
step('script tests', ['--test', 'scripts/**/*.test.mjs'])
process.exit(0)
