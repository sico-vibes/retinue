import assert from 'node:assert/strict'
import test from 'node:test'
import { PACKAGE, buildManifests, pickInstaller } from './make-manifests.mjs'

const sha = 'd6cdaa9782b1a35d6cf87d833beb3abbde852d4067fc560cf4f8c66538b0e57f'
const base = {
  version: '0.1.0-beta.51',
  url: 'https://github.com/o/r/releases/download/v0.1.0-beta.51/Retinue_0.1.0-beta.51_x64-setup.exe',
  sha256: sha,
  repo: 'o/r',
}

test('writes the three manifests with matching identifier and version', () => {
  const files = buildManifests(base)
  assert.deepEqual(Object.keys(files).sort(), [
    `${PACKAGE.identifier}.installer.yaml`,
    `${PACKAGE.identifier}.locale.en-US.yaml`,
    `${PACKAGE.identifier}.yaml`,
  ])
  for (const text of Object.values(files)) {
    assert.match(text, new RegExp(`^PackageIdentifier: ${PACKAGE.identifier.replace('.', '\\.')}$`, 'm'))
    assert.match(text, /^PackageVersion: 0\.1\.0-beta\.51$/m)
  }
})

test('the installer manifest describes the per-user NSIS installer', () => {
  const text = buildManifests(base)[`${PACKAGE.identifier}.installer.yaml`]
  assert.match(text, /^InstallerType: nullsoft$/m)
  assert.match(text, /^Scope: user$/m)
  assert.match(text, new RegExp(`InstallerSha256: ${sha.toUpperCase()}`))
  assert.match(text, /InstallerUrl: https:\/\/github\.com\/o\/r\/releases\/download\//)
})

test('the locale manifest links the licence and privacy page', () => {
  const text = buildManifests(base)[`${PACKAGE.identifier}.locale.en-US.yaml`]
  assert.match(text, /^License: MIT$/m)
  assert.match(text, /^PrivacyUrl: https:\/\/github\.com\/o\/r\/blob\/main\/PRIVACY\.md$/m)
})

test('rejects bad input', () => {
  assert.throws(() => buildManifests({ ...base, version: 'beta' }), /Not a release version/)
  assert.throws(() => buildManifests({ ...base, url: 'http://x/a.exe' }), /https link/)
  assert.throws(() => buildManifests({ ...base, sha256: 'abc' }), /64 hex/)
})

test('picks the one installer for the version from the release assets', () => {
  const assets = [
    { name: 'Retinue_0.1.0-beta.51_x64-setup.exe', url: 'https://x/Retinue_0.1.0-beta.51_x64-setup.exe', digest: `sha256:${sha}` },
    { name: 'Retinue_0.1.0-beta.51_x64-setup.exe.sig', url: 'https://x/sig', digest: 'sha256:00' },
    { name: 'latest.json', url: 'https://x/latest.json', digest: 'sha256:00' },
  ]
  assert.deepEqual(pickInstaller(assets, '0.1.0-beta.51'), { url: 'https://x/Retinue_0.1.0-beta.51_x64-setup.exe', sha256: sha })
  assert.throws(() => pickInstaller(assets, '0.1.0-beta.50'), /found 0/)
})
