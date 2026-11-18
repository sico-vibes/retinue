// The update round trip: the manifest `scripts/release-windows.ps1` writes has
// to be one an installed build can read, and it has to offer something newer
// than what is published (wave 3, `docs/plans/nod-adoption-plan.md`, idea 7).
//
// The manifest is built with the same function the release script's tool uses
// (`buildManifest`), so these cases exercise the real writer and the reader
// together, not two hand-written strings.
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'

import { buildManifest, manifestToJson } from './make-update-manifest.mjs'
import { compareVersions, ManifestError, offersAnUpdate, readManifest } from './update-round-trip.mjs'

const SIGNATURE = 'dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ=='

function manifestFor(version, overrides = {}) {
  return manifestToJson(
    buildManifest({
      version,
      notes: `Retinue ${version}`,
      pubDate: '2026-11-17T12:00:00Z',
      signature: SIGNATURE,
      url: `https://github.com/sico-vibes/retinue/releases/download/v${version}/Retinue_${version}_x64-setup.exe`,
      ...overrides,
    }),
  )
}

function refuses(text, pattern) {
  assert.throws(() => readManifest(text), (error) => {
    assert.ok(error instanceof ManifestError, `${error} is not a ManifestError`)
    assert.match(error.message, pattern)
    return true
  })
}

test('the manifest the release script writes reads back, field for field', () => {
  const manifest = readManifest(manifestFor('0.2.0-beta.5'))
  assert.equal(manifest.version, '0.2.0-beta.5')
  assert.equal(manifest.notes, 'Retinue 0.2.0-beta.5')
  assert.equal(manifest.pubDate, '2026-11-17T12:00:00Z')
  assert.equal(manifest.platform, 'windows-x86_64')
  assert.match(manifest.url, /^https:\/\/github\.com\/sico-vibes\/retinue\/releases\/download\/v0\.2\.0-beta\.5\//)
  assert.equal(manifest.signature, SIGNATURE)
})

test('a byte-order mark and surrounding whitespace do not stop the read', () => {
  const text = `\uFEFF${manifestFor('0.2.0-beta.5')}`
  assert.equal(readManifest(text).version, '0.2.0-beta.5')
})

test('the version it offers is newer than the published one', () => {
  const manifest = readManifest(manifestFor('0.2.0-beta.5'))
  assert.equal(offersAnUpdate(manifest, '0.2.0-beta.4'), true)
  assert.equal(offersAnUpdate(manifest, '0.2.0-beta.5'), false, 'the same version is not an update')
  assert.equal(offersAnUpdate(manifest, '0.2.0-beta.6'), false)
  assert.equal(offersAnUpdate(manifest, '0.2.0'), false, 'a release is newer than its beta')
  assert.equal(offersAnUpdate(readManifest(manifestFor('0.2.0')), '0.2.0-beta.9'), true)
})

test('pre-release identifiers compare the way semver says', () => {
  const ordered = [
    '0.2.0-beta.3',
    '0.2.0-beta.4',
    '0.2.0-beta.9',
    '0.2.0-beta.10',
    '0.2.0-beta.10.1',
    '0.2.0-rc.1',
    '0.2.0',
    '0.2.1',
    '0.10.0',
    '1.0.0',
  ]
  for (let index = 1; index < ordered.length; index += 1) {
    const previous = ordered[index - 1]
    const current = ordered[index]
    assert.equal(compareVersions(previous, current), -1, `${previous} < ${current}`)
    assert.equal(compareVersions(current, previous), 1, `${current} > ${previous}`)
    assert.equal(compareVersions(current, current), 0, `${current} = ${current}`)
  }
})

test('a manifest this build could not read is refused, naming the field', () => {
  refuses(manifestFor('0.2.0-beta.5').replace('"windows-x86_64"', '"linux-x86_64"'), /offers no windows-x86_64 build/)
  refuses(manifestFor('0.2.0-beta.5').replace('https://github.com', 'http://github.com'), /not an https URL/)
  refuses(manifestFor('0.2.0-beta.5').replace(/"signature": "[^"]*"/, '"signature": "  "'), /no signature/)
  refuses(manifestFor('0.2.0-beta.5').replace('"version": "0.2.0-beta.5"', '"version": "v0.2.0-beta.5"'), /not semver/)
  refuses(manifestFor('0.2.0-beta.5').replace('"pub_date": "2026-11-17T12:00:00Z"', '"pub_date": "yesterday"'), /ISO 8601/)
  refuses(manifestFor('0.2.0-beta.5').replace('"notes": "Retinue 0.2.0-beta.5"', '"notes": 7'), /notes are not text/)
  refuses('not json', /not JSON/)
  refuses('[]', /not a JSON object/)
})

test('the command line refuses a version that is not newer', () => {
  const directory = mkdtempSync(join(tmpdir(), 'retinue-update-'))
  try {
    const path = join(directory, 'latest.json')
    writeFileSync(path, manifestFor('0.2.0-beta.5'))
    const script = fileURLToPath(new URL('./update-round-trip.mjs', import.meta.url))
    const ok = spawnSync(process.execPath, [script, '--manifest', path, '--installed', '0.2.0-beta.4'], { encoding: 'utf8' })
    assert.equal(ok.status, 0, ok.stderr)
    assert.match(ok.stdout, /newer than the published 0\.2\.0-beta\.4/)
    const same = spawnSync(process.execPath, [script, '--manifest', path, '--installed', '0.2.0-beta.5'], { encoding: 'utf8' })
    assert.equal(same.status, 1, 'the same version must be refused')
    assert.match(same.stderr, /not newer than the published 0\.2\.0-beta\.5/)
    const shapeOnly = spawnSync(process.execPath, [script, '--manifest', path], { encoding: 'utf8' })
    assert.equal(shapeOnly.status, 0, shapeOnly.stderr)
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})
