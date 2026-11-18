// Reads the update manifest the release script writes the way an installed
// build's updater does, and says whether it offers a newer version (wave 3 of
// `docs/plans/nod-adoption-plan.md`, release safety; the design is
// `docs/research/n8n-nod/07-architecture.md` idea 7).
//
// The reader mirrors `tauri-plugin-updater` 2.10.1's `RemoteRelease`
// (`src/updater.rs`): `version` is a semver `Version`, `notes` is optional,
// `pub_date` is an RFC 3339 timestamp parsed by `time::OffsetDateTime`, and the
// platform map holds `{ url, signature }` under `windows-<arch>`. A field it
// would refuse is refused here, with a sentence that names the field, so a
// release fails locally rather than in every installed app.
//
// `scripts/release-windows.ps1` runs this on the manifest it just wrote, and
// against the version `channel-beta` currently offers: publishing a manifest
// that is not newer than what phones and desktops already see is the one
// release mistake nobody can install their way out of.
//
// Usage:
//   node scripts/update-round-trip.mjs --manifest latest.json
//   node scripts/update-round-trip.mjs --manifest latest.json --installed 0.2.0-beta.4

import { readFileSync } from 'node:fs'
import { realpathSync } from 'node:fs'
import { fileURLToPath, pathToFileURL } from 'node:url'

import { DEFAULT_PLATFORM } from './make-update-manifest.mjs'

/**
 * Semver 2.0.0, the same rule as `SEMVER` in `scripts/make-update-manifest.mjs`
 * and `Test-SemVer` in `scripts/release-windows.ps1`.
 */
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$/

/** RFC 3339 in UTC with a `Z`, which is what the release script writes. */
const RFC3339_UTC = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/

export class ManifestError extends Error {}

function refuse(message) {
  throw new ManifestError(message)
}

/** `[major, minor, patch, prerelease[]]`, or `null` when it is not semver. */
function parseVersion(version) {
  const match = typeof version === 'string' ? SEMVER.exec(version) : null
  if (!match) return null
  return {
    numbers: [Number(match[1]), Number(match[2]), Number(match[3])],
    prerelease: match[4] ? match[4].split('.') : [],
  }
}

function comparePrerelease(left, right) {
  // A version without a prerelease outranks one with it.
  if (!left.length && !right.length) return 0
  if (!left.length) return 1
  if (!right.length) return -1
  const length = Math.max(left.length, right.length)
  for (let index = 0; index < length; index += 1) {
    const a = left[index]
    const b = right[index]
    if (a === undefined) return -1
    if (b === undefined) return 1
    if (a === b) continue
    const aNumber = /^\d+$/.test(a)
    const bNumber = /^\d+$/.test(b)
    // Numeric identifiers compare numerically and rank below alphanumeric ones.
    if (aNumber && bNumber) return Number(a) < Number(b) ? -1 : 1
    if (aNumber) return -1
    if (bNumber) return 1
    return a < b ? -1 : 1
  }
  return 0
}

/** -1, 0 or 1, by semver precedence. Throws for a string that is not semver. */
export function compareVersions(left, right) {
  const a = parseVersion(left)
  const b = parseVersion(right)
  if (!a) refuse(`${left} is not a version, so it cannot be compared.`)
  if (!b) refuse(`${right} is not a version, so it cannot be compared.`)
  for (let index = 0; index < 3; index += 1) {
    if (a.numbers[index] !== b.numbers[index]) return a.numbers[index] < b.numbers[index] ? -1 : 1
  }
  return comparePrerelease(a.prerelease, b.prerelease)
}

/**
 * The release a device would see in this manifest, for one platform.
 *
 * `json` is the manifest object (or the text of the file). Every check the
 * updater makes is made here, in its order: the document, the version, the
 * notes, the date, the platform, its URL and its signature.
 */
