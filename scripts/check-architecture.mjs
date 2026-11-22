// The import and dependency boundaries this repository keeps, as a check that
// fails a build instead of a paragraph someone has to remember.
//
// Design borrowed from n8n-io/nod's `scripts/check-architecture.mjs` (Apache-2.0,
// read at commit 47cfee3 on 16 November 2026): a short table of allowed edges plus
// a walk over every source file. Our rules are our own; see the tables below.
//
// Usage:
//   node scripts/check-architecture.mjs            # check the working tree
//   node scripts/check-architecture.mjs --json     # machine-readable result
//
// Every rule is either true today or carries an allowlist entry with its reason.
// An allowlist entry that matched nothing fails the check, so the exceptions
// cannot rot. Keep the table short: a rule that is wrong is more expensive than
// a boundary left unchecked.

import { readdirSync, readFileSync, statSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { pathToFileURL } from 'node:url'

/** A crate may depend only on these repository crates (one row each). */
const CRATE_DEPENDENCIES = {
  'crates/retinue-agent-core': [],
  'crates/retinue-budget': [],
  'crates/retinue-process': [],
  'crates/retinue-protocol': [],
  'crates/retinue-security': [],
  'crates/retinue-speech': [],
  'crates/retinue-usage': [],
  'crates/retinue-wsl': [],
  'crates/retinue-storage': ['retinue-usage'],
  'crates/retinue-runtime': ['retinue-process', 'retinue-protocol'],
  'crates/retinue-adapter-claude': ['retinue-agent-core', 'retinue-process'],
  'crates/retinue-adapter-codex': ['retinue-agent-core', 'retinue-process'],
  'crates/retinue-adapter-acp': ['retinue-agent-core', 'retinue-process', 'retinue-runtime'],
  'crates/retinue-adapter-opencode': ['retinue-adapter-acp', 'retinue-agent-core'],
  'crates/retinue-adapter-pi': ['retinue-agent-core', 'retinue-process'],
  'crates/retinue-daemon': [
    'retinue-adapter-acp',
    'retinue-adapter-claude',
    'retinue-adapter-codex',
    'retinue-adapter-pi',
    'retinue-agent-core',
    'retinue-process',
    'retinue-protocol',
    'retinue-runtime',
    'retinue-security',
    'retinue-storage',
    'retinue-usage',
  ],
  // The native shell uses the speech crate for the dictation island, and
  // retinue-process so that every child it starts is hidden: a GUI-subsystem
  // parent gives a console program a console window (21 November 2026).
  'apps/desktop/src-tauri': ['retinue-process', 'retinue-speech'],
}

/** Only these crates may open an HTTP or WebSocket client of their own. */
const HTTP_CLIENTS = {
  'retinue-daemon': 'the one crate that talks to providers, machines, the linked computer and Tailscale',
  'retinue-speech': 'opt-in cloud dictation, the only outbound call the speech crate makes',
  'src-tauri': "the native shell's loopback calls to the daemon, which is the only address it uses",
}
const HTTP_CRATES = ['reqwest', 'hyper', 'ureq', 'tungstenite', 'tokio-tungstenite']

/** Exceptions, each with the reason it is one. A stale entry fails the check. */
export const ALLOWLIST = [
  {
    rule: 'daemon-network',
    path: 'crates/retinue-daemon/src/console.rs',
    reason: "the console client's call to its own daemon on loopback, with the console token a headless start writes (17 November 2026)",
  },
  {
    rule: 'daemon-network',
    path: 'crates/retinue-daemon/src/channels.rs',
    reason: "Telegram's Bot API and Meta's WhatsApp Cloud API, the chat channels to the CEO (owner decision, 16 November 2026)",
  },
  {
    rule: 'daemon-network',
    path: 'crates/retinue-daemon/src/connectors.rs',
    reason: 'the MCP client for servers the owner added: HTTPS only, DNS pinned to the checked address, no cross-origin redirect',
  },
  {
    rule: 'daemon-network',
    path: 'crates/retinue-daemon/src/connectors_oauth.rs',
    reason: 'OAuth discovery, dynamic registration and the token endpoint for an owner-added connector, under the same HTTPS-only, pinned-DNS rules as connectors.rs',
  },
  {
    rule: 'daemon-network',
    path: 'crates/retinue-daemon/src/browser.rs',
    reason: 'controls the browser Retinue started, on a loopback or linked address',
  },
  {
    rule: 'daemon-network',
    path: 'crates/retinue-daemon/src/computer.rs',
    reason: 'the linked computer (Harbor) REST calls; no provider or owner credential is sent',
  },
  {
    rule: 'daemon-network',
    path: 'crates/retinue-daemon/src/preview.rs',
    reason: 'a loopback probe of a local dev server, attributed to a conversation by process ancestry',
  },
  {
    rule: 'daemon-network',
    path: 'crates/retinue-daemon/src/quota_sources.rs',
    reason: 'provider usage endpoints, with the one read-only sign-in token held for that request',
  },
  {
    rule: 'daemon-network',
    path: 'crates/retinue-daemon/src/machines.rs',
    reason: 'passes a request method into machine_client, which owns the client and the pinned certificate',
  },
  {
    rule: 'daemon-network',
    path: 'crates/retinue-daemon/src/remote/',
    reason: 'the Remote gateway, its push sender and its native TLS listener',
  },
  {
    rule: 'daemon-network',
    path: 'crates/retinue-daemon/src/machine_client.rs',
    reason: 'the only code that talks to another Retinue machine (web PKI or a pinned certificate)',
  },
  {
    rule: 'websocket',
    path: 'apps/desktop/src/remote/remoteTransport.ts',
    reason: 'the phone PWA has its own transport; it cannot import the desktop one',
  },
]

const SOURCE_FILE = /\.(ts|tsx)$/
// Test code may reach wherever it likes: it is not shipped, and a fixture is
// there to stand in for something the production graph may not import.
const isTestFile = (path) => /(?:\.test\.|\.spec\.|_tests\.rs$|\/tests\.rs$)/.test(path)

function walk(dir, out = []) {
  let entries
  try {
    entries = readdirSync(dir, { withFileTypes: true })
  } catch {
    return out
  }
  for (const entry of entries) {
    if (['node_modules', 'target', 'dist', 'dist-remote'].includes(entry.name)) continue
    const full = join(dir, entry.name)
    if (entry.isDirectory()) walk(full, out)
    else if (entry.isFile()) out.push(full)
  }
  return out
}

function isFile(path) {
  try {
    return statSync(path).isFile()
  } catch {
    return false
  }
}

/** One `Cargo.toml`'s named dependencies, per section (`[dependencies]`, …). */
function crateDependencies(manifest) {
  const sections = new Map()
  let section = null
  for (const line of readFileSync(manifest, 'utf8').split(/\r?\n/)) {
    const header = line.match(/^\[([^\]]+)\]$/)
    if (header) {
      section = header[1].trim()
      continue
    }
    if (!section) continue
    const name = line.match(/^\s*([A-Za-z0-9_-]+)\s*=\s*/)
    if (!name) continue
    if (!sections.has(section)) sections.set(section, new Set())
    sections.get(section).add(name[1])
  }
  return sections
}

