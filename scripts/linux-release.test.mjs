import assert from 'node:assert/strict'
import { gunzipSync } from 'node:zlib'
import { spawnSync } from 'node:child_process'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

import { buildTarHeader, packAppImage } from './linux-release.mjs'

const scriptPath = fileURLToPath(new URL('./linux-release.mjs', import.meta.url))

async function tempDir(t) {
  const root = await mkdtemp(join(tmpdir(), 'retinue-linux-release-'))
  t.after(() => rm(root, { recursive: true, force: true }))
  return root
}

function readHeader(tar) {
  const header = tar.subarray(0, 512)
  const octal = (start, length) => parseInt(header.subarray(start, start + length).toString('utf8').replace(/\0.*$/, ''), 8)
  const name = header.subarray(0, 100).toString('utf8').replace(/\0.*$/, '')
  const prefix = header.subarray(345, 500).toString('utf8').replace(/\0.*$/, '')
  const checksumField = header.subarray(148, 156)
  const copy = Buffer.from(header)
  copy.fill(0x20, 148, 156)
  let checksum = 0
  for (const byte of copy) checksum += byte
  return {
    name: prefix ? `${prefix}/${name}` : name,
    mode: octal(100, 8),
    uid: octal(108, 8),
    gid: octal(116, 8),
    size: octal(124, 12),
    typeflag: String.fromCharCode(header[156]),
    magic: header.subarray(257, 263).toString('utf8'),
    checksum: octal(148, 8),
    computedChecksum: checksum,
    checksumField,
  }
}

test('packs the AppImage into a tar.gz whose member is executable', async (t) => {
  const root = await tempDir(t)
  const name = 'Retinue_0.2.0-beta.14_amd64.AppImage'
  const appImage = join(root, name)
  const bytes = Buffer.from(Array.from({ length: 1500 }, (_, index) => index % 251))
  await writeFile(appImage, bytes)

  const archive = await packAppImage(appImage)
  assert.equal(archive, `${appImage}.tar.gz`)

  const tar = gunzipSync(await readFile(archive))
  const header = readHeader(tar)
  assert.equal(header.name, name)
  assert.equal(header.mode, 0o755)
  assert.equal(header.size, bytes.length)
  assert.equal(header.typeflag, '0')
  assert.equal(header.magic, 'ustar\0')
  assert.equal(header.checksum, header.computedChecksum)
  // The bytes, then zero padding to the block, then the two end blocks.
  assert.deepEqual(tar.subarray(512, 512 + bytes.length), bytes)
  const padded = 512 + Math.ceil(bytes.length / 512) * 512
  assert.equal(tar.length, padded + 1024)
  assert.ok(tar.subarray(512 + bytes.length, padded).every((byte) => byte === 0))
  assert.ok(tar.subarray(padded).every((byte) => byte === 0))

  // A second opinion from the platform's tar, which the updater's reader shares
  // the format with (skipped only when no `tar` is on PATH).
  const list = spawnSync('tar', ['-tzf', archive], { encoding: 'utf8' })
  if (list.error === undefined && list.status === 0) {
    assert.deepEqual(list.stdout.split(/\r?\n/).filter(Boolean), [name])
    const verbose = spawnSync('tar', ['-tvzf', archive], { encoding: 'utf8' })
    assert.match(verbose.stdout, new RegExp(`^-rwxr-xr-x .*${name}$`, 'm'))
  }
})

test('writes a custom --out and leaves the AppImage byte-identical', async (t) => {
  const root = await tempDir(t)
  const appImage = join(root, 'Retinue_1.0.0_amd64.AppImage')
  const out = join(root, 'nested', 'updater.tar.gz')
  const bytes = Buffer.from('not really an AppImage')
  await writeFile(appImage, bytes)
  const { mkdir } = await import('node:fs/promises')
  await mkdir(join(root, 'nested'))

  const result = spawnSync(process.execPath, [scriptPath, 'pack-appimage', appImage, '--out', out, '--quiet'], { encoding: 'utf8' })
  assert.equal(result.status, 0, result.stderr)
  assert.equal(result.stdout, '')
  assert.deepEqual(readHeader(gunzipSync(await readFile(out))).name, 'Retinue_1.0.0_amd64.AppImage')
  assert.deepEqual(await readFile(appImage), bytes)
})

test('splits a long member name into the ustar prefix', () => {
  const long = `${'a/'.repeat(60)}Retinue_1.0.0_amd64.AppImage`
  const header = buildTarHeader(long, 10, 0)
  const parsed = readHeader(header)
  assert.equal(parsed.name, long)
  assert.equal(parsed.mode, 0o755)
})

test('refuses a missing, empty or self-referential AppImage', async (t) => {
  const root = await tempDir(t)
  await assert.rejects(packAppImage(join(root, 'absent.AppImage')), /not found/)
  const empty = join(root, 'empty.AppImage')
  await writeFile(empty, '')
  await assert.rejects(packAppImage(empty), /empty/)
  const real = join(root, 'real.AppImage')
  await writeFile(real, 'x')
  await assert.rejects(packAppImage(real, real), /cannot be the AppImage itself/)

  const cli = spawnSync(process.execPath, [scriptPath, 'pack-appimage', join(root, 'absent.AppImage')], { encoding: 'utf8' })
  assert.notEqual(cli.status, 0)
  assert.match(cli.stderr, /not found/)
})