export function readManifest(json, { platform = DEFAULT_PLATFORM } = {}) {
  let manifest = json
  if (typeof manifest === 'string') {
    try {
      manifest = JSON.parse(manifest.replace(/^\uFEFF/, ''))
    } catch {
      refuse('The manifest is not JSON.')
    }
  }
  if (manifest === null || typeof manifest !== 'object' || Array.isArray(manifest)) {
    refuse('The manifest is not a JSON object.')
  }
  if (typeof manifest.version !== 'string' || !parseVersion(manifest.version)) {
    refuse(`The manifest's version is not semver: ${JSON.stringify(manifest.version)}`)
  }
  if (manifest.notes !== undefined && manifest.notes !== null && typeof manifest.notes !== 'string') {
    refuse('The manifest’s notes are not text.')
  }
  if (manifest.pub_date !== undefined && manifest.pub_date !== null) {
    if (typeof manifest.pub_date !== 'string' || !RFC3339_UTC.test(manifest.pub_date) || Number.isNaN(Date.parse(manifest.pub_date))) {
      refuse(`The manifest's pub_date is not an ISO 8601 UTC date: ${JSON.stringify(manifest.pub_date)}`)
    }
  }
  const platforms = manifest.platforms
  if (platforms === null || typeof platforms !== 'object' || Array.isArray(platforms)) {
    refuse('The manifest has no platforms object.')
  }
  const entry = platforms[platform]
  if (entry === undefined) {
    refuse(`The manifest offers no ${platform} build, so this install would never see it.`)
  }
  if (entry === null || typeof entry !== 'object' || Array.isArray(entry)) {
    refuse(`The manifest's ${platform} entry is not an object.`)
  }
  let parsedUrl
  try {
    parsedUrl = new URL(entry.url)
  } catch {
    refuse(`The manifest's ${platform} URL is not a URL: ${JSON.stringify(entry.url)}`)
  }
  if (parsedUrl.protocol !== 'https:') {
    refuse(`The manifest's ${platform} URL is not an https URL: ${entry.url}`)
  }
  if (typeof entry.signature !== 'string' || entry.signature.trim() === '') {
    refuse(`The manifest's ${platform} entry has no signature, so the update would be refused when it downloads.`)
  }
  return {
    version: manifest.version,
    notes: typeof manifest.notes === 'string' ? manifest.notes : null,
    pubDate: typeof manifest.pub_date === 'string' ? manifest.pub_date : null,
    platform,
    url: entry.url,
    signature: entry.signature.trim(),
  }
}

/**
 * Whether the manifest offers something newer than `installedVersion`.
 * The updater installs only a strictly greater version, so an equal one is not
 * an update and must not be published as one.
 */
export function offersAnUpdate(json, installedVersion, options = {}) {
  const manifest = typeof json === 'object' && json !== null && typeof json.version === 'string' ? json : readManifest(json, options)
  return compareVersions(manifest.version, installedVersion) > 0
}

function parseArgs(argv) {
  const args = {}
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index]
    if (!token.startsWith('--')) refuse(`Unexpected argument: ${token}`)
    const key = token.slice(2)
    const value = argv[index + 1]
    if (value === undefined || value.startsWith('--')) refuse(`Missing value for --${key}`)
    if (Object.prototype.hasOwnProperty.call(args, key)) refuse(`Repeated argument --${key}`)
    args[key] = value
    index += 1
  }
  return args
}

function main() {
  const args = parseArgs(process.argv.slice(2))
  if (!args.manifest) refuse('Missing --manifest')
  let text
  try {
    text = readFileSync(args.manifest, 'utf8')
  } catch {
    refuse(`Could not read the manifest: ${args.manifest}`)
  }
  const manifest = readManifest(text, { platform: args.platform || DEFAULT_PLATFORM })
  if (!args.installed) {
    process.stdout.write(`The manifest offers ${manifest.version} for ${manifest.platform}.\n`)
    return
  }
  if (!offersAnUpdate(manifest, args.installed)) {
    refuse(`The manifest offers ${manifest.version}, which is not newer than the published ${args.installed}. Installs would not update.`)
  }
  process.stdout.write(`The manifest offers ${manifest.version}, newer than the published ${args.installed}.\n`)
}

function runningAsCli() {
  const entry = process.argv[1]
  if (!entry) return false
  try {
    return realpathSync(entry) === realpathSync(fileURLToPath(import.meta.url))
  } catch {
    return import.meta.url === pathToFileURL(entry).href
  }
}

if (runningAsCli()) {
  try {
    main()
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`)
    process.exitCode = 1
  }
}
