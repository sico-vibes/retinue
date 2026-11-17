// The mechanical half of formatting, as a gate: encoding, a BOM, a stray NUL,
// trailing whitespace, and line endings that disagree inside one file. It is
// what a reviewer cannot see in a diff and what a scripted edit quietly breaks
// (`references/lessons.md`, 2026-11-14: a PowerShell round-trip put a BOM in
// `browser.rs` and turned every em dash into mojibake).
//
// What it deliberately does not do: reformat code. The tree is hand-formatted
// (dense one-line JSX, long lines, no `cargo fmt` gate), so a Prettier pass
// would rewrite every file once and fight the house style afterwards. A full
// formatter needs the owner's decision on one large commit; this check is the
// part that can be turned on today and never has to change the code.
//
// Usage:
//   node scripts/check-format.mjs             # every tracked code and config file
//   node scripts/check-format.mjs --staged    # staged additions and modifications
//   node scripts/check-format.mjs --files a b # the named files
//
// Markdown is out of scope on purpose: two trailing spaces at the end of a line
// are a hard line break, and `docs/library/` is generated from `docs/`.

import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'

/** Code and configuration only; see the header for why Markdown is absent. */
const CODE_FILE = /\.(?:ts|tsx|js|mjs|cjs|css|rs|toml|json|ya?ml|ps1|sh|cmd)$/i

/** Exceptions, each with the reason it is one. A stale entry fails the check. */
export const ALLOWLIST = [
  {
    rule: 'nul',
    path: 'crates/retinue-storage/src/lib.rs',
    reason: "one NUL byte inside a char literal that checks a seed for control characters: it is code, not corruption",
  },
]

/**
 * `full` says whether `files` is every tracked file. Only a whole-tree pass can
 * judge the allowlist: a file that is not being checked is not evidence that its
 * exception is stale, which is why the pre-commit hook (`--staged`) never
 * reports one.
 */
export function checkFormat({ root = process.cwd(), files, full = true } = {}) {
  const failures = []
  const used = new Set()
  const checked = []

  const allow = (rule, path) => {
    const entry = ALLOWLIST.find((candidate) => candidate.rule === rule && candidate.path === path)
    if (entry) used.add(`${entry.rule}:${entry.path}`)
    return Boolean(entry)
  }

  for (const { path, bytes } of files) {
    if (!CODE_FILE.test(path)) continue
    checked.push(path)
    const fail = (rule, message) => failures.push({ rule, path, message })

    if (bytes.length === 0) continue
    if (bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) fail('bom', 'starts with a byte-order mark; write it as UTF-8 without one')
    if (bytes.includes(0) && !allow('nul', path)) fail('nul', 'contains a NUL byte; a text file with one is usually a broken scripted edit')

    let text
    try {
      text = new TextDecoder('utf-8', { fatal: true }).decode(bytes)
    } catch {
      fail('encoding', 'is not valid UTF-8')
      continue
    }

    const crlf = (text.match(/\r\n/g) ?? []).length
    const loneLf = (text.match(/(?<!\r)\n/g) ?? []).length
    const loneCr = (text.match(/\r(?!\n)/g) ?? []).length
    if (loneCr) fail('line-endings', 'has a carriage return outside a CRLF pair')
    else if (crlf && loneLf) fail('line-endings', `mixes CRLF (${crlf}) and LF (${loneLf}) line endings`)

    const lines = text.split(/\r?\n/)
    lines.forEach((line, index) => {
      if (/[ \t]+$/.test(line)) fail('trailing-whitespace', `line ${index + 1} ends with whitespace`)
    })
  }

  for (const entry of ALLOWLIST) {
    if (!full) break
    if (!used.has(`${entry.rule}:${entry.path}`)) {
      failures.push({ rule: 'stale-allowlist', path: entry.path, message: `the ${entry.rule} exception matched nothing; remove it from ALLOWLIST` })
    }
  }

  return { checked: checked.length, failures }
}

function trackedFiles(root) {
  const paths = execFileSync('git', ['ls-files', '-z'], { cwd: root, encoding: 'utf8' }).split('\0').filter(Boolean)
  return paths.filter((path) => CODE_FILE.test(path)).map((path) => ({ path, bytes: readFileSync(`${root}/${path}`) }))
}

/** What will be committed, not what happens to be in the working tree. */
function stagedFiles(root) {
  const paths = execFileSync('git', ['diff', '--cached', '--name-only', '--diff-filter=ACMR', '-z'], { cwd: root, encoding: 'utf8' })
    .split('\0')
    .filter(Boolean)
    .filter((path) => CODE_FILE.test(path))
  return paths.map((path) => ({
    path,
    bytes: execFileSync('git', ['show', `:${path}`], { cwd: root, maxBuffer: 64 * 1024 * 1024 }),
  }))
}

const invokedDirectly = process.argv[1]?.replaceAll('\\', '/').endsWith('scripts/check-format.mjs')
if (invokedDirectly) {
  const root = process.cwd()
  const args = process.argv.slice(2)
  let files
  if (args.includes('--staged')) files = stagedFiles(root)
  else if (args.includes('--files')) {
    const named = args.slice(args.indexOf('--files') + 1)
    files = named.map((path) => ({ path, bytes: readFileSync(`${root}/${path.replaceAll('\\', '/')}`) }))
  } else files = trackedFiles(root)

  const { checked, failures } = checkFormat({ root, files, full: !args.includes('--staged') && !args.includes('--files') })
  if (args.includes('--json')) {
    console.log(JSON.stringify({ ok: failures.length === 0, checked, failures }, null, 2))
  } else if (failures.length) {
    for (const failure of failures) console.error(`${failure.path}: [${failure.rule}] ${failure.message}`)
    console.error(`\n${failures.length} formatting ${failures.length === 1 ? 'problem' : 'problems'} in ${checked} ${checked === 1 ? 'file' : 'files'} (scripts/check-format.mjs).`)
  } else {
    console.log(`Formatting check passed (${checked} ${checked === 1 ? 'file' : 'files'}).`)
  }
  if (failures.length) process.exitCode = 1
}
