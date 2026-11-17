// Proves that every exception in `.gitleaks.toml` is still a narrow one: a
// planted secret beside it, one path away, or in another provider's format must
// still fail the scan. A secret scan whose allowlist nobody tests is how a
// committed key stops being noticed.
//
// Design from n8n-io/nod's `.github/scripts/check-secret-config.py` (Apache-2.0,
// read at commit 47cfee3 on 16 November 2026).
//
// Usage:
//   node scripts/check-secret-config.mjs <path-to-gitleaks>
//   GITLEAKS_PATH=<path> node scripts/check-secret-config.mjs
//
// Needs a `gitleaks` binary (8.30.1 is what CI downloads); it writes only into a
// temporary folder and removes it.

import { spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const scanner = process.argv[2] || process.env.GITLEAKS_PATH || 'gitleaks'
const root = fileURLToPath(new URL('..', import.meta.url))
const config = join(root, '.gitleaks.toml')

const checks = []
function expect(description, { path, content }, expectedRules) {
  checks.push({ description, path, content, expectedRules })
}

// A deterministic key-shaped value, so a failure is reproducible.
const candidate = Array.from({ length: 64 }, (_, index) => 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789'[(index * 29 + 7) % 62]).join('')
// Written in pieces so this file is not itself a secret-scanning finding: the
// scan runs over the repository, and a complete token literal here would trip it.
const jwt = ['eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9', 'eyJzdWIiOiIxMjM0NTY3ODkwIn0', 'dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U'].join('.')
// A fake bearer, assembled so this file is not itself a finding.
const fakeBearer = ['sk', 'placeholder', 'preview'].join('-') + '-' + candidate.slice(0, 20)

// A planted generic key in source still fails.
expect('a planted key in source is found', { path: 'crates/retinue-daemon/src/planted.rs', content: `let api_key = "${candidate}";\n` }, ['generic-api-key'])
// The same key is excused in a test file, because only generic-api-key is.
expect('a synthetic key in a test file is excused', { path: 'crates/retinue-daemon/src/phase1b_tests.rs', content: `let token = "${candidate}";\n` }, [])
// ...but a provider's own key format is not excused anywhere.
const github = 'ghp_' + candidate.slice(0, 36)
expect('a GitHub token in a test file still fails', { path: 'crates/retinue-daemon/src/phase1b_tests.rs', content: `let token = "${github}";\n` }, ['github-pat'])
expect('a GitHub token in a history report still fails', { path: 'docs/history/wave/x.md', content: `token ${github}\n` }, ['github-pat'])
// A generic key in a history report is excused (dated reports quote examples).
expect('an example key in a history report is excused', { path: 'docs/history/wave/x.md', content: `api_key = "${candidate}"\n` }, [])
// The generated viewer repeats whatever the pages hold.
expect('a key in the generated viewer is excused', { path: 'docs/DOCUMENTATION.html', content: `<p>api_key = "${candidate}"</p>\n` }, [])
// The JWT exception names one test-vector line, not the file.
expect('the RFC example token is excused where it is used', { path: 'crates/retinue-daemon/src/remote/push.rs', content: `    const RFC8292_JWT: &str = "${jwt}";\n` }, [])
expect('another token in the same file still fails', { path: 'crates/retinue-daemon/src/remote/push.rs', content: `    let raw = "${jwt}";\n` }, ['jwt'])
// The redaction fixture is excused on its own line, in its own file.
const redaction = `let line = "request failed session=abc-123 token=${['0631d4ce'].join('')} x-api-key=${['AKIA', '1234567890'].join('')}";\n`
expect('the redaction fixture is excused in its own file', { path: 'crates/retinue-daemon/src/diagnostics.rs', content: redaction }, [])
expect('the same values in another file fail', { path: 'crates/retinue-daemon/src/elsewhere.rs', content: redaction }, ['generic-api-key'])
// A preview fixture may draw a masked command; the window may not hold one.
const curl = `const cmd = 'curl -H "Authorization: Bearer ${fakeBearer}" https://example.com'\n`
expect('a preview fixture may draw a masked command', { path: 'apps/desktop/src/preview/scenes.ts', content: curl }, [])
expect('a window module may not', { path: 'apps/desktop/src/ui/scenes.ts', content: curl }, ['curl-auth-header'])

let failed = 0
for (const check of checks) {
  const directory = mkdtempSync(join(tmpdir(), 'retinue-secret-config-'))
  try {
    const file = join(directory, check.path)
    mkdirSync(dirname(file), { recursive: true })
    writeFileSync(file, check.content)
    const report = join(directory, 'findings.json')
    const result = spawnSync(
      scanner,
      ['dir', '--config', config, '--no-banner', '--redact', '--report-format', 'json', '--report-path', report, '.'],
      { cwd: directory, encoding: 'utf8' },
    )
    if (result.error) throw new Error(`the scanner could not run (${scanner}): ${result.error.message}`)
    const findings = readFileSync(report, 'utf8').trim() ? JSON.parse(readFileSync(report, 'utf8')) : []
    const rules = [...new Set(findings.map((finding) => finding.RuleID))].sort()
    const wanted = [...check.expectedRules].sort()
    const ok = JSON.stringify(rules) === JSON.stringify(wanted)
    if (!ok) failed += 1
    console.log(`${ok ? 'ok  ' : 'FAIL'} ${check.description}${ok ? '' : `: expected [${wanted.join(', ')}], found [${rules.join(', ')}]`}`)
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
}

if (failed) {
  console.error(`\n${failed} of ${checks.length} secret-scanning exceptions behave wrongly (.gitleaks.toml).`)
  process.exitCode = 1
} else {
  console.log(`\nSecret-scanning configuration regression passed (${checks.length} checks).`)
}
