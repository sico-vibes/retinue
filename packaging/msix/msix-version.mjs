#!/usr/bin/env node
// Turn a Retinue version into the four-part version an MSIX package needs.
//
// The Store requires every submission to carry a higher version and the fourth part to be 0.
// Pre-releases count up inside their patch: 0.1.0-beta.51 -> 0.1.51.0, and the final
// 0.1.0 -> 0.1.999.0, so a stable release always sorts above its betas.
//
//   node packaging/msix/msix-version.mjs 0.1.0-beta.51   # prints 0.1.51.0

import { fileURLToPath } from 'node:url'

const MAX_PART = 65535

export function msixVersion(version) {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z-]+\.(\d+))?$/.exec(version)
  if (!match) throw new Error(`Cannot map ${version}: use x.y.z or x.y.z-<label>.<n>.`)
  const [major, minor, patch] = [match[1], match[2], match[3]].map(Number)
  const pre = match[4] === undefined ? null : Number(match[4])
  if (pre !== null && (pre < 1 || pre > 998)) throw new Error(`The pre-release number must be 1 to 998: ${version}`)
  const third = patch * 1000 + (pre ?? 999)
  if (major > MAX_PART || minor > MAX_PART || third > MAX_PART) throw new Error(`${version} is too large for an MSIX version.`)
  return `${major}.${minor}.${third}.0`
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    console.log(msixVersion(process.argv[2] ?? ''))
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(1)
  }
}
