#!/usr/bin/env node
// Fetches the pinned Windows computer-use driver, checks it, and stages it next
// to the daemon. It is the only way Retinue gets that binary: nothing here is
// committed, and a wrong hash is a refusal, never a warning.
//
// The driver is `open-computer-use` v1.1.4 (MIT), from the
// `iFurySt/open-computer-use` GitHub release. The daemon starts it as a
// supervised child behind the `owner_*` tools and refuses to start anything whose
// bytes are not the ones pinned here (`crates/retinue-daemon/src/owner_computer.rs`).
//
// Usage:
//   node scripts/fetch-owner-computer-driver.mjs            # beside `retinued.exe`
//   node scripts/fetch-owner-computer-driver.mjs --release  # the release build
//   node scripts/fetch-owner-computer-driver.mjs --dest <dir>
//
// The pin, the licence and the review date are recorded in
// `docs/operations/owner-computer-driver.md`. Upgrade by reading the diff,
// taking the new release's figures at fetch time, and updating both this file and
// that document.

import { createHash } from 'node:crypto'
import { mkdtempSync, mkdirSync, readFileSync, rmSync, copyFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, resolve, join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')

/** The pinned release. These two figures are the whole trust anchor. */
export const DRIVER = {
  version: '1.1.4',
  release: 'v1.1.4',
  repository: 'iFurySt/open-computer-use',
  asset: 'open-computer-use-cli-1.1.4-windows-amd64.zip',
  /** The release zip, as GitHub publishes it in the release's digest field. */
  zipSha256: '6635efc579ee3ac1f8b26cfb67e31ccb0ee88f1aa5c587359d17daa780e6817a',
  /** The `open-computer-use.exe` inside that zip. The daemon checks this one. */
  exeSha256: 'c8ca36d8871f02e621829f6024a794f186c779217a02b9b5cc60161917e90854',
  exeName: 'open-computer-use.exe',
}

export function url() {
  return `https://github.com/${DRIVER.repository}/releases/download/${DRIVER.release}/${DRIVER.asset}`
}

export function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex')
}

/** Where the daemon expects it: the folder holding `retinued.exe`. */
function daemonFolder(releaseBuild) {
  const profile = releaseBuild ? 'release' : 'debug'
  const metadata = spawnSync('cargo', ['metadata', '--no-deps', '--format-version', '1'], { cwd: root, encoding: 'utf8' })
  if (metadata.status !== 0) throw new Error(`cargo metadata failed: ${metadata.stderr || metadata.error || 'unknown error'}`)
  const target = JSON.parse(metadata.stdout).target_directory
  if (!target) throw new Error('cargo metadata did not report a target_directory.')
  return resolve(target, profile)
}

async function main() {
  const dest = process.argv.includes('--dest') ? process.argv[process.argv.indexOf('--dest') + 1] : daemonFolder(process.argv.includes('--release'))
  if (!dest) throw new Error('--dest needs a folder.')
  const work = mkdtempSync(join(tmpdir(), 'retinue-ocu-'))
  try {
    const zip = join(work, DRIVER.asset)
    console.log(`Fetching ${url()}`)
    const response = await fetch(url())
    if (!response.ok) throw new Error(`the release asset answered HTTP ${response.status}`)
    const bytes = Buffer.from(await response.arrayBuffer())
    const zipped = createHash('sha256').update(bytes).digest('hex')
    if (zipped !== DRIVER.zipSha256) {
      throw new Error(`the downloaded zip is not the pinned one: expected ${DRIVER.zipSha256}, got ${zipped}. Nothing was staged.`)
    }
    console.log(`zip   sha256 ${zipped} (pinned)`)
    const { writeFileSync } = await import('node:fs')
    writeFileSync(zip, bytes)

    const expanded = join(work, 'unpacked')
    mkdirSync(expanded, { recursive: true })
    const unzip = spawnSync('powershell', ['-NoProfile', '-NonInteractive', '-Command', `Expand-Archive -LiteralPath '${zip}' -DestinationPath '${expanded}' -Force`], { encoding: 'utf8' })
    if (unzip.status !== 0) throw new Error(`the zip could not be expanded: ${unzip.stderr || unzip.error || 'unknown error'}`)
    const extracted = join(expanded, DRIVER.exeName)
    if (!existsSync(extracted)) throw new Error(`the zip holds no ${DRIVER.exeName}.`)

    const actual = sha256(extracted)
    if (actual !== DRIVER.exeSha256) {
      throw new Error(`${DRIVER.exeName} is not the pinned build: expected ${DRIVER.exeSha256}, got ${actual}. Nothing was staged.`)
    }
    console.log(`exe   sha256 ${actual} (pinned)`)

    mkdirSync(dest, { recursive: true })
    const target = join(dest, DRIVER.exeName)
    copyFileSync(extracted, target)
    console.log(`Staged ${target}`)
    console.log('This binary is never committed: it is fetched, checked and staged, and `.gitignore` keeps it out of the tree.')
  } finally {
    rmSync(work, { recursive: true, force: true })
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`fetch-owner-computer-driver: ${error.message}`)
    process.exit(1)
  })
}
