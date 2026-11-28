// Pack a Linux AppImage into the archive the Tauri updater installs
// (`<name>.AppImage.tar.gz`), without needing a Unix `tar`.
//
// Why not `tar`: the updater replaces the running AppImage with the archive's
// member and takes that member's permission bits from the tar header
// (`tauri-plugin-updater-2.10.1/src/updater.rs`, `install_appimage`). Windows'
// `tar` records no Unix mode, so an archive made there can leave the AppImage
// non-executable after an update. This writes the ustar header itself with mode
// 0755, which is what tauri-bundler does on Linux.
//
// Usage:
//   node scripts/linux-release.mjs pack-appimage <Retinue_<version>_amd64.AppImage> [--out <file.tar.gz>]
//
// The release script calls it and signs the archive with the updater key.

import { createReadStream, createWriteStream, realpathSync, statSync } from 'node:fs'
import { basename, resolve } from 'node:path'
import { Readable } from 'node:stream'
import { pipeline } from 'node:stream/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { createGzip } from 'node:zlib'

const BLOCK = 512
/** ustar's octal size field: 11 digits, so just under 8 GiB. */
const MAX_SIZE = 8 ** 11 - 1
const FILE_MODE = 0o755

function writeText(buffer, offset, text, length) {
  const bytes = Buffer.from(text, 'utf8')
  if (bytes.length > length) throw new Error(`tar header field holds ${length} bytes, ${bytes.length} given: ${text}`)
  bytes.copy(buffer, offset)
}

function writeOctal(buffer, offset, value, length) {
  const text = value.toString(8)
  if (text.length > length - 1) throw new Error(`tar header field holds ${length - 1} octal digits, ${text} needs more`)
  writeText(buffer, offset, text.padStart(length - 1, '0'), length - 1)
  buffer[offset + length - 1] = 0
}

/**
 * Split a long path into ustar's name (100) and prefix (155) fields.
 * Short names stay whole, which is every AppImage Tauri builds.
 */
function splitName(name) {
  if (Buffer.byteLength(name) <= 100) return { name, prefix: '' }
  const bytes = Buffer.from(name, 'utf8')
  for (let index = name.length - 1; index >= 0; index -= 1) {
    if (name[index] !== '/') continue
    const prefix = name.slice(0, index)
    const rest = name.slice(index + 1)
    if (Buffer.byteLength(rest) <= 100 && Buffer.byteLength(prefix) <= 155 && rest !== '') return { name: rest, prefix }
  }
  throw new Error(`The archive member name is too long for a ustar header: ${name}`)
}

/** A ustar header for one regular file. Exported for the test that reads it back. */
export function buildTarHeader(name, size, mtimeSeconds) {
  if (!Number.isInteger(size) || size < 0 || size > MAX_SIZE) throw new Error(`Unsupported size for ${name}: ${size}`)
  const split = splitName(name)
  const header = Buffer.alloc(BLOCK, 0)
  writeText(header, 0, split.name, 100)
  writeOctal(header, 100, FILE_MODE, 8)
  writeOctal(header, 108, 0, 8) // uid
  writeOctal(header, 116, 0, 8) // gid
  writeOctal(header, 124, size, 12)
  writeOctal(header, 136, mtimeSeconds, 12)
  header.fill(0x20, 148, 156) // checksum field is spaces while it is summed
  header.write('0', 156, 'utf8') // regular file
  writeText(header, 257, 'ustar', 6) // magic, with its NUL
  writeText(header, 263, '00', 2) // version
  writeText(header, 265, 'root', 32)
  writeText(header, 297, 'root', 32)
  writeOctal(header, 329, 0, 8) // devmajor
  writeOctal(header, 337, 0, 8) // devminor
  writeText(header, 345, split.prefix, 155)
  let checksum = 0
  for (const byte of header) checksum += byte
  const octal = checksum.toString(8).padStart(6, '0')
  if (octal.length !== 6) throw new Error(`tar checksum does not fit its field: ${checksum}`)
  header.write(octal, 148, 'utf8')
  header[154] = 0
  header[155] = 0x20
  return header
}

/**
 * Write `<appImage>.tar.gz` holding one member, the AppImage, mode 0755.
 * Resolves to the archive path.
 */
export async function packAppImage(appImagePath, outPath) {
  const source = resolve(appImagePath)
  const destination = resolve(outPath ?? `${source}.tar.gz`)
  if (source === destination) throw new Error('The archive cannot be the AppImage itself.')
  const stats = statSync(source, { throwIfNoEntry: false })
  if (!stats || !stats.isFile()) throw new Error(`The AppImage was not found: ${source}`)
  if (stats.size === 0) throw new Error(`The AppImage is empty: ${source}`)
  const name = basename(source)
  const header = buildTarHeader(name, stats.size, Math.floor(stats.mtimeMs / 1000))
  const padding = Buffer.alloc((BLOCK - (stats.size % BLOCK)) % BLOCK, 0)
  const end = Buffer.alloc(BLOCK * 2, 0)
  async function* pieces() {
    yield header
    yield* createReadStream(source)
    if (padding.length) yield padding
    yield end
  }
  await pipeline(Readable.from(pieces()), createGzip({ level: 9 }), createWriteStream(destination))
  return destination
}

function parseArgs(argv) {
  const [command, ...rest] = argv
  const args = { command, quiet: false }
  for (let index = 0; index < rest.length; index += 1) {
    const token = rest[index]
    if (token === '--quiet') {
      args.quiet = true
      continue
    }
    if (!token.startsWith('--')) {
      if (args.appimage !== undefined) throw new Error(`Unexpected argument: ${token}`)
      args.appimage = token
      continue
    }
    const key = token.slice(2)
    const value = rest[index + 1]
    if (value === undefined || value.startsWith('--')) throw new Error(`Missing value for --${key}`)
    if (Object.prototype.hasOwnProperty.call(args, key)) throw new Error(`Repeated argument --${key}`)
    args[key] = value
    index += 1
  }
  return args
}

async function main() {
  const args = parseArgs(process.argv.slice(2))
  if (args.command !== 'pack-appimage') throw new Error('Usage: node scripts/linux-release.mjs pack-appimage <AppImage> [--out <file.tar.gz>]')
  if (!args.appimage) throw new Error('Missing the AppImage path.')
  const archive = await packAppImage(args.appimage, args.out)
  if (!args.quiet) {
    const size = statSync(archive).size
    process.stdout.write(`Packed ${basename(archive)} (${size} bytes, member ${basename(args.appimage)} at mode 0755)\n`)
  }
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
    await main()
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`)
    process.exitCode = 1
  }
}