const isDependencySection = (section) => /^(dev-|build-)?dependencies$/.test(section)

export function checkArchitecture({ root = process.cwd() } = {}) {
  const failures = []
  const used = new Set()
  const fail = (rule, path, message) => failures.push({ rule, path, message })
  const allow = (rule, path) => {
    for (const entry of ALLOWLIST) {
      if (entry.rule !== rule) continue
      if (entry.path.endsWith('/') ? path.startsWith(entry.path) : path === entry.path) {
        used.add(`${entry.rule}:${entry.path}`)
        return true
      }
    }
    return false
  }

  // Rule 1: the crate dependency table. A new edge, or a new crate, is written
  // down here with the reason it needs the other crate.
  const manifests = []
  let crateNames = []
  try {
    crateNames = readdirSync(join(root, 'crates'), { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
  } catch {
    crateNames = []
  }
  for (const name of crateNames) {
    const path = join(root, 'crates', name, 'Cargo.toml')
    if (isFile(path)) manifests.push({ key: `crates/${name}`, path })
  }
  const desktopManifest = join(root, 'apps', 'desktop', 'src-tauri', 'Cargo.toml')
  if (isFile(desktopManifest)) manifests.push({ key: 'apps/desktop/src-tauri', path: desktopManifest })
  for (const { key, path } of manifests) {
    const crate = key.split('/').pop()
    const allowed = CRATE_DEPENDENCIES[key]
    if (!allowed) {
      fail('crate-table', key, 'a new crate must be added to CRATE_DEPENDENCIES in scripts/check-architecture.mjs, with the crates it may use')
      continue
    }
    const sections = crateDependencies(path)
    for (const [section, names] of sections) {
      if (!isDependencySection(section)) continue
      for (const name of names) {
        // Rule 2: an HTTP client belongs to the daemon (and the speech crate).
        if (HTTP_CRATES.includes(name) && !HTTP_CLIENTS[crate]) {
          fail('http-client', key, `${section} names ${name}: only ${Object.keys(HTTP_CLIENTS).join(' and ')} may open a connection of their own`)
        }
        if (!name.startsWith('retinue-') || name === crate) continue
        if (!allowed.includes(name)) {
          fail('crate-table', key, `${section} names ${name}, which is not in this crate's row: add it with a reason, or use the crate that owns it`)
        }
      }
    }
  }

  // Rule 3: inside the daemon, only the named modules hold a network client.
  // The gate catches a new module reaching for one; the modules that
  // legitimately do carry an allowlist entry above.
  for (const file of walk(join(root, 'crates', 'retinue-daemon', 'src'))) {
    if (!file.endsWith('.rs')) continue
    const path = relative(root, file).replaceAll('\\', '/')
    if (isTestFile(path)) continue
    // `reqwest::Url` is the URL parser it re-exports, not a client.
    if (!/\breqwest::(?!Url\b)/.test(readFileSync(file, 'utf8'))) continue
    if (allow('daemon-network', path)) continue
    fail('daemon-network', path, 'builds an HTTP client outside the allowlisted modules: send it through remote/, machine_client.rs or an existing client, or add it to ALLOWLIST with a reason')
  }

  // Rules 4 and 5: the window's transport seam, and the preview harness.
  // Production code must not open its own connection, and must not import the
  // preview fixtures.
  const sourceRoot = join(root, 'apps', 'desktop', 'src')
  for (const file of walk(sourceRoot)) {
    if (!SOURCE_FILE.test(file)) continue
    const path = relative(root, file).replaceAll('\\', '/')
    const inside = relative(sourceRoot, file).replaceAll('\\', '/')
    if (isTestFile(path) || inside.startsWith('preview/')) continue
    const text = readFileSync(file, 'utf8')
    if (/\bnew\s+(?:WebSocket|EventSource)\s*\(/.test(text) && !allow('websocket', path)) {
      fail('websocket', path, 'opens a WebSocket or EventSource outside the transport seam (src/transport/, src/remote/)')
    }
    for (const match of text.matchAll(/(?:\bfrom|\bimport)\s*\(?\s*['"]([^'"]+)['"]/g)) {
      const specifier = match[1]
      if (!specifier.startsWith('.')) continue
      const target = relative(sourceRoot, dirname(file) + '/' + specifier).replaceAll('\\', '/')
      if (target === 'preview' || target.startsWith('preview/')) {
        fail('preview-leak', path, `imports the preview harness (${specifier}); fixture data belongs behind a test or in preview/ itself`)
      }
    }
  }

  // An exception nobody needs any more is a rule nobody can trust.
  for (const entry of ALLOWLIST) {
    if (!used.has(`${entry.rule}:${entry.path}`)) {
      failures.push({ rule: 'stale-allowlist', path: entry.path, message: `the ${entry.rule} exception matched nothing; remove it from ALLOWLIST` })
    }
  }

  return { failures, used: [...used] }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { failures } = checkArchitecture()
  if (process.argv.includes('--json')) {
    console.log(JSON.stringify({ ok: failures.length === 0, failures }, null, 2))
  } else if (failures.length) {
    for (const failure of failures) console.error(`${failure.path}: [${failure.rule}] ${failure.message}`)
    console.error(`\n${failures.length} architecture ${failures.length === 1 ? 'violation' : 'violations'} (scripts/check-architecture.mjs).`)
  } else {
    console.log('Architecture boundaries passed.')
  }
  if (failures.length) process.exitCode = 1
}
