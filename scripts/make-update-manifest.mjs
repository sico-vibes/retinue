// Writes the Tauri updater static manifest (`latest.json`). One release may
// carry several platforms — Windows NSIS and the Linux AppImage updater
// archive — under one version and one note, which is what the updater reads to
// pick the build for the machine it runs on.
import { readFileSync, realpathSync, writeFileSync } from 'node:fs'
import { fileURLToPath, pathToFileURL } from 'node:url'

export const DEFAULT_PLATFORM = 'windows-x86_64'

/** Tauri's updater key for the AppImage build of a 64-bit Linux machine. */
export const LINUX_PLATFORM = 'linux-x86_64'

/**
 * Semver, including pre-release identifiers and optional build metadata.
 * Same rule as scripts/release-windows.ps1 (Test-SemVer).
 */
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$/

const ISO_UTC = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?Z$/

const PLATFORM_NAME = /^[a-z0-9_-]+$/

export function isSemver(version) {
  return typeof version === 'string' && SEMVER.test(version)
}

export function isPreRelease(version) {
  const match = typeof version === 'string' ? SEMVER.exec(version) : null
  return Boolean(match && match[4])
}

/** One-line note used when a release notes file is absent or blank. */
export function defaultNotes(version) {
  return `Retinue ${version}`
}

function stripBom(text) {
  if (typeof text !== 'string' || text.charCodeAt(0) !== 0xfeff) return text
  return text.slice(1)
}

export function assertHttpsUrl(url) {
  if (typeof url !== 'string' || url.trim() === '') {
    throw new Error('URL must be an https URL.')
  }
  let parsed
  try {
    parsed = new URL(url)
  } catch {
    throw new Error(`URL must be an https URL: ${url}`)
  }
  if (parsed.protocol !== 'https:' || parsed.host === '') {
    throw new Error(`URL must be an https URL: ${url}`)
  }
}

/**
 * One `platforms` entry, validated. Returns `[name, { signature, url }]`.
 * `signature` is trimmed; an empty result is rejected.
 */
function platformEntry({ platform, signature, url }) {
  const name = platform || DEFAULT_PLATFORM
  if (!PLATFORM_NAME.test(name)) {
    throw new Error(`Invalid platform: ${name}`)
  }
  const trimmedSignature = typeof signature === 'string' ? stripBom(signature).trim() : ''
  if (trimmedSignature === '') {
    throw new Error('Signature is empty.')
  }
  assertHttpsUrl(url)
  return [name, { signature: trimmedSignature, url }]
}

/**
 * Build the Tauri updater static manifest object.
 *
 * Pass either one `platform`/`signature`/`url` triple, or a `platforms` object
 * (`{ 'windows-x86_64': { signature, url }, 'linux-x86_64': { ... } }`).
 * `notes` falls back to {@link defaultNotes} when omitted or blank.
 * `pubDate` defaults to the current UTC time as ISO 8601.
 */
export function buildManifest({
  version,
  notes,
  pubDate,
  platform = DEFAULT_PLATFORM,
  signature,
  url,
  platforms,
} = {}) {
  if (!isSemver(version)) {
    throw new Error(`Invalid semver version: ${version}`)
  }
  const entries = {}
  if (platforms != null) {
    if (typeof platforms !== 'object' || Array.isArray(platforms)) {
      throw new Error('platforms must be an object of platform entries.')
    }
    const names = Object.keys(platforms)
    if (names.length === 0) {
      throw new Error('At least one platform is required.')
    }
    for (const name of names) {
      const entry = platforms[name]
      if (entry === null || typeof entry !== 'object' || Array.isArray(entry)) {
        throw new Error(`The ${name} platform entry is not an object.`)
      }
      const [key, value] = platformEntry({ platform: name, ...entry })
      if (Object.prototype.hasOwnProperty.call(entries, key)) {
        throw new Error(`Duplicate platform: ${key}`)
      }
      entries[key] = value
    }
  } else {
    const [key, value] = platformEntry({ platform, signature, url })
    entries[key] = value
  }
  let published = pubDate
  if (published == null || published === '') {
    published = new Date().toISOString()
  } else if (typeof published !== 'string' || !ISO_UTC.test(published)) {
    throw new Error('pub-date must be ISO 8601 UTC, for example 2026-10-02T15:04:05Z.')
  } else {
    const parsed = new Date(published)
    const canonical = Number.isNaN(parsed.getTime()) ? '' : parsed.toISOString().replace(/\.\d{3}Z$/, 'Z')
    const given = published.replace(/\.\d{1,3}Z$/, 'Z')
    if (given !== canonical) {
      throw new Error('pub-date must be ISO 8601 UTC, for example 2026-10-02T15:04:05Z.')
    }
  }
  const noteSource = typeof notes === 'string' ? stripBom(notes) : notes
  const noteText = typeof noteSource === 'string' && noteSource.trim() !== '' ? noteSource.trim() : defaultNotes(version)
  return {
    version,
    notes: noteText,
    pub_date: published,
    platforms: entries,
  }
}

