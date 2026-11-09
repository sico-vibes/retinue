#!/usr/bin/env node
// Write the winget manifests for one Retinue release, ready to submit to microsoft/winget-pkgs.
//
//   node packaging/winget/make-manifests.mjs --tag v0.1.0-beta.51 [--repo owner/name] [--out dir]
//
// It reads the release's NSIS installer and its SHA-256 from GitHub (`gh release view`), so the
// manifest always matches the published file. Offline: pass --version, --url and --sha256 instead.
// See docs/operations/distribution.md for submitting.

import { execFileSync } from 'node:child_process'
import { mkdirSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const PACKAGE = {
  identifier: 'SicoVibes.Retinue',
  publisher: 'Bassem Hesham',
  name: 'Retinue',
  moniker: 'retinue',
  license: 'MIT',
  shortDescription: 'Your company of AI employees: a Windows app that runs Claude Code, Codex and OpenCode as a team.',
  description:
    'Retinue is a Windows app that runs like a company: a CEO, specialists hired into your projects, and you as the owner. ' +
    'It runs on the coding agents you already use (Claude Code, Codex and OpenCode), each with its own sign-in, and keeps ' +
    'conversations, approvals and usage on your computer.',
  tags: ['ai', 'agents', 'claude-code', 'codex', 'opencode', 'developer-tools'],
}
const SCHEMA = '1.10.0'

const yamlString = (value) => JSON.stringify(String(value))

/** The three manifests (version, installer, default locale) as { fileName: text }. */
export function buildManifests({ version, url, sha256, repo, releaseNotesUrl }) {
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) throw new Error(`Not a release version: ${version}`)
  if (!/^https:\/\/\S+\.exe$/.test(url)) throw new Error(`The installer URL must be an https link to an .exe: ${url}`)
  if (!/^[0-9a-f]{64}$/i.test(sha256)) throw new Error('The SHA-256 must be 64 hex characters.')
  const id = PACKAGE.identifier
  const header = (type) => `# yaml-language-server: $schema=https://aka.ms/winget-manifest.${type}.${SCHEMA}.schema.json\n`
  const repoUrl = `https://github.com/${repo}`
  return {
    [`${id}.yaml`]:
      header('version') +
      `PackageIdentifier: ${id}\nPackageVersion: ${version}\nDefaultLocale: en-US\nManifestType: version\nManifestVersion: ${SCHEMA}\n`,
    [`${id}.installer.yaml`]:
      header('installer') +
      `PackageIdentifier: ${id}\nPackageVersion: ${version}\n` +
      // Tauri's NSIS installer: per-user, silent with /S, and it fetches WebView2 itself if needed.
      `InstallerType: nullsoft\nScope: user\nInstallModes:\n- interactive\n- silent\nUpgradeBehavior: install\n` +
      `ReleaseDate: ${new Date().toISOString().slice(0, 10)}\n` +
      `Installers:\n- Architecture: x64\n  InstallerUrl: ${url}\n  InstallerSha256: ${sha256.toUpperCase()}\n` +
      `ManifestType: installer\nManifestVersion: ${SCHEMA}\n`,
    [`${id}.locale.en-US.yaml`]:
      header('defaultLocale') +
      `PackageIdentifier: ${id}\nPackageVersion: ${version}\nPackageLocale: en-US\n` +
      `Publisher: ${yamlString(PACKAGE.publisher)}\nPublisherUrl: ${repoUrl}\nPublisherSupportUrl: ${repoUrl}/issues\n` +
      `PrivacyUrl: ${repoUrl}/blob/main/PRIVACY.md\nPackageName: ${PACKAGE.name}\nPackageUrl: ${repoUrl}\n` +
      `License: ${PACKAGE.license}\nLicenseUrl: ${repoUrl}/blob/main/LICENSE\n` +
      `ShortDescription: ${yamlString(PACKAGE.shortDescription)}\nDescription: ${yamlString(PACKAGE.description)}\n` +
      `Moniker: ${PACKAGE.moniker}\nTags:\n${PACKAGE.tags.map((tag) => `- ${tag}`).join('\n')}\n` +
      `ReleaseNotesUrl: ${releaseNotesUrl ?? `${repoUrl}/releases/tag/v${version}`}\n` +
      `ManifestType: defaultLocale\nManifestVersion: ${SCHEMA}\n`,
  }
}

/** The installer asset of a GitHub release, from `gh release view --json assets`. */
export function pickInstaller(assets, version) {
  const found = assets.filter((asset) => asset.name.endsWith('_x64-setup.exe') && asset.name.includes(`_${version}_`))
  if (found.length !== 1) throw new Error(`Expected one x64 setup installer for ${version}, found ${found.length}.`)
  const [asset] = found
  const sha256 = String(asset.digest ?? '').replace(/^sha256:/, '')
  return { url: asset.url, sha256 }
}

function parseArgs(argv) {
  const args = {}
  for (let i = 0; i < argv.length; i += 1) {
    if (!argv[i].startsWith('--')) throw new Error(`Unexpected argument ${argv[i]}`)
    args[argv[i].slice(2)] = argv[i + 1]
    i += 1
  }
  return args
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  try {
    const args = parseArgs(process.argv.slice(2))
    const repo = args.repo ?? 'sico-vibes/retinue'
    let { version, url, sha256 } = args
    if (args.tag) {
      version = args.tag.replace(/^v/, '')
      const json = execFileSync('gh', ['release', 'view', args.tag, '--repo', repo, '--json', 'assets'], { encoding: 'utf8' })
      ;({ url, sha256 } = pickInstaller(JSON.parse(json).assets, version))
    }
    if (!version || !url || !sha256) throw new Error('Pass --tag, or all of --version, --url and --sha256.')
    const id = PACKAGE.identifier
    const [publisherPart, namePart] = id.split('.')
    const out = resolve(args.out ?? join('output', 'winget', 'manifests', id[0].toLowerCase(), publisherPart, namePart, version))
    mkdirSync(out, { recursive: true })
    for (const [name, text] of Object.entries(buildManifests({ version, url, sha256, repo }))) writeFileSync(join(out, name), text)
    console.log(`Wrote the winget manifests for ${id} ${version} to ${out}`)
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(1)
  }
}
