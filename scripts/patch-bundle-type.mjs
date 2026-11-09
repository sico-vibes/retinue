#!/usr/bin/env node
// Mark a Tauri main executable as coming from the NSIS installer, before it is code signed.
//
// `tauri bundle` normally writes this marker into the executable itself, which would break a
// signature made earlier. The signed release build (`.github/workflows/release.yml`) therefore
// patches the executable here, has it signed, and then bundles with `--no-binary-patching`.
// The updater reads the marker to know which kind of package it is updating.
//
// Usage: node scripts/patch-bundle-type.mjs <path-to-exe> [nsis|msi]

import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

export const UNKNOWN = Buffer.from('__TAURI_BUNDLE_TYPE_VAR_UNK')
export const MARKERS = {
  nsis: Buffer.from('__TAURI_BUNDLE_TYPE_VAR_NSS'),
  msi: Buffer.from('__TAURI_BUNDLE_TYPE_VAR_MSI'),
}

/** Returns a patched copy of `data`, or throws if the marker is missing, repeated or the type unknown. */
export function patchBundleType(data, type = 'nsis') {
  const marker = MARKERS[type]
  if (!marker) throw new Error(`Unknown bundle type "${type}". Use nsis or msi.`)
  const at = data.indexOf(UNKNOWN)
  if (at < 0) {
    if (data.indexOf(marker) >= 0) throw new Error('The executable is already marked; build it again with --no-bundle.')
    throw new Error('The bundle-type marker was not found. Is this a Tauri 2 main executable?')
  }
  if (data.indexOf(UNKNOWN, at + 1) >= 0) throw new Error('The bundle-type marker appears more than once.')
  const out = Buffer.from(data)
  marker.copy(out, at)
  return out
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const [file, type = 'nsis'] = process.argv.slice(2)
  if (!file) {
    console.error('Usage: node scripts/patch-bundle-type.mjs <path-to-exe> [nsis|msi]')
    process.exit(2)
  }
  try {
    writeFileSync(file, patchBundleType(readFileSync(file), type))
    console.log(`Marked ${file} as ${type}.`)
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(1)
  }
}