export function manifestToJson(manifest) {
  return `${JSON.stringify(manifest, null, 2)}\n`
}

function readText(path, label) {
  try {
    return readFileSync(path, 'utf8')
  } catch {
    throw new Error(`Could not read ${label}: ${path}`)
  }
}

const REPEATABLE = new Set(['platform', 'signature-file', 'url'])

function parseArgs(argv) {
  const args = { quiet: false }
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index]
    if (token === '--quiet') {
      if (args.quiet) throw new Error('Repeated argument --quiet')
      args.quiet = true
      continue
    }
    if (!token.startsWith('--')) {
      throw new Error(`Unexpected argument: ${token}`)
    }
    const key = token.slice(2)
    const value = argv[index + 1]
    if (value === undefined || value.startsWith('--')) {
      throw new Error(`Missing value for --${key}`)
    }
    if (REPEATABLE.has(key)) {
      if (!Array.isArray(args[key])) args[key] = []
      args[key].push(value)
    } else {
      if (Object.prototype.hasOwnProperty.call(args, key)) {
        throw new Error(`Repeated argument --${key}`)
      }
      args[key] = value
    }
    index += 1
  }
  return args
}

function main() {
  const args = parseArgs(process.argv.slice(2))
  if (!args.version) throw new Error('Missing --version')
  const signatureFiles = args['signature-file'] ?? []
  const urls = args.url ?? []
  const platforms = args.platform ?? []
  if (signatureFiles.length === 0) throw new Error('Missing --signature-file')
  if (urls.length === 0) throw new Error('Missing --url')
  const count = Math.max(signatureFiles.length, urls.length, platforms.length)
  if (signatureFiles.length !== count || urls.length !== count || (platforms.length !== 0 && platforms.length !== count)) {
    throw new Error('Pass one --signature-file and one --url for every --platform.')
  }
  const names = platforms.length === count ? platforms : Array.from({ length: count }, () => DEFAULT_PLATFORM)
  const entries = {}
  for (let index = 0; index < count; index += 1) {
    const name = names[index]
    if (Object.prototype.hasOwnProperty.call(entries, name)) {
      throw new Error(`Duplicate platform: ${name}`)
    }
    entries[name] = {
      signature: readText(signatureFiles[index], 'signature file'),
      url: urls[index],
    }
  }
  const notes = args['notes-file'] ? readText(args['notes-file'], 'notes file') : undefined
  const manifest = buildManifest({
    version: args.version,
    notes,
    pubDate: args['pub-date'],
    platforms: entries,
  })
  const json = manifestToJson(manifest)
  if (args.out) writeFileSync(args.out, json)
  if (args.quiet) {
    if (!args.out) {
      throw new Error('Pass --out to write latest.json when --quiet is set.')
    }
    return
  }
  process.stdout.write(json)
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
    const message = error instanceof Error ? error.message : String(error)
    process.stderr.write(`${message}\n`)
    process.exitCode = 1
  }
}
