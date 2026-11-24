import type { SharedSettings } from '../ui/ProjectSettingsDialog'
import { previewBackdrop } from './frameState'
import { clipboardReadFailures, pairingFailures, scaffoldFailures, workflowError } from './demoWorkflowErrors'
import type { Connector, ConnectorSignIn } from '../ui/ConnectorsPage'
import type { LearnedSkill } from '../ui/SkillsPage'
import type { DiagnosticsPreview } from '../tauri'
import type { ModelChangePlan } from '../types'
import { normalizeDelegation, type DelegationRecord, type DelegationStatus } from '../ui/delegations'
import type { ProjectSuggestion } from '../ui/ProjectSuggestions'
import type { MessageSearchResult } from '../ui/ExperimentalFeatures'
import type { ApprovalRule } from '../ui/ApprovalRulesPage'
import type { Environment } from '../ui/SessionRail'
import type { Workspace, Checkpoints } from '../ui/SessionRailExtras'
// Development-only stand-in for src/tauri.ts, wired by vite.preview.config.ts
// so the real App can be inspected in a browser without the daemon. Every
// value below is a visual fixture, not product data.
import { DEPARTMENTS, PRESETS } from '../team/presets'
import { handleFixtureTaskRpc } from './fixtureTasks'
import { EXPERIMENTS } from '../ui/experiments'
import type { HealthCheck } from '../ui/HealthPage'
import { recordUnknownRpc } from './demoDiagnostics'
import type { Agent, ChatMessage, DaemonEvent, ImportRow, DaemonSupervisor, MessageMeta, PermissionRequest, Project, Runtime, Session, Snapshot, WindowBackdrop } from '../types'
import { answerUsageAnalytics } from '../ui/analyticsFixtures'
import { normalizeAnalytics } from '../ui/analyticsFormat'
import { parseCapabilities, parseExecSnapshot, parseModelCatalog } from '../executionContract'
import type { UsageAnalyticsRequest } from '../analyticsTypes'
import { IDLE_UPDATE_PHASE, parseUpdateChannel, type UpdateChannel, type UpdateInfo, type UpdatePhaseState, type UpdateProgress, type UpdatesState } from '../updatesContract'
import { entityNoun } from '../ui/vocabulary'
import { themeStyle } from '../blob/retinue'
import { REMOTE_METHODS } from '../remote/remoteContract'
import type { RemoteMethod, RemoteStatus, RemoteTailscaleState } from '../remote/remoteContract'
import { createRemoteSimulator } from './remoteFixture'
import { SCENE_SESSION_ID, sceneFor } from './fixtureScenes'
import type { RemoteSeed } from './remoteFixture'

type Unlisten = () => void
const noop: Unlisten = () => undefined
let discoveryAttempts = 0
const pendingDeletes = new Map<string, { deadline: number; timer: number; archived: boolean }>()
const fixtureSettings: Record<string, unknown> = { 'notifications.enabled': true, 'claude.enableWorkflows': true, ...(new URLSearchParams(location.search).has('providers-off') ? { 'runtimes.disabled': ['cursor', 'grok'] } : {}) }
const daemonListeners = new Set<(event: DaemonEvent) => void>()
const minutesAgo = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString()
const flags = () => new URLSearchParams(location.search)
const fixturePreset = () => flags().get('preset')
const setPreviewSelection = (agentId: string) => {
  try { localStorage.setItem('retinue.selectedAgentId', agentId) } catch { /* Preview selection is optional. */ }
}

// Remote mode preview: one shared simulator, seeded once from the URL. Flags are
// `?remote=off|on|<tailscale state>` and `?devices=0|1|3`; the default is off
// with no devices so the empty state is what people see first. A Tailscale state
// (`not_installed`, `starting`, `not_running`, `logged_out`, `https_disabled`,
// `port_in_use`, `funnel_on`) shows the host refusing to start and naming the
// fix; `on` and `ready` seed a working serve address.
const remoteSimulator = createRemoteSimulator()
let remoteFixtureSeeded = false
const REMOTE_PREVIEW_FLAGS = ['off', 'on', 'ready', 'not_installed', 'starting', 'not_running', 'logged_out', 'https_disabled', 'port_in_use', 'funnel_on'] as const
const remoteFixtureSeed = (): RemoteSeed => {
  const search = flags()
  const devicesParam = search.get('devices')
  const requested = search.get('remote') ?? 'off'
  const flag = (REMOTE_PREVIEW_FLAGS as readonly string[]).includes(requested) ? requested : 'off'
  const tailscale: RemoteTailscaleState = flag === 'on' || flag === 'off' ? 'ready' : flag as RemoteTailscaleState
  return {
    enabled: flag === 'on' || flag === 'ready',
    method: search.get('remote-method') === 'native' ? 'native' : 'tailscale',
    devices: devicesParam === '1' ? 1 : devicesParam === '3' ? 3 : 0,
    tailscale,
  }
}
function ensureRemoteFixture() {
  if (remoteFixtureSeeded) return
  remoteFixtureSeeded = true
  remoteSimulator.reset(remoteFixtureSeed())
}
remoteSimulator.subscribe((event) => { notifyFixtureDaemon(event.type, event.payload as Record<string, unknown>) })

const baseRuntimes: Runtime[] = [
  { id: 'runtime-codex', provider: 'codex', status: 'online', protocolFamily: 'codex_app_server', version: 'codex-cli 0.159.3', authState: 'signed_in' },
  { id: 'runtime-claude', provider: 'claude', status: 'online', protocolFamily: 'claude_stream', version: '2.1.286' },
  { id: 'runtime-opencode', provider: 'opencode', status: 'online', protocolFamily: 'acp', version: '1.18.34', authState: 'authenticated', gatewayAuthStates: { opencode: 'authenticated', 'opencode-go': 'authenticated' } },
  // `?providers-off`: two optional coding agents switched off in Providers, and Antigravity on (22 November 2026).
  ...(flags().has('providers-off') ? [{ id: 'runtime-antigravity', provider: 'antigravity', status: 'online', protocolFamily: 'acp', version: '1.5.14', authState: 'authenticated' }, { id: 'runtime-cursor', provider: 'cursor', status: 'online', protocolFamily: 'acp', version: '1.9.0' }, { id: 'runtime-grok', provider: 'grok', status: 'online', protocolFamily: 'acp', version: '0.4.0' }] as Runtime[] : []),
]

// All catalog/session values below are preview-only visual fixtures. They do
// not represent installed models, provider capabilities, or user conversations.
const composerPreviewSessionId = 'session-codex-composer-preview'
/** Preview-only prompt queues, keyed by session id. */
const fixtureQueues: Record<string, { items: Array<Record<string, unknown>>; paused: boolean }> = {}
/** Preview-only automations and their runs. */
const fixtureAutomations: Array<Record<string, unknown>> = []
/** Preview-only memory notes. */
const fixtureMemory: Array<Record<string, unknown>> = []
const fixtureSkills: Array<Record<string, unknown>> = [
  { name: 'pdf', description: 'Read, fill and merge PDF files. Use when a task mentions a PDF form or document.', source: 'claude', sourcePath: null, contentSha256: 'a1', enabled: true, fileCount: 4, totalBytes: 18_400, importedAt: '2026-10-16T10:00:00Z', updatedAt: '2026-10-16T10:00:00Z', disabledFor: [], nativeFor: ['claude'] },
  { name: 'release-notes', description: 'Write release notes from the git log in the house style: short, plain, grouped by area.', source: 'codex', sourcePath: null, contentSha256: 'b2', enabled: true, fileCount: 1, totalBytes: 2_100, importedAt: '2026-10-16T10:00:00Z', updatedAt: '2026-10-16T10:00:00Z', disabledFor: [], nativeFor: [] },
  { name: 'figma-to-react', description: 'Turn a Figma frame export into React components that follow DESIGN.md.', source: 'folder', sourcePath: null, contentSha256: 'c3', enabled: false, fileCount: 7, totalBytes: 64_000, importedAt: '2026-10-16T10:00:00Z', updatedAt: '2026-10-16T10:00:00Z', disabledFor: [], nativeFor: [] },
]
const fixtureRuns: Record<string, Array<Record<string, unknown>>> = {}
const fixtureFanouts: Array<{ id: string; title: string; text: string; projectId: string | null; projectName: string | null; createdAt: string; chosenSessionId: string | null; rows: Array<{ agentId: string; sessionId: string }> }> = []
const fixtureStash: Array<{ id: string; text: string; attachments: Array<{ file: string; name: string }>; agentId: string | null; agentName: string | null; projectId: string | null; projectName: string | null; createdAt: string; updatedAt: string }> = []
const fixtureExternal: { enabled: boolean; tier: string; lastCallAt: string | null; lastClient: string | null; allowFullAccess: boolean } = fixturePreset() === 'mcp' ? { enabled: true, tier: 'operator', lastCallAt: new Date(Date.now() - 4 * 60_000).toISOString(), lastClient: 'claude-code', allowFullAccess: false } : { enabled: false, tier: 'chat', lastCallAt: null, lastClient: null, allowFullAccess: false }
type FixtureExternalClient = { id: string; name: string; installed: boolean; state: 'connected' | 'not_connected' | 'different'; configPath: string | null; addedByRetinue: boolean; error: string | null }
const fixtureExternalClients: FixtureExternalClient[] = [
  { id: 'claude-code', name: 'Claude Code', installed: true, state: fixturePreset() === 'mcp' ? 'connected' : 'not_connected', configPath: 'C:/Users/you/.claude.json', addedByRetinue: fixturePreset() === 'mcp', error: null },
  { id: 'codex', name: 'Codex', installed: true, state: 'not_connected', configPath: 'C:/Users/you/.codex/config.toml', addedByRetinue: false, error: null },
  { id: 'opencode', name: 'OpenCode', installed: true, state: 'not_connected', configPath: 'C:/Users/you/.config/opencode/opencode.json', addedByRetinue: false, error: null },
  { id: 'cursor', name: 'Cursor', installed: true, state: 'different', configPath: 'C:/Users/you/.cursor/mcp.json', addedByRetinue: false, error: null },
  { id: 'qwen', name: 'Qwen Code', installed: false, state: 'not_connected', configPath: null, addedByRetinue: false, error: null },
]
// The computer on the owner's server. `?preset=computer` is linked, awake and shared with employees;
// `computer-asleep`, `computer-away` (the server does not answer) and `computer-linked` (linked, not shared yet).
// `?flag=computer-no-input` is an awake computer missing the screen tools, for the setup card.
const computerPreset = (fixturePreset() ?? '').startsWith('computer') ? fixturePreset()! : null
const fixtureComputer: { linked: boolean; enabled: boolean; awake: boolean; away: boolean; since: string } = { linked: !!computerPreset, enabled: !!computerPreset && computerPreset !== 'computer-linked', awake: computerPreset !== 'computer-asleep', away: computerPreset === 'computer-away', since: new Date(Date.now() - 5 * 3_600_000).toISOString() }
const fixtureRoutines: Array<Record<string, unknown>> = [
  { id: 'rt_1', name: 'Morning price check', cron: '0 8 * * *', command: 'node ~/jobs/prices.mjs --notify', enabled: true, lastRunAt: new Date(Date.now() - 3 * 3_600_000).toISOString(), lastOk: true, lastOutput: 'Checked 42 listings.\n3 changed since yesterday.' },
  { id: 'rt_2', name: 'Nightly site backup', cron: '30 2 * * *', command: 'bash ~/jobs/backup.sh', enabled: true, lastRunAt: new Date(Date.now() - 9 * 3_600_000).toISOString(), lastOk: false, lastOutput: 'rsync: connection timed out (110)' },
  { id: 'rt_3', name: 'Refresh sign-in', cron: '*/30 * * * *', command: 'npx playwright test refresh.spec.ts', enabled: false, lastRunAt: null, lastOk: null, lastOutput: null },
]
const fixtureOutside = {
  enabled: fixturePreset() === 'outside',
  sessions: [
    { id: 'cli-1', provider: 'claude', folder: 'invoice-api', state: 'working', tool: 'Bash', startedAt: new Date(Date.now() - 600_000).toISOString(), updatedAt: new Date().toISOString() },
    { id: 'cli-2', provider: 'claude', folder: 'site', state: 'waiting', tool: null, startedAt: new Date(Date.now() - 1_800_000).toISOString(), updatedAt: new Date(Date.now() - 60_000).toISOString() },
  ],
}
// `?preset=schedule-retry`: one run failed after changing files, one ended quietly (wave 13).
if (fixturePreset() === 'schedule-retry') {
  const at = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString()
  const changed = { id: 'run-changed', automationId: 'auto-tidy', sessionId: null, status: 'failed', reason: 'error_after_changes', scheduledFor: at(30), startedAt: at(30), finishedAt: at(28) }
  const quiet = { id: 'run-quiet', automationId: 'auto-watch', sessionId: null, status: 'completed', reason: 'nothing_to_report', scheduledFor: at(60), startedAt: at(60), finishedAt: at(59) }
  const base = { agentId: 'agent-codex', sessionMode: 'new', approvalMode: 'auto', maxRuntimeSecs: 1800, enabled: true, createdBy: 'user', consecutiveFailures: 0, pauseReason: null, running: false, nextRunAt: new Date(Date.now() + 20 * 3_600_000).toISOString() }
  fixtureAutomations.push({ ...base, id: 'auto-tidy', name: 'Tidy the changelog', prompt: 'Tidy CHANGELOG.md.', schedule: { kind: 'cron', cron: '0 9 * * *', timezone: 'UTC' }, consecutiveFailures: 1, lastRunAt: changed.startedAt, lastRun: changed })
  fixtureAutomations.push({ ...base, id: 'auto-watch', name: 'Watch the build', prompt: 'Check the nightly build.', schedule: { kind: 'cron', cron: '0 * * * *', timezone: 'UTC' }, lastRunAt: quiet.startedAt, lastRun: quiet })
  fixtureRuns['auto-tidy'] = [changed]
  fixtureRuns['auto-watch'] = [quiet]
  // Wave 2: reported cost, a projection, a skip when nothing changed and a stop at the spend check-in.
  const tidy = fixtureAutomations.find((item) => item.id === 'auto-tidy')!
  Object.assign(tidy, { spend: { currency: null, projection: null, basis: 'no_priced_runs', sampledRuns: 0, monthRuns: 30 }, spendCheckUsd: null, skipUnchanged: false })
  const watch = fixtureAutomations.find((item) => item.id === 'auto-watch')!
  Object.assign(watch, { spend: { currency: 'USD', projection: '18.4', basis: 'recent_runs', sampledRuns: 6, monthRuns: 720 }, spendCheckUsd: '0.50', skipUnchanged: true })
  Object.assign(quiet, { cost: { amount: '0.0261', currency: 'USD', reported: 3, unreported: 0 } })
  fixtureRuns['auto-watch'].push(
    { id: 'run-same', automationId: 'auto-watch', sessionId: null, status: 'skipped', reason: 'unchanged', scheduledFor: at(120), startedAt: at(120), finishedAt: at(120), cost: { amount: null, currency: 'USD', reported: 0, unreported: 0 } },
    { id: 'run-spend', automationId: 'auto-watch', sessionId: null, status: 'cancelled', reason: 'spend_check', scheduledFor: at(180), startedAt: at(180), finishedAt: at(176), cost: { amount: '0.52', currency: 'USD', reported: 4, unreported: 0 } },
  )
}
/** Preview-only Manager knowledge (FIX36 D3): structured facts and a playbook. */
const fixtureManagerFacts: Array<Record<string, unknown>> = [
  { id: 'fact-name', category: 'identity', key: 'name', value: 'Sico Vibes', source: 'said', locked: true, createdAt: '2026-10-16T09:00:00Z', updatedAt: '2026-10-16T09:00:00Z' },
  { id: 'fact-role', category: 'identity', key: 'role', value: 'Founder and product owner.', source: 'said', locked: false, createdAt: '2026-10-16T09:01:00Z', updatedAt: '2026-10-16T09:01:00Z' },
  { id: 'fact-tone', category: 'style', key: 'tone', value: 'Direct, plain English. No filler or hype.', source: 'learned', locked: false, createdAt: '2026-10-16T09:02:00Z', updatedAt: '2026-10-16T09:02:00Z' },
]
const fixturePlaybookSeed = `# How I work

I am the user's Manager. I plan, keep their notes and playbook, and ask before
anything consequential.

## First run: get to know the user

Ask about their name and role, how they work, their communication style, coding
conventions, design taste and references. Save each answer with
\`remember_about_user\`. Never store secrets.

## Proposing a team

One employee per responsibility, two to six employees, each role explained in a sentence.
`
const fixturePlaybookVersions: Array<Record<string, unknown>> = [
  { version: 1, text: fixturePlaybookSeed, author: 'default', createdAt: '2026-10-16T08:00:00Z' },
]
const composerPreviewStorageKey = 'retinue.preview.composer-session.v1'
const PREVIEW_HINTS = ['companion.drag', 'approval.first', 'plan.first', 'context.high', 'team.first']

let agents: Agent[] = [
  { id: 'agent-claude', name: 'Claude', look: { shape: 'cloud', seed: 'claude' }, description: 'Default agent for Claude.', instructions: '', color: '#f38c6f', runtimeId: 'runtime-claude', model: null, thinking: null, serviceTier: null, customArgs: [], customEnv: {}, maxConcurrency: 1, defaultProject: null, sortOrder: 0, archived: false, createdAt: minutesAgo(4000), updatedAt: minutesAgo(4000) },
  { id: 'agent-codex', name: 'Codex', description: 'Default agent for Codex.', instructions: '', color: '#82aaff', runtimeId: 'runtime-codex', model: null, thinking: null, serviceTier: null, customArgs: [], customEnv: {}, maxConcurrency: 1, defaultProject: null, sortOrder: 0, archived: false, createdAt: minutesAgo(3000), updatedAt: minutesAgo(3000), approvalMode: 'auto', effectiveApprovalMode: 'auto' },
  { id: 'agent-invoice', name: 'Invoice helper', look: { shape: 'capsule', seed: 'invoice' }, description: '', instructions: '', color: 'lemon', runtimeId: 'runtime-codex', model: null, thinking: null, serviceTier: null, customArgs: [], customEnv: {}, maxConcurrency: 1, defaultProject: null, sortOrder: 1, archived: false, createdAt: minutesAgo(2000), updatedAt: minutesAgo(2000) },
  { id: 'agent-opencode', name: 'OpenCode', look: { shape: 'droplet', seed: 'opencode' }, description: 'Default agent for OpenCode.', instructions: '', color: 'violet', runtimeId: 'runtime-opencode', model: null, thinking: null, serviceTier: null, customArgs: [], customEnv: {}, maxConcurrency: 1, defaultProject: null, sortOrder: 0, archived: false, createdAt: minutesAgo(1000), updatedAt: minutesAgo(1000) },
  { id: 'agent-old', name: 'Retired', description: '', instructions: '', color: 'pink', runtimeId: 'runtime-codex', model: null, thinking: null, serviceTier: null, customArgs: [], customEnv: {}, maxConcurrency: 1, defaultProject: null, sortOrder: 2, archived: true, createdAt: minutesAgo(5000), updatedAt: minutesAgo(10) },
]

let sessions: Session[] = [
  {
    id: 'session-codex', runtimeId: 'runtime-codex', agentId: 'agent-codex', title: 'Invoice parser tests', projectPath: 'C:/work/korus', ...(fixturePreset() === 'moved' ? { link: { movedTo: { machineId: 'preview-laptop', name: 'Work laptop', at: new Date().toISOString() } } } : {}), ...(fixturePreset() === 'imported' ? { imported: { source: 'codex', conversationId: 'rollout-2026-11-13T11-00-00-0190aaaa-bbbb-cccc-dddd-eeeeffff0000', canResumeWith: null, readOnly: true } } : {}), state: flags().has('idle') ? 'idle' : 'working', model: 'gpt-6.1-sol', updatedAt: minutesAgo(fixturePreset() === 'generated-image' ? 0 : 2),
    messages: [
      { id: 'm1', role: 'user', content: 'Can you make the invoice parser tests pass?', createdAt: minutesAgo(9), ...(fixturePreset() === 'mcp' ? { meta: { origin: { kind: 'mcp', client: 'claude-code' } } } : {}) },
      ...(flags().has('demo-image') ? [{ id: 'm-image', role: 'user', content: 'Here is the invoice screenshot.', createdAt: minutesAgo(8), attachments: [{ name: 'invoice.png' }] }] : []),
      { id: 'm2', role: 'assistant', turnId: 'turn-codex-1', content: 'Sure. I found two failing cases in **tests/invoice.test.ts**: the currency field is parsed before the locale is known, and totals use floats.\n\nI will switch totals to integer minor units and parse the locale first.', createdAt: minutesAgo(8) },
      ...(fixturePreset() === 'openui' ? [{ id: 'openui-1', role: 'assistant', content: "Here is the test run at a glance.\n\n```openui\nroot = Stack([title, stats, tabs, card])\ntitle = Heading(\"Test run\", 1)\nstats = Row([Stat(\"Passed\", 1687, \"+12\", \"good\"), Stat(\"Failed\", 2, \"-1\", \"good\"), Stat(\"Duration\", \"4m 12s\"), Badge(\"main\", \"info\")])\ntabs = Tabs([Tab(\"By suite\", [chart]), Tab(\"Failures\", [table])])\nchart = Chart(\"bar\", [\"ui\", \"daemon\", \"storage\"], [{name: \"tests\", values: [1687, 966, 160]}], \"Tests by suite\")\ntable = Table([\"Test\", \"Suite\", \"Reason\"], [[\"composer sends\", \"ui\", \"timeout\"], [\"warm resume\", \"daemon\", \"assertion\"]])\ncard = Card(\"Next step\", [Text(\"Two tests fail on Windows only.\"), Progress(\"Release checklist\", 70), form])\nform = Form(\"next\", [Select(\"who\", \"Who fixes them\", [\"Ada\", \"Sleuth\"]), Checkbox(\"release\", \"Release after\")], \"Send to the team\")\n```", createdAt: new Date().toISOString(), sequence: 101 }] : []),
      ...(fixturePreset() === 'visual' ? [{ id: 'visual-1', role: 'assistant', content: "Here is a mock of the card.\n\n```retinue-visual\n<div style=\"padding:16px;border-radius:12px;background:#eef\"><h3 style=\"margin:0\">Invoice 104</h3><p>Paid on 31 October</p><script>document.body.innerHTML=\"SCRIPT RAN\"</script></div>\n```\n\nAnd as SVG:\n\n```svg\n<svg width=\"120\" height=\"60\" xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"120\" height=\"60\" rx=\"10\" fill=\"#7a7\"/></svg>\n```", createdAt: new Date().toISOString(), sequence: 100 }] : []),
      ...(fixturePreset() === 'generated-image' ? [
        { id: 'img-failed', role: 'notice', content: "The image couldn't be generated: The image didn't arrive.", createdAt: minutesAgo(8), meta: { kind: 'generated_image', mediaId: 'img-failed', status: 'failed', detail: "The image didn't arrive." } },
        { id: 'img-ready', role: 'notice', content: 'Generated an image: sunrise.png', createdAt: minutesAgo(7), meta: { kind: 'generated_image', mediaId: 'img-ready', status: 'ready', name: 'sunrise.png', bytes: 182_400, mediaType: 'image/png', provider: 'codex' } },
        { id: 'img-pending', role: 'notice', content: 'Generating an image…', createdAt: minutesAgo(6), meta: { kind: 'generated_image', mediaId: 'img-pending', status: 'pending' } },
      ] : []),
      { id: 'm3', role: 'assistant', turnId: 'turn-codex-2', content: fixturePreset() === 'message-links'
        ? 'Here are the projects I found:\n\n| Project | Stars | Licence | What it gives you |\n| --- | ---: | --- | --- |\n| [dicebear/dicebear](https://github.com/dicebear/dicebear) | 9.9k | MIT | 61 styles for avatars and characters |\n| [Alain00/blobatar](https://github.com/Alain00/blobatar) | 1.5k | MIT | Parametric blob illustrations |\n\nMore details: https://github.com/hardbeat920/monocode.'
        : fixturePreset() === 'document' ? 'The research is done. Report: [docs/research/n8n-nod/05-messaging.md](docs/research/n8n-nod/05-messaging.md)'
        : 'Patched `parseInvoice` and the fixture loader. Running the suite now.', createdAt: minutesAgo(3) },
    ],
    turns: [
      { id: 'turn-codex-1', state: 'completed', createdAt: minutesAgo(9), completedAt: minutesAgo(8) },
      // A half-done task list: two steps finished, one in progress, one waiting.
      { id: 'turn-codex-2', state: 'working', createdAt: minutesAgo(3), plan: { items: [
        { text: 'Read tests/invoice.test.ts and the parser', status: 'completed' },
        { text: 'Switch totals to integer minor units', status: 'completed' },
        { text: 'Parse the locale before the currency', status: 'in_progress' },
        { text: 'Run the invoice suite', status: 'pending' },
      ], updatedAt: minutesAgo(1) } },
    ],
    tools: [
      { id: 't1', title: 'npm test', kind: 'command', state: 'running', command: 'npm test -- invoice', sequence: 4 },
      // The to-do tool's own row: kind `plan`, so the window shows the list instead.
      { id: 't-plan', turnId: 'turn-codex-2', title: 'Updated the task list', kind: 'plan', state: 'completed', sequence: 5 },
      // `?subbots=1` also shows a provider's own sub-agent, with its steps under it.
      ...(flags().get('subbots') ? [
        { id: 'agent-1', title: 'Agent', kind: 'Agent', state: 'completed', sequence: 4, detail: { tool: 'Agent', subagent: { type: 'general-purpose', model: 'haiku', description: 'Count the invoice fixtures' }, output: 'Async agent launched successfully. (This tool result is internal metadata)\nagentId: a1 (internal ID)\nThe agent is working in the background.\nThere are 12 invoice fixtures.' } },
        { id: 'agent-1-read', title: 'Read', kind: 'read', state: 'completed', parentToolCallId: 'agent-1', sequence: 4, detail: { path: 'tests/fixtures/invoices.json' } },
        { id: 'agent-1-grep', title: 'Search', kind: 'search', state: 'completed', parentToolCallId: 'agent-1', sequence: 4, detail: { query: 'invoice', output: '12 matches' } },
      ] : []),
    ],
  },
  {
    id: composerPreviewSessionId, runtimeId: 'runtime-codex', agentId: 'agent-codex',
    title: 'Preview: composer controls', projectPath: 'C:/work/korus', state: 'idle',
    modelLock: { model: 'gpt-6.1-sol', thinking: flags().has('composer-low') ? 'low' : 'high' }, contextUsed: 14_000, contextSize: 1_000_000,
    updatedAt: minutesAgo(4), previewOnly: true,
    messages: [
      { id: 'composer-preview-user-1', role: 'user', content: 'Check the checkout flow and keep the controls in the composer.', createdAt: minutesAgo(9) },
      { id: 'composer-preview-assistant-1', role: 'assistant', content: 'I reviewed the checkout steps. The selected model and effort are locked to this preview conversation.', createdAt: minutesAgo(8) },
      { id: 'composer-preview-user-2', role: 'user', content: 'Can you make the confirmation state clearer?', createdAt: minutesAgo(7) },
      { id: 'composer-preview-assistant-2', role: 'assistant', content: 'The confirmation summary now shows the selected delivery option before submission.', createdAt: minutesAgo(6) },
      { id: 'composer-preview-user-3', role: 'user', content: 'Check the selected model, effort and context meter.', createdAt: minutesAgo(5) },
    ],
  },
  // `?preset=usage-limit`: Claude reached its usage limit, so the conversation offers a handoff (wave 13).
  { id: 'session-claude', runtimeId: 'runtime-claude', agentId: 'agent-claude', title: 'Landing page copy', projectPath: 'C:/work/site', state: fixturePreset() === 'usage-limit' ? 'limited' : 'completed', ...(fixturePreset() === 'usage-limit' ? { provider: 'claude', limitedUntil: new Date(Date.now() + 2 * 3_600_000).toISOString() } : {}), model: 'claude-opus', updatedAt: minutesAgo(41), messages: [{ id: 'c1', role: 'assistant', content: 'Updated the hero copy and the pricing table.', createdAt: minutesAgo(41) },
    ...(fixturePreset() === 'usage-limit' ? [{ id: 'c-limit', role: 'notice', content: "Limited until 14:00. I'll pick up when the window opens.", createdAt: minutesAgo(40), meta: { kind: 'usage_limit' } }] : [])] },
  { id: 'session-opencode', runtimeId: 'runtime-opencode', agentId: 'agent-opencode', title: 'Refactor auth', projectPath: 'C:/work/api', state: 'idle', updatedAt: minutesAgo(60 * 26), messages: [], origin: { kind: 'mcp', client: 'claude-code' } },
  { id: 'session-legacy', runtimeId: 'runtime-codex', agentId: null, title: 'Untied notes', projectPath: 'C:/work/notes', state: 'idle', updatedAt: minutesAgo(90), messages: [] },
  { id: 'session-claude-2', runtimeId: 'runtime-claude', agentId: 'agent-claude', title: 'Hero follow-up', projectPath: 'c:\\work\\site\\', state: 'idle', updatedAt: minutesAgo(20), modelLock: flags().get('preset') === 'claude-context' ? { model:'claude-sonnet-5-5', thinking:'high' } : undefined, contextUsed: flags().get('preset') === 'claude-context' ? 17_340 : null, contextSize: flags().get('preset') === 'claude-context' ? 1_000_000 : null, contextModel: flags().get('preset') === 'claude-context' ? 'claude-sonnet-5-5' : null, contextEstimated: flags().get('preset') === 'claude-context', messages: [] },
  { id: 'session-claude-web', runtimeId: 'runtime-claude', agentId: 'agent-claude', title: 'Pricing page', projectPath: 'C:\\work\\web', state: fixturePreset() === 'failure' ? 'error' : 'completed', updatedAt: minutesAgo(80), messages: [] },
  { id: 'session-claude-dotdot', runtimeId: 'runtime-claude', agentId: 'agent-claude', title: 'Dotdot path', projectPath: 'C:\\work\\site\\..\\site', state: 'idle', updatedAt: minutesAgo(120), messages: [] },
  { id: 'session-claude-dot', runtimeId: 'runtime-claude', agentId: 'agent-claude', title: 'Dot path', projectPath: 'C:\\work\\site\\.', state: 'idle', updatedAt: minutesAgo(130), messages: [] },
  { id: 'session-invoice-a', runtimeId: 'runtime-codex', agentId: 'agent-invoice', title: 'North site', projectPath: 'C:\\client\\site', state: 'idle', updatedAt: minutesAgo(50), messages: [] },
  { id: 'session-invoice-b', runtimeId: 'runtime-codex', agentId: 'agent-invoice', title: 'South site', projectPath: 'D:\\other\\site', state: 'working', updatedAt: minutesAgo(40), messages: [] },
  { id: 'session-retired', runtimeId: 'runtime-codex', agentId: 'agent-old', title: 'Archived chat', projectPath: 'C:\\old\\repo', state: 'idle', updatedAt: minutesAgo(15), messages: [] },
  ...Array.from({ length: 31 }, (_, index) => {
    const n = index + 1
    return { id: `session-codex-many-${n}`, runtimeId: 'runtime-codex', agentId: 'agent-codex', title: `Bulk ${n}`, projectPath: 'C:\\work\\korus', state: 'idle' as const, updatedAt: minutesAgo(32 - n), messages: [] }
  }),
]
// `?settled`: three more of Codex's conversations, idle for 9, 12 and 30 days. They file under the Settled shelf (24 November 2026).
if (flags().has('settled')) {
  const daysAgo = (days: number) => new Date(Date.now() - days * 86_400_000).toISOString()
  for (const [id, title, days] of [['session-old-1', 'Move invoices to the new schema', 9], ['session-old-2', 'Tidy the settings copy', 12], ['session-old-3', 'First look at the export bug', 30]] as const) {
    sessions.push({ id, runtimeId: 'runtime-codex', agentId: 'agent-codex', title, projectPath: 'C:/work/korus', state: 'idle', createdAt: daysAgo(days + 1), updatedAt: daysAgo(days), lastMessageAt: daysAgo(days), messages: [{ id: `${id}-m1`, role: 'user', content: title, createdAt: daysAgo(days) }, { id: `${id}-m2`, role: 'assistant', content: 'Done.', createdAt: daysAgo(days) }] } as unknown as Session)
  }
}

try {
  const saved = localStorage.getItem(composerPreviewStorageKey)
  if (saved) {
    const state = JSON.parse(saved) as { modelLock?:Session['modelLock']; contextUsed?:number|null; contextSize?:number|null }
    const session = sessions.find((item) => item.id === composerPreviewSessionId)
    if (session && state.modelLock && typeof state.modelLock === 'object') {
      session.modelLock = state.modelLock
      session.model = state.modelLock.model ?? undefined
      session.contextUsed = typeof state.contextUsed === 'number' ? state.contextUsed : null
      session.contextSize = typeof state.contextSize === 'number' ? state.contextSize : null
    }
  }
} catch { /* Preview fixture storage is optional. */ }

// Team fixture (preview only): projects own blobs; Codex leads, Pololo is a teammate.
let projects: Project[] = [
  { id: 'project-board', name: 'Board', path: 'C:/work/korus', sortOrder: 0, collapsed: false },
  { id: 'project-side', name: 'Side Projects', path: 'C:/work/site', sortOrder: 1, collapsed: false },
]
if (flags().get('demo-shared') === 'missing' && projects[0]) {
  const project = projects[0]
  projects[0] = { ...project, overrides: { ...project.overrides, setupScript: 'npm install' } }
}

const teamFields: Record<string, Partial<Agent>> = {
  'agent-codex': { projectId: 'project-board', role: 'CTO', leader: true },
  'agent-claude': { projectId: 'project-side', role: 'Copywriter' },
  'agent-invoice': { projectId: 'project-board', role: 'Finance' },
  'agent-opencode': { role: 'Researcher' },
}
agents = agents.map((agent) => ({ projectId: null, role: '', leader: false, hidden: false, ...agent, ...teamFields[agent.id] }))
agents.push({ id: 'agent-pololo', name: 'Pololo', look: { shape: 'cloud', seed: 'pololo' }, description: 'UI/UX designer: finds and adapts component references.', instructions: '', color: '#2fbf9b', runtimeId: 'runtime-opencode', model: null, thinking: null, serviceTier: null, customArgs: [], customEnv: {}, maxConcurrency: 1, defaultProject: null, sortOrder: 2, archived: false, createdAt: minutesAgo(900), updatedAt: minutesAgo(900), projectId: 'project-board', role: 'UI/UX Designer', leader: false, hidden: false })
// FIX36: the one built-in Manager previews as an ordinary bot that carries
// systemRole "manager" and works across every project (projectId null).
const managerDescription = 'Runs your team: knows every project and employee, plans the work and asks before it acts.'
const managerInstructions = "You are the Manager. You work across all of the user's projects and know every employee on the team. Ask the user before consequential actions: creating a project, creating an employee, assigning a task, or changing your own playbook."
agents.push({ id: 'agent-manager', name: 'Manager', look: { shape: 'hexagon', seed: 'manager' }, description: managerDescription, instructions: managerInstructions, color: 'mint', runtimeId: 'runtime-opencode', model: null, thinking: null, serviceTier: null, customArgs: [], customEnv: {}, maxConcurrency: 1, defaultProject: null, sortOrder: 0, archived: false, createdAt: minutesAgo(5000), updatedAt: minutesAgo(2), projectId: null, role: 'Manager', leader: false, hidden: false, approvalMode: 'ask', effectiveApprovalMode: 'ask', systemRole: 'manager' })
const teamMainId = 'session-team-leader'
const teamSideId = 'session-team-side'
sessions.push({
  id: teamMainId, runtimeId: 'runtime-codex', agentId: 'agent-codex', title: 'Codex', projectPath: 'C:/work/korus', state: flags().has('idle') ? 'idle' : 'waiting_permission', updatedAt: minutesAgo(0.5),
  modelLock: { model: 'gpt-6.1-sol', thinking: 'high', planMode: flags().has('plan') }, contextUsed: 52_000, contextSize: 400_000,
  messages: [
    { id: 'team-u1', sequence: 1, turnId: 'team-tA', role: 'user', content: 'Can you research an effort picker like the one on 21st.dev? @Pololo is great at UI, ask him to find it.', createdAt: minutesAgo(30) },
    { id: 'team-a1', sequence: 2, turnId: 'team-tA', role: 'assistant', content: 'On it — sending Pololo the brief and asking him to hunt down that effort / reasoning-level picker.', createdAt: minutesAgo(29) },
    { id: 'team-n1', sequence: 3, role: 'notice', content: 'Find an open-source effort picker like the 21st.dev one…', createdAt: minutesAgo(29), meta: { kind: 'delegation', direction: 'sent', peerAgentId: 'agent-pololo', peerName: 'Pololo', topic: 'Effort picker research', sideSessionId: teamSideId } },
    { id: 'team-r1', sequence: 4, turnId: 'team-tB', role: 'user', content: '[Reply from Pololo]\nNothing pixel-exact exists as a package. Closest: a PostHog ReasoningLevelDropdown (MIT) and a model selector on 21st.dev with Effort, Context and Fast toggles.', createdAt: minutesAgo(12), meta: { kind: 'blob_reply', fromAgentId: 'agent-pololo', fromName: 'Pololo', sideSessionId: teamSideId } },
    { id: 'team-a2', sequence: 5, turnId: 'team-tB', role: 'assistant', content: 'Pololo found it — nothing is a pixel-exact drop-in, but these are the useful hits:\n\n- **PostHog ReasoningLevelDropdown** (MIT): Faster ↔ Smarter, notched snaps, a Default badge.\n- **AI Model Select** on 21st.dev: Effort, Context and Fast in one popover.\n\nMy take: fork the PostHog one. Want me to plan the port?', createdAt: minutesAgo(11) },
    { id: 'team-u2', sequence: 6, turnId: 'team-tC', role: 'user', content: 'Yes, plan it.', createdAt: minutesAgo(3) },
    { id: 'team-p1', sequence: 7, turnId: 'team-tC', role: 'plan', content: '1. Vendor the ReasoningLevelDropdown into `src/ui/effort`.\n2. Replace its token colours with Retinue theme tokens.\n3. Wire it to the session lock and add keyboard tests.', createdAt: minutesAgo(2), meta: { kind: 'plan' } },
  ],
  tools: [
    { id: 'team-tool-1', turnId: 'team-tA', kind: 'mcpToolCall', title: 'retinue · list_blobs', state: 'completed', startedAt: minutesAgo(29.5), detail: { tool: 'retinue · list_blobs', result: 'Your team:\n- Codex (you) [leader] — CTO\n- Pololo — UI/UX Designer · available' } },
    { id: 'team-tool-2', turnId: 'team-tA', kind: 'mcpToolCall', title: 'retinue · message_blob', state: 'completed', startedAt: minutesAgo(29.2), detail: { tool: 'retinue · message_blob', input: '{\n  "blob": "Pololo",\n  "message": "Find an open-source effort picker like the 21st.dev one…"\n}', result: 'Sent to Pololo. Their reply will arrive here as a message starting with "[Reply from Pololo]".' } },
    { id: 'team-tool-3', turnId: 'team-tB', kind: 'commandExecution', title: 'rg -n "ReasoningLevel" node_modules/@posthog', state: 'completed', startedAt: minutesAgo(11), detail: { command: 'rg -n "ReasoningLevel" node_modules/@posthog', output: 'node_modules/@posthog/ui/ReasoningLevelDropdown.tsx:12:export function ReasoningLevelDropdown(', exitCode: 0 } },
    { id: 'team-tool-4', turnId: 'team-tB', kind: 'webSearch', title: 'Searched: 21st.dev model select effort', state: 'completed', startedAt: minutesAgo(10.5), detail: { query: '21st.dev model select effort' } },
  ],
  files: [
    { id: 'team-file-1', turnId: 'team-tB', path: 'docs/effort-picker.md', createdAt: minutesAgo(10), detail: { path: 'docs/effort-picker.md', change: 'add', diff: '+# Effort picker options\n+- PostHog ReasoningLevelDropdown (MIT)\n+- 21st.dev AI Model Select' } },
  ],
})
sessions.push({
  id: teamSideId, runtimeId: 'runtime-opencode', agentId: 'agent-pololo', title: 'Effort picker research', projectPath: 'C:/work/korus', state: 'completed', updatedAt: minutesAgo(12),
  link: { kind: 'side', peerAgentId: 'agent-codex', peerName: 'Codex', originSessionId: teamMainId, topic: 'Effort picker research' },
  messages: [
    { id: 'side-m1', role: 'user', content: 'Hi Pololo, it\'s Codex. Sam needs a UI research hunt: an Effort / reasoning-level picker with a Faster ↔ Smarter slider, about six snap points and a Recommended mark. Search 21st.dev and GitHub for open-source replicas and reply with the best hits ranked.', createdAt: minutesAgo(29), meta: { kind: 'blob_message', fromAgentId: 'agent-codex', fromName: 'Codex' } },
    { id: 'side-t1', role: 'thinking', content: 'The user (Codex, a blob) wants a UI hunt. Search first, answer concisely.', createdAt: minutesAgo(12.5) },
    { id: 'side-a1', role: 'assistant', content: 'UI research done.\n\n**Bottom line:** nothing pixel-exact as a drop-in. Closest fork is PostHog; closest install is an AI Model Select on 21st.dev.', createdAt: minutesAgo(12) },
  ],
})
sessions.push({
  id: 'session-team-side-2', runtimeId: 'runtime-codex', agentId: 'agent-codex', title: 'Which model are you on', projectPath: 'C:/work/korus', state: 'completed', updatedAt: minutesAgo(95),
  link: { kind: 'side', peerAgentId: 'agent-pololo', peerName: 'Pololo', originSessionId: 'session-pololo', topic: 'Which model are you on' },
  messages: [
    { id: 'side2-m1', sequence: 1, role: 'user', content: 'Quick one from Pololo: which model and effort are you running?', createdAt: minutesAgo(96), meta: { kind: 'blob_message', fromAgentId: 'agent-pololo', fromName: 'Pololo' } },
    { id: 'side2-a1', sequence: 2, role: 'assistant', content: 'GPT-5.5 at High effort.', createdAt: minutesAgo(95) },
  ],
})

// `?demo` (website screenshots): the placeholder employees become real hires with profile photos,
// and every message, title and delegation uses their names.
if (flags().has('demo')) {
  const cast: Record<string, { name: string; role: string; photo: `photo-${string}`; was: string }> = {
    'agent-codex': { name: 'Ada', role: 'Backend engineer', photo: 'photo-17', was: 'Codex' },
    'agent-claude': { name: 'Mira', role: 'Data scientist', photo: 'photo-21', was: 'Claude' },
    'agent-opencode': { name: 'Grey', role: 'Research and planning', photo: 'photo-27', was: 'OpenCode' },
    'agent-invoice': { name: 'Wu', role: 'Full-stack engineer', photo: 'photo-05', was: 'Invoice helper' },
    'agent-pololo': { name: 'Fifi', role: 'UI designer', photo: 'photo-24', was: 'Pololo' },
    'agent-manager': { name: 'CEO', role: 'Runs the team', photo: 'photo-43', was: 'Manager' },
  }
  agents = agents.map((agent) => {
    const role = cast[agent.id]
    return role ? { ...agent, name: role.name, role: role.role, look: { ...(agent.look ?? { shape: 'circle', seed: agent.id }), photo: role.photo } } : agent
  })
  const rename = (text: string) => Object.values(cast).reduce((value, role) => value.split(role.was).join(role.name), text)
  const swap = (value: unknown): unknown => typeof value === 'string' ? rename(value) : Array.isArray(value) ? value.map(swap) : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([key, inner]) => [key, key === 'id' || key.endsWith('Id') ? inner : swap(inner)])) : value
  sessions = sessions.map((session) => swap(session) as Session)
}
// `?scene=<id>`: the inspector's conversations, one per state the App can show.
const fixtureScene = sceneFor(flags().get('scene'))
let sceneQueueSeeded = false
if (fixtureScene) {
  setPreviewSelection('agent-codex')
  sessions.push({ id: SCENE_SESSION_ID, runtimeId: 'runtime-codex', agentId: 'agent-codex', title: fixtureScene.title ?? 'Preview', projectPath: 'C:/work/korus', state: fixtureScene.state ?? 'idle', model: 'gpt-6.1-sol', updatedAt: new Date().toISOString(), messages: fixtureScene.messages ?? [], tools: fixtureScene.tools ?? [], files: fixtureScene.files ?? [], ...fixtureScene.session } as Session)
  for (const extra of fixtureScene.extraSessions ?? []) sessions.push(extra as unknown as Session)
}
// 22 November 2026: an employee whose coding agent Retinue retired (Gemini CLI). The daemon keeps its saved runtime
// id, which no longer appears in the roster.
if (flags().has('demo-runtime-gone')) {
  const agent = agents.find((item) => item.id === 'agent-codex')
  if (agent) agent.runtimeId = 'rt_gemini_retired'
}
if (flags().has('demo-troubleshoot')) {
  const session = sessions.find((item) => item.id === 'session-codex')
  if (session) {
    session.state = 'error'
    session.turns = [{ id: 'preview-failed-turn', state: 'error', failureClass: 'process_exited', createdAt: minutesAgo(3), endedAt: minutesAgo(2) }]
  }
}
if (flags().has('demo-first-run') || flags().has('empty')) {
  agents = []
  sessions = []
}
// `?chatbg=dither`: a generated chat background, so the inspector can show each effect.
if (flags().get('chatbg')) void import('../ui/chatBackgroundStore').then((module) => module.seedChatBackgroundForPreview(flags().get('chatbg') as never))
else if (flags().has('clean')) void import('../ui/chatBackgroundStore').then((module) => module.updateChatBackgroundPrefs({ hasImage: false }))
// `?demo-no-name`: the owner has not set a name yet, so the profile plate asks for one.
if (flags().has('demo-no-name')) { try { localStorage.removeItem('retinue.profile.name') } catch { /* Preview storage is optional. */ } }
// `?clean`: no first-run hints, tour or What's New over the screen being inspected.
if (flags().has('clean')) {
  try {
    localStorage.removeItem('retinue.whatsNew.seen')
    localStorage.setItem('retinue.tour', JSON.stringify({ version: 1, done: true, step: 0 }))
    localStorage.setItem('retinue.firstRun.complete', '1')
    localStorage.removeItem('retinue.drafts.v1')
    localStorage.setItem('retinue.inspector.open', '0')
  } catch { /* Preview storage is optional. */ }
}
const teamQuestion: PermissionRequest = { id: 'perm-question', sessionId: teamMainId, runtimeId: 'runtime-codex', status: 'pending', kind: 'question', title: 'Codex has a question', choices: ['answer', 'dismiss'], questions: [{ id: 'q-direction', header: 'Direction', question: 'Which direction should the effort picker follow?', options: [{ label: 'PostHog fork', description: 'Closest behaviour, MIT licensed' }, { label: '21st.dev model select', description: 'One popover for effort, context and fast' }, { label: 'Build our own' }], multiSelect: false, allowOther: true }] }
// FIX36 D2: a Manager proposal surfaces as a `kind: "proposal"` permission card.
let fixtureProposalPending = true
const teamProposal: PermissionRequest = { id: 'perm-proposal', sessionId: teamMainId, runtimeId: 'runtime-opencode', status: 'pending', kind: 'proposal', title: 'Create the employee "Scout"', detail: 'Research · opencode', choices: ['approve', 'reject'], proposal: { id: 'perm-proposal', kind: 'bot', payload: { name: 'Scout', role: 'Research', provider: 'opencode' } } }

let sequence = 1
let agentSerial = 6
let fixtureAutostart = false
export const fixtureNotifications: Array<{ title: string; body: string }> = []
export const launchFixtureModes = ['in-progress', 'done', 'done-warning', 'service-down', 'onboarding-look', 'onboarding-step-1', 'onboarding-step-2', 'onboarding-step-3'] as const
export function launchFixtureData(mode: typeof launchFixtureModes[number]) {
  const warning = mode === 'done-warning'
  const progress = mode === 'in-progress'
  const checks = [
    { id: 'service', label: 'Connecting to the Retinue service', state: mode === 'service-down' ? 'failed' : progress ? 'ok' : 'ok', detail: mode === 'service-down' ? 'Service did not respond' : 'Connected and loaded the latest state' },
    { id: 'discovery', label: 'Finding coding agents', state: progress ? 'running' : 'ok', detail: progress ? 'In progress' : '3 coding agents found' },
    { id: 'auth:runtime-claude', label: 'Checking Claude Code sign-in', state: progress ? 'running' : 'ok', detail: progress ? 'In progress' : 'Signed in' },
    { id: 'models:runtime-codex', label: 'Loading Codex models', state: progress ? 'running' : 'ok', detail: progress ? 'In progress' : 'Loaded 12 models' },
    ...(warning ? [{ id: 'auth:runtime-opencode', label: 'Checking OpenCode sign-in', state: 'warning', detail: 'Warning: OpenCode: sign-in could not be confirmed' }, { id: 'models:runtime-opencode', label: 'Loading OpenCode models', state: 'warning', detail: 'Warning: OpenCode: models could not be loaded' }] : []),
    { id: 'usage', label: 'Loading usage and limits', state: progress ? 'running' : 'ok', detail: progress ? 'In progress' : 'Usage and limits are ready' },
  ]
  const onboarding = mode.startsWith('onboarding-step-')
  const runtimes = (onboarding && mode === 'onboarding-step-1' ? [] : baseRuntimes).map((runtime) => ({ ...runtime, authState: runtime.provider === 'codex' ? 'authenticated' : runtime.authState }))
  return { checks, slow: false, serviceDown: mode === 'service-down', runtimes }
}
export async function autostartEnabled() { return fixtureAutostart }
export async function setAutostartEnabled(enabled: boolean) { fixtureAutostart = enabled }
export async function sendDesktopNotification(title: string, body: string) { fixtureNotifications.push({ title, body }) }
export async function flashMainWindow() {}
function rpcError(code: string, message: string) {
  return new Error(`${code}: ${message}`)
}

function sortAgents(list: Agent[]) {
  return [...list].sort((a, b) => a.runtimeId.localeCompare(b.runtimeId) || a.sortOrder - b.sortOrder || a.createdAt.localeCompare(b.createdAt) || a.id.localeCompare(b.id))
}

const clearedSignIns = new Set<string>()

function currentSnapshot(): Snapshot {
  const search = flags()
  const empty = search.has('empty') || search.has('demo-first-run')
  const runtimes = baseRuntimes.map((runtime) => {
    if (search.has('demo-auth') && runtime.id === 'runtime-codex' && !clearedSignIns.has(runtime.id)) return { ...runtime, status: 'auth_required', authState: 'signed_out' }
    if (fixturePreset() === 'opencode-runtime-error' && runtime.id === 'runtime-opencode') return { ...runtime, status: 'error', probeStep: 'Protocol check', probeError: 'The CLI does not advertise the required protocol.' }
    return search.has('offline') && runtime.id === 'runtime-codex' ? { ...runtime, status: 'offline' } : { ...runtime }
  })
  return {
    sequence,
    runtimes,
    agents: (search.get('subbots') ? [...agents, { ...agents[0], id: 'sub-1', name: 'Pricing research', hidden: true, parentAgentId: 'agent-codex' } as Agent] : agents).map((agent) => ({ ...agent, customArgs: [...agent.customArgs], customEnv: { ...agent.customEnv } })),
    sessions: sessions.filter((session) => !session.archived).map((session) => ({ ...session })),
    permissions: [...livePermissions.values(), ...((empty || demoLive) ? [] : fixturePreset() === 'message-links'
      ? []
      : search.has('approval')
      ? [{ id: 'perm-1', sessionId: 'session-codex', runtimeId: 'runtime-codex', status: 'pending', title: 'Run shell command', command: 'npm test -- invoice', choices: ['allow_once', 'allow_session', 'deny'] }]
      : fixtureScene
      ? (fixtureScene.permissions ?? []).map((permission) => ({ sessionId: SCENE_SESSION_ID, runtimeId: 'runtime-codex', status: 'pending', ...permission } as unknown as PermissionRequest))
      // A pending approval pulls the window to its conversation, so a request for another conversation clears them.
      : search.get('session') && search.get('session') !== teamMainId ? []
      : [...(fixtureProposalPending ? [teamProposal] : []), ...(sessions.some((session) => session.id === teamMainId && session.state === 'waiting_permission') ? [teamQuestion] : [])])]
      .filter((permission) => !answeredPermissions.has(permission.id)),
    projects: projects.map((project) => ({ ...project })),
    usageSummary: { inputTokens: 182_400, outputTokens: 24_900 },
    quotas: fixtureQuotas(),
  }
}

const hoursFromNow = (hours: number) => new Date(Date.now() + hours * 3_600_000).toISOString()
function fixtureQuotas() {
  if (flags().has('no-quota')) return []
  const window = (usedPercent: number, hours: number, windowDurationMins: number | null) => ({ usedPercent, remainingPercent: 100 - usedPercent, resetsAt: hoursFromNow(hours), windowDurationMins })
  const presetUsage = fixturePreset() === 'usage-tip-90'
  const orb = flags().get('orb')
  const fetchedAt = minutesAgo(1)
  return [
    { provider: 'claude', runtimeId: 'runtime-claude', fetchedAt, lastAttemptAt: fetchedAt, failureCount: 0, lastError: null, snapshot: { provider: 'claude', limits: [
      { label: 'Claude Code', primary: window(1, 3.9, 300), secondary: orb === 'unknown' ? null : window(orb === 'zero' ? 0 : orb === 'full' ? 100 : 27, 52, 10_080) },
      { label: 'Claude Opus', primary: window(12, 52, 10_080), secondary: null },
    ] } },
    { provider: 'codex', runtimeId: 'runtime-codex', fetchedAt, lastAttemptAt: fetchedAt, failureCount: 0, lastError: null, snapshot: { provider: 'codex', limits: [
      { label: 'Codex', primary: window(0, 4.98, 300), secondary: orb === 'unknown' ? null : window(orb === 'zero' ? 0 : orb === 'full' ? 100 : presetUsage ? 90 : 44, presetUsage ? 18 : 107, 10_080) },
    ] } },
    { provider: 'opencode', runtimeId: 'runtime-opencode', fetchedAt, lastAttemptAt: fetchedAt, failureCount: 0, lastError: flags().has('quota-error') ? 'unauthorized' : null, snapshot: { provider: 'opencode', limits: [
      { label: '5-hour', primary: window(0, 2.78, 300), secondary: null },
      { label: 'Weekly', primary: orb === 'unknown' ? null : window(orb === 'zero' ? 0 : orb === 'full' ? 100 : 4, 48, 10_080), secondary: null },
      { label: 'Monthly', primary: window(16, 493, null), secondary: null },
    ] } },
  ]
}

function nameTaken(name: string, exceptId?: string) {
  const key = name.trim().toLocaleLowerCase('en')
  return agents.some((agent) => !agent.archived && agent.id !== exceptId && agent.name.trim().toLocaleLowerCase('en') === key)
}

type FixtureModel = { id:string; displayName:string; supportedThinking:string[]; defaultThinking?:string; serviceTiers:unknown[]; isDefault?:boolean }

function fixtureCatalog(runtimeId: string) {
  const runtime = baseRuntimes.find((item) => item.id === runtimeId)
  const models: FixtureModel[] = runtimeId === 'runtime-opencode' ? [
    { id: 'opencode-go/preview', displayName: 'Preview model', supportedThinking: ['low', 'high', 'max'], serviceTiers: [] },
    { id: 'opencode-go/alternate-preview', displayName: 'Alternate preview', supportedThinking: [], serviceTiers: [] },
  ] : runtime?.provider === 'claude' ? [
    { id: 'claude-opus-5-5', displayName: 'Claude Opus 5.5', supportedThinking: ['low', 'medium', 'high', 'xhigh', 'max', ...(fixtureSettings['claude.enableWorkflows'] === true ? ['ultracode'] : [])], defaultThinking: 'medium', serviceTiers: [{ id: 'fast', name: 'Fast' }], isDefault: true },
    { id: 'claude-sonnet-5-5', displayName: 'Claude Sonnet 5.5', supportedThinking: ['low', 'medium', 'high', 'xhigh', 'max', ...(fixtureSettings['claude.enableWorkflows'] === true ? ['ultracode'] : [])], defaultThinking: 'medium', serviceTiers: [], isDefault: false },
    { id: 'claude-haiku-4-5', displayName: 'Claude Haiku 4.5', supportedThinking: ['low', 'medium'], defaultThinking: 'low', serviceTiers: [], isDefault: false },
  ] : [
    // Raw effort values and offered order mirror the prompt-free Codex 0.161.0 catalog fixture.
    { id: 'gpt-6.1-sol', displayName: 'GPT-6.1-Sol', supportedThinking: ['low', 'medium', 'high', 'xhigh', 'max', 'ultra'], defaultThinking: 'low', serviceTiers: [], isDefault: true },
    { id: 'gpt-6-astra', displayName: 'GPT-6-Astra', supportedThinking: ['low', 'medium', 'high', 'xhigh', 'max', 'ultra'], defaultThinking: 'low', serviceTiers: [], isDefault: false },
    { id: 'gpt-6-sol', displayName: 'GPT-6-Sol', supportedThinking: ['low', 'medium', 'high', 'xhigh', 'max', 'ultra'], defaultThinking: 'low', serviceTiers: [], isDefault: false },
    { id: 'gpt-6-luna', displayName: 'GPT-6-Luna', supportedThinking: ['low', 'medium', 'high', 'xhigh', 'max'], defaultThinking: 'low', serviceTiers: [], isDefault: false },
    { id: 'gpt-5.6-sol', displayName: 'GPT-5.6-Sol', supportedThinking: ['low', 'medium', 'high', 'xhigh', 'max', 'ultra'], defaultThinking: 'low', serviceTiers: [], isDefault: false },
    { id: 'gpt-5.6-terra', displayName: 'GPT-5.6-Terra', supportedThinking: ['low', 'medium', 'high', 'xhigh', 'max', 'ultra'], defaultThinking: 'low', serviceTiers: [], isDefault: false },
    { id: 'gpt-5.6-luna', displayName: 'GPT-5.6-Luna', supportedThinking: ['low', 'medium', 'high', 'xhigh', 'max'], defaultThinking: 'low', serviceTiers: [], isDefault: false },
  ]
  return { runtimeId, provider: runtime?.provider ?? '', models, validated: true, source: 'fixture', fetchedAt: '2026-10-04T00:00:00Z' }
}

export const inDesktop = true
export const previewMode = true
/**
 * `session.repository` for the preview: what the rail shows for a repository. `?repo=` picks the state:
 * changes (default), clean, nogit, error, many (a long list, cut short) or slow (the first read takes 2 s).
 * Visual fixtures only; no Git runs and nothing is ever written.
 */
const repositoryDiff = (name: string, added: string[], removed: string[]) => [`diff --git a/${name} b/${name}`, `--- a/${name}`, `+++ b/${name}`, `@@ -1,${removed.length + 2} +1,${added.length + 2} @@`, ' import { render } from "./render"', ...removed.map((line) => `-${line}`), ...added.map((line) => `+${line}`), ' export {}'].join('\n')
const fixtureStaged = new Map<string, string>()
const withStaged = <T extends { files: Array<Record<string, unknown>> }>(answer: T): T => ({ ...answer, files: answer.files.map((file) => ({ ...file, staged: fixtureStaged.get(String(file.repositoryPath)) ?? 'none' })) })
// Phase 8: `?subbots=1` gives Codex a fleet.
const stoppedHelpers = new Map<string, string>()
let fixtureDelegations: Map<string, DelegationRecord> | undefined
function delegationFixtures() {
  return fixtureDelegations ??= new Map((fixtureScene?.delegations ?? []).flatMap((row) => {
    const record = normalizeDelegation(row)
    return record ? [[record.id, record]] : []
  }))
}
function delegationWire(row: DelegationRecord) {
  return { ...row, workspace: { path: row.workspacePath, worktree: row.worktree } }
}
function fixtureSubbots() {
  const row = (agentId: string, name: string, model: string, status: DelegationStatus, doingNow: string | null, error: string | null = null) => ({ agentId, parentAgentId: 'agent-codex', originSessionId: 'session-codex', name, role: 'Research', provider: 'claude', model, thinking: null, sessionId: 'session-claude-2', delegationId: `delegation-${agentId}`, status, doingNow, startedAt: null, finishedAt: null, error })
  return [row('sub-1', 'Pricing research', 'claude-haiku-5-5', 'running', 'Reading pricing.md'), row('sub-2', 'Competitors', 'claude-haiku-5-5', 'running', 'Running a search'), row('sub-3', 'Reviews', 'claude-haiku-5-5', 'queued', null), row('sub-4', 'Licences', 'claude-haiku-5-5', 'failed', null, 'The usage limit was reached.')].map((helper) => {
    const finishedAt = stoppedHelpers.get(helper.delegationId)
    return finishedAt ? { ...helper, status: 'cancelled' as const, doingNow: null, finishedAt } : helper
  })
}
// Phase 5: accounts, custom models and secrets. `?secretrequest=1` shows a bot asking for one.
let fixtureAccounts: Array<{ id: string; provider: string; name: string; signedIn: boolean; label: string | null; plan: string | null; bots: number }> = [{ id: 'preview-account-1', provider: 'claude', name: 'Work', signedIn: true, label: 'work@example.com', plan: 'max', bots: 2 }]

// Phase 10: what `import.*` answers in the browser preview. Synthetic rows only.
function fixtureImportSources() {
  return [
    { source: 'claude', label: 'Claude Code', folder: 'C:\\Users\\you\\.claude\\projects', conversations: 62, recent: 18, newest: '2026-11-13T21:05:00Z', oldest: '2025-02-01T09:00:00Z', bytes: 356_000_000, error: null },
    { source: 'codex', label: 'Codex', folder: 'C:\\Users\\you\\.codex\\sessions', conversations: 44, recent: 11, newest: '2026-11-13T23:36:00Z', oldest: '2025-03-12T18:00:00Z', bytes: 268_000_000, error: null },
    { source: 'opencode', label: 'OpenCode', folder: 'C:\\Users\\you\\.local\\share\\opencode\\opencode.db', conversations: 465, recent: 96, newest: '2026-11-14T08:10:00Z', oldest: '2025-05-04T12:00:00Z', bytes: 164_000_000, error: null },
  ]
}

function fixtureImportRows(): ImportRow[] {
  return [
    { conversationId: '9afaf311-90be-4722-92c3-42957a067383', source: 'claude', agent: 'claude', agentLabel: 'Claude Code', title: 'Invoice parser tests', projectPath: 'C:/Users/you/Projects/Retinue', firstUser: 'Why is the invoice parser skipping line 2?', messageCount: 42, bytes: 1_240_000, updatedAt: '2026-11-13T21:05:00Z', canResumeWith: 'claude', alreadyImported: false, importedSessionId: null, error: null },
    { conversationId: '0c2b1c4a-1f5e-4a5e-9d3a-4c9f4a1a2b3c', source: 'claude', agent: 'claude', agentLabel: 'Claude Code', title: 'Rename the billing route', projectPath: 'C:/Users/you/Projects/Billing', firstUser: 'Move /invoice to /invoices and keep the old one working.', messageCount: 18, bytes: 420_000, updatedAt: '2026-11-12T14:20:00Z', canResumeWith: 'claude', alreadyImported: false, importedSessionId: null, error: null },
    { conversationId: 'rollout-2026-11-13T11-00-00-0190aaaa-bbbb-cccc-dddd-eeeeffff0000', source: 'codex', agent: 'codex', agentLabel: 'Codex', title: 'Add a health check endpoint', projectPath: 'C:/Users/you/Projects/Retinue', firstUser: 'Add a health check endpoint to the daemon.', messageCount: 27, bytes: 8_900_000, updatedAt: '2026-11-13T11:10:00Z', canResumeWith: 'codex', alreadyImported: false, importedSessionId: null, error: null },
    { conversationId: 'ses_ec891daf6ffeDRFk6xOTB3X8kb', source: 'opencode', agent: 'opencode', agentLabel: 'OpenCode', title: 'Install coding CLIs', projectPath: 'C:/Users/you/Documents/Default Project', firstUser: 'Install the opencode, claude and codex CLIs on this machine.', messageCount: 63, bytes: 980_000, updatedAt: '2026-11-14T08:10:00Z', canResumeWith: 'opencode', alreadyImported: false, importedSessionId: null, error: null },
  ]
}

function fixtureImportedRow() {
  return { ...fixtureImportRows()[0], conversationId: 'seen-1', title: 'Old migration notes', alreadyImported: true, importedSessionId: 'preview-import-seen-1' }
}

// The coding-agent installer in the preview. Synthetic versions, hosts and digests only.
let fixtureInstallJob: { jobId: string; provider: string; mode: string; startedAt: number; version: string | null; bytesTotal: number; cancelled: boolean } | null = null
const fixtureManagedCopies = new Set<string>(['pi'])
function fixtureInstallPlan(provider: string) {
  const names: Record<string, string> = { claude: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', pi: 'Pi', cursor: 'Cursor CLI', grok: 'Grok CLI', antigravity: 'Antigravity', qwen: 'Qwen Code' }
  const pages: Record<string, string> = { cursor: 'https://cursor.com/cli', grok: 'https://x.ai/cli', antigravity: 'https://antigravity.google/docs/getting-started', qwen: 'https://github.com/QwenLM/qwen-code' }
  const damaged = fixturePreset() === 'agent-install-damaged' && provider === 'pi'
  const managed = fixtureManagedCopies.has(provider)
  const base = { provider, displayName: names[provider] ?? provider, target: `C:\\Users\\you\\AppData\\Local\\Retinue\\agents\\${provider}`, freeBytes: 182_000_000_000, runsTurn: false, page: pages[provider] ?? 'https://example.com', installed: { version: managed && !damaged ? '1.5.2' : null, digestPrefix: managed && !damaged ? '4be1c0de9a71' : null, damaged }, previousVersion: null, autoUpdate: fixtureSettings[`agents.${provider}.autoUpdate`] === true }
  if (pages[provider]) return { ...base, mode: 'link', version: null, sourceHost: null, bytes: null, digestPrefix: null, digestAlgorithm: null, supported: false, reason: 'Retinue does not download this coding agent yet. Open its install page instead.', owner: null }
  if (provider === 'claude' || provider === 'codex' || provider === 'opencode') return { ...base, mode: 'adopt', version: provider === 'claude' ? '2.1.330' : provider === 'codex' ? '0.160.0' : '2.2.9', sourceHost: 'registry.npmjs.org', bytes: null, digestPrefix: null, digestAlgorithm: null, supported: true, reason: null, owner: 'npm', runsTurn: provider === 'codex' && flags().has('install-busy') }
  return { ...base, mode: 'manage', version: '1.6.0', sourceHost: 'github.com', bytes: 62_400_000, digestPrefix: '9f2c41ab07de', digestAlgorithm: 'sha256', supported: true, reason: null, owner: 'none' }
}
function fixtureInstallJobValue(job: NonNullable<typeof fixtureInstallJob>) {
  const elapsed = Date.now() - job.startedAt
  const total = job.bytesTotal
  const value = { jobId: job.jobId, provider: job.provider, mode: job.mode, version: job.version, bytesTotal: total, line: null as string | null, error: null as string | null }
  if (job.cancelled) return { ...value, phase: 'cancelled', bytesDone: 0, error: 'The install was cancelled. Nothing was changed.' }
  if (fixturePreset() === 'agent-install-progress') return { ...value, phase: 'downloading', bytesDone: Math.round(total * 0.41) }
  if (elapsed < 3000) return { ...value, phase: 'downloading', bytesDone: Math.round(total * Math.min(1, elapsed / 3000)) }
  if (fixturePreset() === 'agent-install-failed') return { ...value, phase: 'failed', bytesDone: total, error: "The download did not match the publisher's checksum. Nothing was installed.", line: 'sha256 mismatch: expected 9f2c41ab07de, got 1d0e77c2b8a4' }
  if (elapsed < 4000) return { ...value, phase: 'checking', bytesDone: total }
  if (elapsed < 5000) return { ...value, phase: 'installing', bytesDone: total }
  if (job.mode === 'manage') fixtureManagedCopies.add(job.provider)
  return { ...value, phase: 'done', bytesDone: total }
}

function fixtureImportedIds() {
  return ['preview-import-9afaf311-90be-4722-92c3-42957a067383']
}
let fixtureCustomModels: Array<{ id: string; provider: string; modelId: string; label: string; efforts: string[] }> = []
let fixtureSecrets: Array<{ name: string; updatedAt: string | null; usedBy: string[] }> = [{ name: 'GITHUB_TOKEN', updatedAt: null, usedBy: ['Codex'] }]
let fixtureSecretAnswered = false
let fixturePushOn = false
// Phase 4: another machine. `?machines=1` shows one, `?machines=offline` one that can't be reached.
let fixtureSelfName = 'Home desktop'
let fixtureMachineList: Array<Record<string, unknown>> | null = null
let fixturePairJob: { id: string; url: string; state: string; detail: string | null; machine: { id: string; name: string } | null } | null = null
const fixtureMachineSent: Record<string, Array<Record<string, unknown>>> = {}
let fixtureTakeOver: { id: string; state: string; steps: Array<{ id: string; status: string; detail: string | null }>; sessionId: string | null; detail: string | null } | null = null
let fixtureShared: SharedSettings = { found: true, valid: true, problems: [], settings: { setupScript: 'npm install', defaultBot: 'Rae', approvalMode: 'ask' }, hash: 'preview', trusted: false, state: 'untrusted' }
function fixtureMachines(): Array<Record<string, unknown>> {
  if (fixtureMachineList) return fixtureMachineList
  const wanted = flags().get('machines')
  if (!wanted) return []
  const online = wanted !== 'offline'
  // 17 November 2026: `kind` (OS) and `live` (an open event socket); `?machines=2` adds the Linux VPS.
  const laptop = { id: 'preview-laptop', name: 'Work laptop', url: 'https://work-laptop.example.ts.net', auth: 'code', fingerprint: null, tier: 'operator', pairedAt: new Date(Date.now() - 86_400_000).toISOString(), lastSeenAt: new Date(Date.now() - (online ? 20_000 : 7_200_000)).toISOString(), state: online ? 'online' : 'offline', detail: null, version: '0.1.0-beta.44', kind: 'windows', live: online, bots: [
    { id: 'preview-rae', name: 'Rae', color: 'mint', role: 'Frontend', provider: 'claude', model: 'claude-sonnet-5-5', busy: online, activeSessionId: 'preview-remote-1', conversations: 2 },
    { id: 'preview-tom', name: 'Tom', color: 'sky', role: null, provider: 'codex', model: 'gpt-6-luna', busy: false, activeSessionId: null, conversations: 0 },
  ] }
  const vps = { id: 'preview-vps', name: 'VPS', url: 'https://vps.example.ts.net', auth: 'code', fingerprint: null, tier: 'operator', pairedAt: new Date(Date.now() - 3_600_000).toISOString(), lastSeenAt: new Date(Date.now() - 5_000).toISOString(), state: 'online', detail: null, version: '0.2.0-beta.5', kind: 'linux', live: false, bots: [
    { id: 'preview-ivy', name: 'Ivy', color: 'lilac', role: 'Backend', provider: 'opencode', model: null, busy: false, activeSessionId: null, conversations: 1 },
  ] }
  return wanted === '2' ? [laptop, vps] : [laptop]
}
// Another machine's shared folder for new projects, and its devices (17 November 2026).
const fixtureRemoteBase = '/home/owner/projects'
const fixtureRemoteTree: Record<string, string[]> = { '/home/owner/projects': ['korus', 'retinue', 'site'], '/home/owner/projects/korus': ['src', 'tests'], '/home/owner/projects/retinue': ['apps', 'crates', 'docs'], '/home/owner/projects/site': [] }
let fixtureRemoteClone: { id: string; url: string; target: string; state: string; detail: string | null; project: { id: string; name: string } | null; polls: number } | null = null
let fixtureRemoteDevices = [
  { id: 'dev-phone', name: "Owner's phone", kind: 'phone', platform: 'iOS 19', tier: 'chat', login: null, pairedAt: new Date(Date.now() - 6 * 86_400_000).toISOString(), lastSeenAt: new Date(Date.now() - 600_000).toISOString(), online: false },
  { id: 'dev-home', name: 'Home desktop', kind: 'machine', platform: 'Windows', tier: 'operator', login: null, pairedAt: new Date(Date.now() - 86_400_000).toISOString(), lastSeenAt: new Date().toISOString(), online: true },
]
let fixtureRemotePending: Record<string, unknown> | null = flags().has('pending-device') ? { pairingId: 'pair-1', name: 'Work tablet', kind: 'tablet', platform: 'Android 16', login: null, method: 'tailscale', expiresAt: new Date(Date.now() + 240_000).toISOString() } : null
let fixtureRemoteApprovals = flags().has('remote-approval') ? [{ id: 'perm-remote-1', sessionId: 'preview-remote-1', kind: 'tool', title: 'Run a command', command: 'npm test -- invoice', choices: ['allow_once', 'deny'] }] : []
let fixtureAbandoned = [
  { id: 'wt-1', repo: 'C:/work/site', path: 'C:/work/site-retinue/fix-the-header', branch: 'retinue/fix-the-header', reason: 'conversation_deleted', dirtyFiles: 0, commitsAhead: 0, lastActivityAt: '2026-10-20T10:00:00Z' },
  { id: 'wt-2', repo: 'C:/work/site', path: 'C:/work/site-retinue/try-a-new-rail', branch: 'retinue/try-a-new-rail', reason: 'conversation_deleted', dirtyFiles: 2, commitsAhead: 3, lastActivityAt: '2026-10-22T10:00:00Z' },
  { id: 'wt-3', repo: 'C:/work/korus', path: 'C:/work/korus-retinue/invoice-tests', branch: 'retinue/invoice-tests', reason: 'conversation_archived', dirtyFiles: 0, commitsAhead: 1, lastActivityAt: '2026-10-25T10:00:00Z' },
]
let fixtureClone: { id: string; url: string; target: string; state: string; detail: string | null; project: null } | null = null

async function fixtureRepository(sessionId: string) {
  const mode = flags().get('repo') ?? 'changes'
  if (mode === 'slow') await new Promise((resolve) => window.setTimeout(resolve, 2000))
  const root = 'C:/Users/me/work/site'
  const base = { projectPath: root, repoRoot: root, gitRepo: true, branch: 'feature/rail', detached: false, files: [] as Array<Record<string, unknown>>, addedLines: 0, removedLines: 0, truncated: false, error: null as string | null }
  if (mode === 'nogit') return { ...base, repoRoot: null, gitRepo: false, branch: null }
  if (mode === 'error') return { ...base, error: 'git is not installed or could not be started.' }
  if (mode === 'clean') return base
  const files = [
    { path: `${root}/src/App.tsx`, repositoryPath: 'src/App.tsx', change: 'modified', addedLines: 12, removedLines: 3, diff: repositoryDiff('src/App.tsx', ['const title = "Retinue"', 'render(title)'], ['render()']), binary: false, truncated: false },
    { path: `${root}/src/rail.ts`, repositoryPath: 'src/rail.ts', change: 'added', addedLines: 10_352, removedLines: 0, diff: repositoryDiff('src/rail.ts', ['export const rail = 1'], []), binary: false, truncated: true },
    { path: `${root}/docs/notes.md`, repositoryPath: 'docs/notes.md', change: 'added', addedLines: 41, removedLines: 0, diff: repositoryDiff('docs/notes.md', ['# Notes'], []), binary: false, truncated: false },
    { path: `${root}/old/legacy.ts`, repositoryPath: 'old/legacy.ts', change: 'deleted', addedLines: 0, removedLines: 94, diff: null, binary: false, truncated: false },
    { path: `${root}/src/Header.tsx`, repositoryPath: 'src/Header.tsx', change: 'renamed', addedLines: 2, removedLines: 2, diff: repositoryDiff('src/Header.tsx', ['export const Header = 1'], ['export const Head = 1']), binary: false, truncated: false },
    { path: `${root}/assets/logo.png`, repositoryPath: 'assets/logo.png', change: 'modified', addedLines: null, removedLines: null, diff: null, binary: true, truncated: false },
  ]
  if (mode === 'many') for (let index = 0; index < 24; index += 1) files.push({ path: `${root}/src/gen/file${index}.ts`, repositoryPath: `src/gen/file${index}.ts`, change: 'modified', addedLines: index + 1, removedLines: index, diff: repositoryDiff(`src/gen/file${index}.ts`, ['const a = 1'], ['const a = 0']), binary: false, truncated: false })
  const addedLines = files.reduce((sum, file) => sum + (Number(file.addedLines) || 0), 0)
  const removedLines = files.reduce((sum, file) => sum + (Number(file.removedLines) || 0), 0)
  return { ...base, files, addedLines: sessionId ? addedLines : 0, removedLines, truncated: mode === 'many' }
}

let fixtureHires: Array<{ projectId: string; agentId: string; hiredAt: number }> = []
const fixtureReleased = new Set<string>()
/** Members: explicit hires plus, like the daemon's migration, default projects and conversations. */
function fixtureMembers(projectId: string) {
  const ids = new Set(fixtureHires.filter((item) => item.projectId === projectId).map((item) => item.agentId))
  for (const agent of agents) if (!agent.archived && agent.systemRole !== 'manager' && agent.projectId === projectId) ids.add(agent.id)
  for (const session of sessions) if (session.agentId && (session as { projectId?: string | null }).projectId === projectId) ids.add(session.agentId)
  return [...ids].filter((id) => !fixtureReleased.has(`${projectId}:${id}`) && agents.some((agent) => agent.id === id && agent.systemRole !== 'manager')).map((agentId, sortOrder) => ({ agentId, hiredAt: 0, sortOrder }))
}

const profileSample = { id: 'preview-launcher', name: 'Codex launcher', provider: 'codex', protocolFamily: 'codex_app_server', executablePath: 'C:/Tools/codex.exe', args: ['--no-alt-screen'], workingDirectoryPolicy: 'per_session' }
const fixtureProfiles = new Map<string, unknown>(flags().get('demo-profiles') === 'saved' ? [[profileSample.id, profileSample]] : [])
const warmedSessions = new Set<string>()
let ownerGrantScheduled = false
let ownerGrantLive = flags().has('demo-owner-grant')
const skillsStamp = '2026-11-20T12:00:00Z'
const learnedSkills = new Map<string, LearnedSkill>(flags().has('demo-learned') ? [['invoice-tests', {
  name: 'invoice-tests', description: 'Run the invoice parser regression tests.', reviewState: 'pending',
  learnedBy: 'agent-codex', learnedByName: 'Codex', ownerEdited: false, importedAt: skillsStamp, updatedAt: skillsStamp, usedAt: null, enabled: true,
}]] : [])
const learnedBodies = new Map<string, string>()

export async function rpc<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  const failure = flags().get('demo-rpc-error')
  if (failure === method) {
    const code = flags().get('demo-error-code')
    if (code) throw rpcError(code, workflowError(method, code) ?? 'The local daemon request failed.')
    // Some production catch handlers use their own copy only when the failure has no message.
    throw { code: 'internal' }
  }
  if (method === 'agent.reorder') {
    if (!baseRuntimes.some((runtime) => runtime.id === params.runtimeId)) throw rpcError('not_found', 'Runtime not found.')
    const active = agents.filter((agent) => agent.runtimeId === params.runtimeId && !agent.archived && !agent.parentAgentId)
    const ids: unknown[] = Array.isArray(params.agentIds) ? params.agentIds : []
    if (ids.length !== active.length || new Set(ids).size !== ids.length || active.some((agent) => !ids.includes(agent.id))) {
      throw rpcError('invalid_argument', 'The roster order is out of date.')
    }
    const ordered = active.map((agent) => {
      const sortOrder = ids.indexOf(agent.id)
      return sortOrder === agent.sortOrder ? agent : { ...agent, sortOrder, updatedAt: new Date().toISOString() }
    }).sort((a, b) => a.sortOrder - b.sortOrder)
    agents = sortAgents(agents.map((agent) => ordered.find((item) => item.id === agent.id) ?? agent))
    for (const agent of ordered) if (!active.includes(agent)) notifyFixtureDaemon('agent.changed', { action: 'reordered', agent, agentId: agent.id })
    return { agents: ordered } as T
  }
  if (method === 'push.key') return { publicKey: 'cHJldmlldw', enabled: flags().get('demo-push') !== 'off-at-computer' } as T
  if (method === 'push.subscribe') return { subscribed: true } as T
  if (method === 'push.unsubscribe') return { subscribed: false } as T
  if (method === 'diagnostics.preview') return diagnosticsPreview as T
  if (method === 'runtime.profile.list') return { profiles: [...fixtureProfiles.values()] } as T
  if (method === 'runtime.profile.save') {
    const profile = params.profile
    const key = profile && typeof profile === 'object' && 'id' in profile && typeof profile.id === 'string' ? profile.id : ''
    fixtureProfiles.set(key, profile ?? null)
    return { saved: true } as T
  }
  if (method === 'runtime.profile.delete') {
    fixtureProfiles.delete(typeof params.profileId === 'string' ? params.profileId : '')
    return { deleted: true } as T
  }
  if (method === 'runtime.auth.clear') {
    if (!params.runtimeId) throw rpcError('invalid_argument', 'runtimeId is required')
    const runtime = baseRuntimes.find((item) => item.id === params.runtimeId)
    if (!runtime) throw rpcError('not_found', 'Runtime not found.')
    clearedSignIns.add(runtime.id)
    const { runtimes } = currentSnapshot()
    notifyFixtureDaemon('runtime.changed', { runtimes })
    return { runtimes } as T
  }
  if (method === 'usage.summary') {
    const from = typeof params.from === 'string' ? params.from : '0000'
    const to = typeof params.to === 'string' ? params.to : '9999'
    return { from, to, inputTokens: null, outputTokens: null, cacheReadTokens: null, cacheWriteTokens: null, reasoningTokens: null } as T
  }
  if (method === 'owner_computer.stop') {
    const stopped = ownerGrantLive && (!params.sessionId || params.sessionId === 'session-codex')
    if (stopped) {
      ownerGrantLive = false
      notifyFixtureDaemon('owner_computer.grant', { sessionId: 'session-codex', agentId: 'agent-codex', state: 'ended', endsAt: null })
    }
    return { stopped } as T
  }
  if (method === 'message.image') {
    const session = sessions.find((item) => item.id === params.sessionId)
    const message = session?.messages?.find((item) => item.id === params.messageId)
    const index = params.index
    if (typeof index !== 'number' || !Number.isInteger(index) || index < 0) throw rpcError('invalid_argument', 'index must be the position of an image in this message')
    const attachments: unknown[] = Array.isArray(message?.attachments) ? message.attachments : []
    const attachment = attachments[index]
    if (!attachment || typeof attachment !== 'object' || !('name' in attachment)) throw rpcError('not_found', 'That image is not part of this message.')
    if (flags().get('demo-image') === 'missing') throw rpcError('not_found', 'The image file is no longer available.')
    const image = await readClipboardImage()
    const dataUrl = 'data:image/png;base64,' + btoa(Array.from(image, (byte) => String.fromCharCode(byte)).join(''))
    return { messageId: message?.id, index, name: attachment.name, mediaType: 'image/png', dataUrl } as T
  }
  if (method === 'exec.snapshot.latest' || method === 'session.warm' || method === 'session.model.plan') {
    if (!params.sessionId) throw rpcError('invalid_argument', 'sessionId is required')
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'Conversation not found.')
    if (method === 'exec.snapshot.latest') {
      if (!flags().has('demo-execution') || !session.model) return null as T
      const model = session.model
      return {
        id: 'preview-execution', sessionId: session.id, turnId: 'turn-codex-1', agentId: session.agentId,
        runtimeId: session.runtimeId, provider: 'codex', createdAt: minutesAgo(8), status: 'applied',
        requested: { model }, applied: { model: { requested: model, applied: true } },
        evidence: { model: { kind: 'provider_echo', value: model } }, instructionSha256: null, adapterFlags: {},
      } as T
    }
    if (method === 'session.warm') {
      const active = warmedSessions.has(session.id)
      const warming = !active && session.state === 'idle' && !session.archived && session.link?.kind !== 'side' && session.link?.kind !== 'automation' && !flags().has('offline')
      if (warming) warmedSessions.add(session.id)
      return { warming, active } as T
    }
    const model = typeof params.model === 'string' ? params.model.trim() : ''
    if (!model) throw rpcError('invalid_argument', 'Choose a concrete model. Conversation model selections cannot be unset.')
    const provider = baseRuntimes.find((item) => item.id === session.runtimeId)?.provider ?? ''
    const currentModel = session.model ?? null
    const modelChanged = currentModel !== model
    const hasHistory = Boolean(session.messages?.length)
    const continuity = modelChanged && hasHistory ? 'recap' : 'none'
    const reason = continuity === 'recap'
      ? 'This runtime cannot switch models inside a chat, so the new model starts a fresh session and is given a short summary of the conversation; it will not have the full working memory.'
      : modelChanged ? 'There is no conversation context to lose, so the new model starts a fresh provider session.' : 'This conversation already uses this model.'
    const plan = { ok: true, sessionId: session.id, provider, currentModel, nextModel: model, modelChanged, hasHistory, continuity, busy: session.state !== 'idle', contextAtRisk: continuity === 'recap', locked: false, reason } satisfies ModelChangePlan & { ok: boolean; sessionId: string; provider: string; currentModel: string | null; nextModel: string; hasHistory: boolean }
    return plan as T
  }
  if (method === 'manager.troubleshoot') {
    const original = sessions.find((item) => item.id === params.sessionId)
    const manager = agents.find((agent) => agent.systemRole === 'manager' && !agent.archived)
    if (!params.sessionId) throw rpcError('invalid_argument', 'sessionId is required.')
    if (!manager) throw rpcError('conflict', 'There is no CEO yet. Set up a CEO first, then ask them to troubleshoot.')
    if (!original) throw rpcError('not_found', 'That conversation is no longer available.')
    const session: Session = {
      id: `preview-troubleshoot-${sessions.length}`, runtimeId: manager.runtimeId, agentId: manager.id,
      title: `Troubleshoot: ${original.title}`, state: 'idle', updatedAt: new Date().toISOString(),
      messages: [
        { id: 'm-troubleshoot-request', role: 'user', content: `Troubleshoot the failed turn in "${original.title}".`, createdAt: new Date().toISOString() },
        { id: 'm-troubleshoot', role: 'assistant', content: 'The coding agent stopped before completing the turn. Check its sign-in and retry the message.', createdAt: new Date().toISOString() },
      ],
    }
    sessions = [...sessions, session]
    notifyFixtureDaemon('session.changed', { session })
    return { sessionId: session.id } as T
  }

  if (method === 'app.snapshot') return await fetchSnapshot() as T
  if (method === 'permissions.policy.get') return await permissionsPolicyGet() as T
  if (method === 'session.prompt' || method.startsWith('session.suggestion.')) { const reply = (await import('./fixtureSuggestions')).fixtureSuggestionRpc(method, params, fixtureSettings, daemonListeners, notifyFixtureDaemon); if (reply !== undefined) return reply as T }
  if (method === 'remote.preview.pair') {
    // Preview only: act as the phone submitting the code for the live offer. It
    // raises `remote.pairing.requested`; the owner still approves or rejects.
    ensureRemoteFixture()
    return remoteSimulator.simulatePhoneRequest({ code: typeof params.code === 'string' ? params.code : undefined }) as T
  }
  if ((REMOTE_METHODS as readonly string[]).includes(method)) {
    ensureRemoteFixture()
    return remoteSimulator.call(method as RemoteMethod, params) as Promise<T>
  }
  if (method === 'doctor.run') {
    if (flags().get('demo-health') === 'error') throw rpcError('internal', 'Health check could not run.')
    const checks: HealthCheck[] = [
      { id: 'database', area: 'database', label: 'Database', status: 'ok', detail: 'The database passed its integrity check.', fix: null },
      { id: 'runtime', area: 'runtime', label: 'Coding agents', status: fixtureHealthRefreshed ? 'ok' : 'fail', detail: fixtureHealthRefreshed ? 'Coding agents responded.' : 'A coding agent did not respond.', fix: { label: 'Scan again', method: 'runtime.refresh' } },
      { id: 'speech', area: 'speech', label: 'Speech model', status: 'warn', detail: 'No speech model is downloaded.', fix: null },
    ]
    return { checks } as T
  }
  if (method === 'runtime.refresh') {
    fixtureHealthRefreshed = true
    if (flags().has('demo-scan-error')) throw rpcError('provider_unavailable', 'Coding agents could not be scanned.')
    discoveryAttempts += 1
    // ?discovery-timeout-once exercises timeout, background recovery and confirmation.
    if (flags().has('discovery-timeout-once') && discoveryAttempts === 1) await new Promise((resolve) => window.setTimeout(resolve, 31_000))
    else await new Promise((resolve) => window.setTimeout(resolve, 900))
    const runtimes = baseRuntimes.map((runtime) => fixturePreset() === 'opencode-runtime-error' && runtime.id === 'runtime-opencode'
      ? { ...runtime, probeStep: 'Protocol check', probeError: 'The CLI does not advertise the required protocol.', status: 'error' }
      : flags().has('discovery-probe-error') && runtime.provider === 'codex' ? { ...runtime, probeStep: 'Protocol check', probeError: 'Probe timed out.', status: 'error' } : { ...runtime })
    notifyFixtureDaemon('runtime.changed', { runtimes })
    return { runtimes } as T
  }
  if (method === 'events.replay') return { replayAvailable: true, events: [] } as T
  if (method === 'quota.list') return { quotas: fixtureQuotas() } as T
  if (method === 'quota.refresh') { window.setTimeout(() => notifyFixtureDaemon('quota.updated', {}), 600); return { started: true } as T }
  if (method === 'settings.get') {
    // Preview flags: ?activity turns on Agent activity, ?usage-bar the usage bar, ?chat=small|medium|large sizes the companion chat.
    if (flags().has('experiments')) for (const { name } of EXPERIMENTS) fixtureSettings[`experimental.${name}`] = true
    if (flags().has('activity')) fixtureSettings['experimental.activityDetails'] = true
    if (flags().has('usage-bar')) fixtureSettings['experimental.usageBar'] = true
    if (flags().has('logos')) fixtureSettings['experimental.runtimeLogos'] = true
    if (flags().get('style')) fixtureSettings['appearance.style'] = flags().get('style')
    if (flags().get('style') && ['haze','stone'].includes(String(flags().get('style')))) localStorage.setItem('retinue.style', String(flags().get('style')))
    if (['system','dark','light'].includes(String(flags().get('theme')))) { fixtureSettings['appearance.theme'] = flags().get('theme'); try { const saved = JSON.parse(localStorage.getItem('retinue.appearance') ?? '{}'); localStorage.setItem('retinue.appearance', JSON.stringify({ ...saved, theme: flags().get('theme') })) } catch {} }
    if (flags().get('accent')) fixtureSettings['appearance.accent'] = flags().get('accent')
    if (flags().get('density')) fixtureSettings['appearance.density'] = flags().get('density')
    for (const key of ['wallpaper','glass','font','monoFont']) { const value = flags().get(key); if (value) fixtureSettings[`appearance.${key}`] = value }
    if (flags().get('wallpaper')) localStorage.setItem('retinue.wallpaper', String(flags().get('wallpaper')))
    if (flags().get('glass')) localStorage.setItem('retinue.glass', String(flags().get('glass')))
    if (flags().get('font')) localStorage.setItem('retinue.font', String(flags().get('font')))
    if (flags().get('monoFont')) localStorage.setItem('retinue.monoFont', String(flags().get('monoFont')))
    if (flags().has('seen-hints')) fixtureSettings['hints.seen'] = ['companion.drag']
    if (flags().has('clean')) { fixtureSettings['hints.seen'] = PREVIEW_HINTS; fixtureSettings['onboarding.tour'] = { version: 1, done: true, step: 0 } }
    if (flags().has('hints-reset')) fixtureSettings['hints.seen'] = []
    if (flags().has('tour-invite')) fixtureSettings['onboarding.tour'] = { version: 0, done: false, step: 0 }
    if (flags().has('tour')) fixtureSettings['onboarding.tour'] = { version: 1, done: false, step: 2 }
    if (flags().has('focus')) fixtureSettings['companion.focusMode'] = true
    if (flags().has('positions')) fixtureSettings['companion.positions'] = { [String(flags().get('positions') ?? 'display')]: { x: 120, y: 900 } }
    return { settings: { ...fixtureSettings, ...(fixturePreset() === 'usage-tip-90' ? { 'experimental.usageTips': true } : {}) } } as T
  }
  if (method === 'settings.set') {
    if (params.key === 'permissions.default_mode' && params.value === 'bypass' && params.confirm !== true) throw rpcError('invalid_argument', 'Confirm making Full access the default first.')
    fixtureSettings[String(params.key)] = params.value
    return { saved: true } as T
  }
  if (method === 'session.repository') return withStaged(await fixtureRepository(String(params.sessionId ?? ''))) as T
  if (method === 'session.git.diff') {
    const file = (await fixtureRepository(String(params.sessionId ?? ''))).files.find((item) => item.repositoryPath === params.repositoryPath)
    if (!file) return { repositoryPath: params.repositoryPath, previousPath: null, change: 'modified', diff: null, truncated: false, binary: false, addedLines: null, removedLines: null, staged: 'none' } as T
    const long = ['import { render } from "./render"', '', ...Array.from({ length: 6 }, (_, index) => ` const line${index} = ${index}`)]
    const diff = file.diff ? `${file.diff}\n@@ -40,6 +42,7 @@${'\n'}${long.map((line) => line.startsWith(' ') ? line : ` ${line}`).join('\n')}\n-const oldest = load("the old value that was on a very long line which keeps going so wrapping can be seen in the side by side layout")\n+const newest = load("the new value")\n+const extra = true` : null
    return { ...file, previousPath: null, diff, staged: fixtureStaged.get(String(params.repositoryPath)) ?? 'none' } as T
  }
  if (method === 'session.git.stage') {
    for (const path of params.paths as string[]) fixtureStaged.set(path, params.staged === true ? 'all' : 'none')
    return { files: withStaged(await fixtureRepository(String(params.sessionId ?? ''))).files } as T
  }
  if (method === 'session.git.revert') return { undoId: 'preview-undo', name: String(params.repositoryPath), files: withStaged(await fixtureRepository(String(params.sessionId ?? ''))).files } as T
  if (method === 'session.git.revertUndo') return { files: withStaged(await fixtureRepository(String(params.sessionId ?? ''))).files } as T
  if (method === 'session.pr.draft') {
    const reason = flags().get('pr') === 'nogh' ? "The GitHub CLI isn't signed in." : null
    return { ready: !reason, reason, gh: { available: !reason, login: reason ? null : 'preview-user' }, repoRoot: 'C:/Users/me/work/site', branch: 'feature/rail', base: 'main', isWorktree: true, remote: 'origin', commitsAhead: 2, uncommitted: 6, title: 'Tidy the rail', body: '## Changes\n- Tidy the rail\n- Fix the header\n\n## Files\n6 files, +10,407 \u2212105\n\nMade with Retinue.', existingUrl: flags().get('pr') === 'open' ? 'https://github.com/preview/site/pull/12' : null } as T
  }
  if (method === 'session.pr.create') {
    await new Promise((resolve) => window.setTimeout(resolve, 700))
    return { steps: [{ id: 'commit', status: params.commitMessage ? 'done' : 'skipped', detail: null }, { id: 'push', status: 'done', detail: null }, { id: 'pr', status: 'done', detail: null }], url: 'https://github.com/preview/site/pull/13' } as T
  }
  if (method === 'remote.push.get') return { enabled: fixturePushOn, publicKey: 'preview', devices: 0 } as T
  if (method === 'remote.push.set') { fixturePushOn = params.enabled === true; return { enabled: fixturePushOn } as T }
  if (method === 'automation.webhook.enable' || method === 'automation.webhook.rotate') return { url: 'https://example.ts.net/api/hook/' + String(params.id), secret: 'preview-secret-shown-once', webhook: { enabled: true, includeBody: false } } as T
  if (method === 'automation.webhook.update') return { webhook: { enabled: true, includeBody: params.includeBody === true } } as T
  if (method === 'automation.webhook.disable') return { webhook: { enabled: false, includeBody: false } } as T
  if (method === 'delegation.list') {
    return { delegations: [...delegationFixtures().values()].filter((item) => item.originSessionId === params.originSessionId).map(delegationWire) } as T
  }
  if (method === 'delegation.cancel' || method === 'delegation.cancelGroup') {
    if (method === 'delegation.cancelGroup' && (typeof params.originSessionId !== 'string'
      || (!params.originTurnId && (!Array.isArray(params.delegationIds) || !params.delegationIds.length)))) {
      throw rpcError('invalid_argument', 'Choose a conversation and assignment group.')
    }
    const helpers = flags().has('subbots') ? fixtureSubbots() : []
    const matches = (row: DelegationRecord) => method === 'delegation.cancel' ? row.id === params.delegationId
      : row.originSessionId === params.originSessionId && (typeof params.originTurnId === 'string'
        ? row.originTurnId === params.originTurnId : Array.isArray(params.delegationIds) && params.delegationIds.includes(row.id))
    const helperRows: DelegationRecord[] = helpers.map((helper) => ({
      id: helper.delegationId, originSessionId: helper.originSessionId, originTurnId: null, sideSessionId: helper.sessionId, sideTurnId: null,
      fromAgentId: helper.parentAgentId, targetAgentId: helper.agentId, topic: helper.name, briefPreview: '',
      status: helper.status, error: helper.error, workspacePath: null, worktree: null,
      createdAt: null, startedAt: helper.startedAt, finishedAt: helper.finishedAt, updatedAt: null, queueReason: null,
    }))
    const selected = [...delegationFixtures().values(), ...helperRows].filter(matches)
    if (method === 'delegation.cancel' && !selected.length) throw rpcError('not_found', 'That assignment no longer exists.')
    const now = new Date().toISOString()
    const delegations = selected.map((row): DelegationRecord => {
      if (row.status !== 'running' && row.status !== 'queued') return row
      const stopped: DelegationRecord = { ...row, status: 'cancelled', finishedAt: now, updatedAt: now, queueReason: null }
      if (delegationFixtures().has(row.id)) {
        delegationFixtures().set(row.id, stopped)
        const side = sessions.find((session) => session.id === row.sideSessionId)
        if (side && row.status === 'running' && side.state === 'working') finishFixtureTurn(side, 'cancelled', row.sideTurnId ?? undefined)
      } else {
        stoppedHelpers.set(row.id, now)
        notifyFixtureDaemon('subbot.changed', { sessionId: row.originSessionId, parentAgentId: row.fromAgentId })
      }
      notifyFixtureDaemon('delegation.updated', { delegation: delegationWire(stopped) })
      return stopped
    })
    return (method === 'delegation.cancel' ? { delegation: delegationWire(delegations[0]) } : { delegations: delegations.map(delegationWire) }) as T
  }
  if (method === 'subbot.list') return { subbots: flags().get('subbots') && (params.sessionId === 'session-codex' || params.agentId === 'agent-codex') ? fixtureSubbots() : [] } as T
  if (method === 'account.list') return { accounts: fixtureAccounts } as T
  if (method === 'account.create') { const account = { id: `preview-account-${fixtureAccounts.length + 1}`, provider: String(params.provider), name: String(params.name), signedIn: false, label: null, plan: null, bots: 0 }; fixtureAccounts = [...fixtureAccounts, account]; return { account } as T }
  if (method === 'account.rename') { fixtureAccounts = fixtureAccounts.map((item) => item.id === params.id ? { ...item, name: String(params.name) } : item); return { ok: true } as T }
  if (method === 'account.delete') { fixtureAccounts = fixtureAccounts.filter((item) => item.id !== params.id); return { ok: true } as T }
  if (method === 'account.check') { fixtureAccounts = fixtureAccounts.map((item) => item.id === params.id ? { ...item, signedIn: true, label: 'work@example.com', plan: 'pro' } : item); return { ok: true } as T }
  if (method === 'account.signIn') return { command: `$env:CLAUDE_CONFIG_DIR='C:\\Users\\you\\AppData\\Local\\Retinue\\accounts\\${params.id}'; claude auth login` } as T
  if (method === 'model.custom.list') return { models: fixtureCustomModels.filter((item) => item.provider === params.provider) } as T
  if (method === 'model.custom.save') { const model = { id: `preview-model-${fixtureCustomModels.length + 1}`, provider: String(params.provider), modelId: String(params.modelId), label: String(params.label), efforts: params.efforts as string[] }; fixtureCustomModels = [...fixtureCustomModels, model]; return { model } as T }
  if (method === 'model.custom.delete') { fixtureCustomModels = fixtureCustomModels.filter((item) => item.id !== params.id); return { ok: true } as T }
  // Preview-only vault, chat apps and Pi (docs/plans/vault-channels-pi.md). Values are never kept here either.
  if (method.startsWith('vault.') || method.startsWith('channel.') || method.startsWith('pi.')) return previewVaultChannelsPi(method, params) as T
  if (method.startsWith('connector.')) return previewConnectorsRpc(method, params) as T
  if (method === 'secret.list') return { secrets: fixtureSecrets } as T
  if (method === 'secret.set') { fixtureSecrets = [...fixtureSecrets.filter((item) => item.name !== params.name), { name: String(params.name), updatedAt: new Date().toISOString(), usedBy: [] }]; return { ok: true } as T }
  if (method === 'secret.delete') { fixtureSecrets = fixtureSecrets.filter((item) => item.name !== params.name); return { ok: true } as T }
  if (method === 'secret.request.list') return { requests: flags().get('secretrequest') && !fixtureSecretAnswered ? [{ id: 'preview-request', sessionId: 'session-codex', name: 'DEPLOY_TOKEN', reason: 'I need it to check the staging deploy finished.', botName: 'Codex', expiresAt: null }] : [] } as T
  if (method === 'secret.request.answer' || method === 'secret.request.decline') { fixtureSecretAnswered = true; return { ok: true } as T }
  if (method === 'machine.self') return { id: 'preview-self', name: fixtureSelfName, version: '0.1.0-beta.44' } as T
  if (method === 'machine.self.rename') { fixtureSelfName = String(params.name); return { id: 'preview-self', name: fixtureSelfName, version: '0.1.0-beta.44' } as T }
  if (method === 'machine.list') return { machines: fixtureMachines() } as T
  if (method === 'machine.refresh' || method === 'machine.rename') return { ok: true } as T
  if (method === 'machine.remove') { fixtureMachineList = fixtureMachines().filter((item) => item.id !== params.id); return { removed: true, signedOut: true } as T }
  if (method === 'machine.probe') {
    const probe = flags().get('demo-machine-probe')
    if (probe === 'unreachable' || probe === 'wrong-server') return { origin: params.url, reachable: probe === 'wrong-server', isRetinue: false, auth: 'code', fingerprint: null, trusted: false } as T
    return (String(params.url).includes('ts.net')
    ? { origin: String(params.url), reachable: true, isRetinue: true, auth: 'code', fingerprint: null, trusted: true }
    : { origin: String(params.url), reachable: true, isRetinue: true, auth: 'password', fingerprint: 'AB:12:9F:44:0C:7E:D1:52:6A:33:BE:08:91:C4:7F:20:5D:EE:16:48:A9:02:C7:3B:60:1F:84:D5:72:96:BC:0E', trusted: false }) as T
  }
  if (method === 'machine.pair.begin') {
    fixturePairJob = { id: 'preview-pair', url: String(params.url), state: 'pending', detail: null, machine: null }
    const job = fixturePairJob
    window.setTimeout(() => {
      if (fixturePairJob !== job) return
      const mode = flags().get('demo-machine-pair') ?? ''
      const details: Record<string, string> = pairingFailures
      const detail = details[mode]
      fixturePairJob = detail ? { ...job, state: 'failed', detail }
        : mode === 'expired' || mode === 'rejected' || mode === 'cancelled' ? { ...job, state: mode }
        : mode === 'failed' ? { ...job, state: 'failed' }
        : { ...job, state: 'approved', machine: { id: 'preview-laptop', name: 'Work laptop' } }
      notifyFixtureDaemon('machine.pair.changed', { job: fixturePairJob })
    }, 1800)
    return { job: fixturePairJob } as T
  }
  if (method === 'machine.pair.list') return { jobs: fixturePairJob ? [fixturePairJob] : [] } as T
  if (method === 'machine.pair.cancel') { fixturePairJob = null; return { ok: true } as T }
  if (method === 'machine.conversations') return { conversations: [
    { id: 'preview-remote-1', title: 'Fix the invoice export', updatedAt: new Date(Date.now() - 120_000).toISOString(), projectName: 'korus', state: 'working' },
    { id: 'preview-remote-2', title: 'Tidy the settings page', updatedAt: new Date(Date.now() - 86_400_000).toISOString(), projectName: 'korus', state: 'idle' },
  ] } as T
  if (method === 'machine.start') {
    const id = `preview-remote-new-${Date.now().toString(36)}`
    fixtureMachineSent[id] = []
    return { sessionId: id, machineId: params.machineId } as T
  }
  if (method === 'machine.prompt' || method === 'machine.steer') {
    const id = String(params.sessionId)
    ;(fixtureMachineSent[id] ??= []).push({ id: `sent-${Date.now()}`, role: 'user', content: String(params.text ?? ''), createdAt: new Date().toISOString() })
    return { ok: true } as T
  }
  if (method === 'machine.cancel') return { cancelled: true } as T
  if (method === 'machine.conversation' && String(params.sessionId).startsWith('preview-remote-new-')) {
    return { session: { id: params.sessionId, title: 'New conversation', state: 'idle', messages: fixtureMachineSent[String(params.sessionId)] ?? [] } } as T
  }
  if (method === 'machine.sessions') {
    const rows = fixtureMachines().flatMap((machine) => (machine.bots as Array<Record<string, unknown>>).flatMap((bot) => bot.id === 'preview-rae' ? [
      { machineId: machine.id, machineName: machine.name, kind: machine.kind ?? null, id: 'preview-remote-1', title: 'Fix the invoice export', agentId: bot.id, agentName: bot.name, projectName: 'korus', state: 'working', updatedAt: new Date(Date.now() - 120_000).toISOString(), archived: false },
      { machineId: machine.id, machineName: machine.name, kind: machine.kind ?? null, id: 'preview-remote-2', title: 'Tidy the settings page', agentId: bot.id, agentName: bot.name, projectName: 'korus', state: 'idle', updatedAt: new Date(Date.now() - 86_400_000).toISOString(), archived: false },
    ] : bot.id === 'preview-tom' ? [
      { machineId: machine.id, machineName: machine.name, kind: machine.kind ?? null, id: 'preview-remote-3', title: 'Review the pricing page PR', agentId: bot.id, agentName: bot.name, projectName: 'korus', state: 'idle', updatedAt: new Date(Date.now() - 3 * 86_400_000).toISOString(), archived: false },
    ] : bot.id === 'preview-ivy' ? [
      { machineId: machine.id, machineName: machine.name, kind: machine.kind ?? null, id: 'preview-remote-4', title: 'Rotate the deploy keys', agentId: bot.id, agentName: bot.name, projectName: 'infra', state: 'idle', updatedAt: new Date(Date.now() - 9 * 86_400_000).toISOString(), archived: false },
    ] : []))
    return { sessions: params.machineId ? rows.filter((row) => row.machineId === params.machineId) : rows } as T
  }
  if (method === 'machine.stream.status') return { connected: fixtureMachines().find((item) => item.id === params.machineId)?.live === true, lastSequence: 42 } as T
  if (method === 'machine.permissions') return { permissions: fixtureRemoteApprovals.filter((card) => !params.sessionId || card.sessionId === params.sessionId) } as T
  if (method === 'machine.permission.reply') { fixtureRemoteApprovals = fixtureRemoteApprovals.filter((card) => card.id !== params.permissionId); return { resolved: true } as T }
  if (method === 'machine.project.browse') {
    const path = String(params.path || fixtureRemoteBase)
    if (!path.startsWith(fixtureRemoteBase)) throw new Error('That folder is outside the folder this computer shares with other machines.')
    const names = fixtureRemoteTree[path] ?? []
    return { base: fixtureRemoteBase, path, parent: path === fixtureRemoteBase ? null : path.slice(0, path.lastIndexOf('/')), entries: names.map((name) => ({ name, kind: 'folder' })), truncated: false } as T
  }
  if (method === 'machine.project.create') return { project: { id: `remote-project-${Date.now().toString(36)}`, name: String(params.name || String(params.path).split('/').pop()), path: params.path } } as T
  if (method === 'machine.project.clone') {
    const name = String(params.name || String(params.url).replace(/\.git$/, '').split('/').pop())
    fixtureRemoteClone = { id: 'clone-remote-1', url: String(params.url), target: `${params.parent}/${name}`, state: 'running', detail: null, project: null, polls: 0 }
    return { job: { ...fixtureRemoteClone } } as T
  }
  if (method === 'machine.clone.status') {
    if (!fixtureRemoteClone) throw new Error('That clone is no longer listed.')
    fixtureRemoteClone.polls += 1
    if (fixtureRemoteClone.polls >= 2) { fixtureRemoteClone.state = 'done'; fixtureRemoteClone.project = { id: 'remote-project-clone', name: String(fixtureRemoteClone.target.split('/').pop()) } }
    return { job: { ...fixtureRemoteClone } } as T
  }
  if (method === 'machine.remote.status') return { enabled: true, link: 'online', url: 'https://work-laptop.example.ts.net', method: 'tailscale', deviceCount: fixtureRemoteDevices.length, connectedCount: 1 } as T
  if (method === 'machine.devices.list') return { devices: fixtureRemoteDevices } as T
  if (method === 'machine.devices.update') { fixtureRemoteDevices = fixtureRemoteDevices.map((row) => row.id === params.deviceId ? { ...row, ...(params.tier ? { tier: String(params.tier) } : {}), ...(params.name ? { name: String(params.name) } : {}) } : row); return { device: fixtureRemoteDevices.find((row) => row.id === params.deviceId) } as T }
  if (method === 'machine.devices.revoke') {
    if (params.deviceId === 'dev-home') throw new Error("A machine link can't change or remove its own pairing.")
    fixtureRemoteDevices = fixtureRemoteDevices.filter((row) => row.id !== params.deviceId); return { revoked: true } as T
  }
  if (method === 'machine.pair.pending') return { pending: fixtureRemotePending } as T
  if (method === 'machine.pair.confirm') { fixtureRemotePending = null; return params.approve ? { approved: true, device: null } as T : { approved: false } as T }
  if (method === 'machine.activity') return { entries: [
    { id: 'a2', at: new Date(Date.now() - 600_000).toISOString(), deviceName: "Owner's phone", kind: 'prompt', summary: "Owner's phone sent a message to Rae." },
    { id: 'a1', at: new Date(Date.now() - 3_600_000).toISOString(), deviceName: 'Home desktop', kind: 'connected', summary: 'Home desktop connected.' },
  ] } as T
  if (method === 'machine.conversation') {
    const working = params.sessionId === 'preview-remote-1'
    return { session: { id: params.sessionId, title: working ? 'Fix the invoice export' : 'Tidy the settings page', state: working ? (fixtureRemoteApprovals.length ? 'waiting_permission' : 'working') : 'idle', tools: working ? [
      { id: 't1', title: 'Read', kind: 'read', state: 'completed', startedAt: new Date(Date.now() - 500_000).toISOString(), detail: { path: 'src/export/invoice.ts' } },
      { id: 't2', title: 'Bash', kind: 'execute', state: 'completed', startedAt: new Date(Date.now() - 400_000).toISOString(), detail: { label: 'Ran npm test -- invoice' } },
      { id: 't3', title: 'Edit', kind: 'edit', state: 'running', startedAt: new Date(Date.now() - 200_000).toISOString(), detail: { path: 'src/export/invoice.test.ts' } },
    ] : [], messages: [
      { id: 'r1', role: 'user', content: working ? 'The invoice export drops the last row. Can you find out why?' : 'Group the settings by what they change.', createdAt: new Date(Date.now() - 600_000).toISOString() },
      { id: 'r2', role: 'assistant', content: working ? 'Found it. The loop stops one row early because `end` is exclusive.\n\nI\'m adding a test for it now.' : 'Done. They\'re grouped under **Account**, **Appearance** and **Behaviour**.', createdAt: new Date(Date.now() - 300_000).toISOString() },
      ...(fixtureMachineSent[String(params.sessionId)] ?? []),
    ] } } as T
  }
  if (method === 'machine.takeOver.preview') return {
    ready: params.sessionId !== 'preview-remote-1', reason: params.sessionId === 'preview-remote-1' ? 'Rae is working. Stop it or wait, then try again.' : null,
    title: 'Tidy the settings page', bot: { id: 'preview-rae', name: 'Rae', provider: 'claude', model: null, agentId: 'preview-rae', willCreate: flags().get('takeover') === 'newbot' },
    repo: { identity: 'github.com/example/korus', url: 'https://github.com/example/korus.git', branch: 'retinue/tidy-the-settings', isWorktree: true, hasRemote: true },
    uncommitted: 2, commitsAhead: 3, project: flags().get('takeover') === 'noproject' ? null : { id: 'korus', name: 'korus' }, needsProject: flags().get('takeover') === 'noproject', runtimeReady: true,
  } as T
  if (method === 'machine.takeOver') {
    fixtureTakeOver = { id: 'preview-takeover', state: 'running', steps: [{ id: 'prepare', status: 'done', detail: null }], sessionId: null, detail: null }
    window.setTimeout(() => { fixtureTakeOver = { id: 'preview-takeover', state: 'done', steps: ['prepare', 'code', 'bot', 'import', 'commit', 'open'].map((id) => ({ id, status: 'done', detail: null })), sessionId: String(params.sessionId), detail: null } }, 2200)
    return { job: fixtureTakeOver } as T
  }
  if (method === 'machine.takeOver.list') return { jobs: fixtureTakeOver ? [fixtureTakeOver] : [] } as T
  if (method === 'project.sharedSettings.get') {
    const demo = flags().get('demo-shared')
    if (demo && fixtureShared.state === 'untrusted') {
      if (demo === 'trusted') fixtureShared = { ...fixtureShared, trusted: true, state: 'trusted' }
      if (demo === 'invalid') fixtureShared = { found: true, valid: false, problems: ["The file isn't valid JSON."], settings: null, hash: null, trusted: false, state: 'invalid' }
      if (demo === 'missing') fixtureShared = { found: false, valid: false, problems: [], settings: null, hash: null, trusted: false, state: 'none' }
    }
    return fixtureShared as T
  }
  if (method === 'project.sharedSettings.trust') { fixtureShared = { ...fixtureShared, trusted: true, state: 'trusted' }; return fixtureShared as T }
  if (method === 'project.sharedSettings.untrust') { fixtureShared = { ...fixtureShared, trusted: false, state: 'untrusted' }; return fixtureShared as T }
  if (method === 'project.sharedSettings.write') return fixtureShared as T
  if (method === 'worktree.abandoned') return { items: fixtureAbandoned } as T
  if (method === 'worktree.removeAbandoned') {
    fixtureAbandoned = fixtureAbandoned.filter((item) => item.id !== params.id)
    return { removed: true, branchDeleted: params.deleteBranch === true, detail: null } as T
  }
  if (method === 'project.clone') {
    fixtureClone = { id: 'preview-clone', url: String(params.url), target: `${params.parent}\\${params.name ?? String(params.url).split('/').pop()?.replace(/\.git$/, '')}`, state: 'running', detail: null, project: null }
    window.setTimeout(() => { if (fixtureClone?.state === 'running') fixtureClone = { ...fixtureClone, state: flags().get('clone') === 'fail' ? 'failed' : 'done', detail: flags().get('clone') === 'fail' ? 'Repository not found.' : null } }, 2500)
    return { job: fixtureClone } as T
  }
  if (method === 'project.clone.list') return { jobs: fixtureClone ? [fixtureClone] : [] } as T
  if (method === 'project.clone.cancel') { if (fixtureClone) fixtureClone = { ...fixtureClone, state: 'cancelled' }; return { ok: true } as T }
  if (method === 'project.discover') return { folder: params.folder, truncated: false, repos: [
    { path: `${params.folder}\\korus`, name: 'korus', branch: 'main', alreadyAdded: true },
    { path: `${params.folder}\\site`, name: 'site', branch: 'feature/rail', alreadyAdded: false },
    { path: `${params.folder}\\tools\\scripts`, name: 'scripts', branch: 'main', alreadyAdded: false },
  ] } as T
  if (method === 'project.files.search') {
    const query = String(params.query ?? '').toLowerCase()
    const files = ['src/App.tsx', 'src/ui/ChangesView.tsx', 'src/ui/DiffBody.tsx', 'docs/notes.md', 'README.md', 'package.json']
    const found = files.flatMap((path) => {
      let at = 0
      const matches: number[] = []
      for (let index = 0; index < path.length && at < query.length; index += 1) if (path[index]!.toLowerCase() === query[at]) { matches.push(index); at += 1 }
      return at === query.length ? [{ path, name: path.split('/').pop() ?? path, score: 100 - path.length, matches }] : []
    })
    return { files: found.slice(0, Number(params.limit ?? 20)), truncated: false, source: 'git' } as T
  }
  if (method === 'project.content.search') {
    if (flags().has('demo-search-pattern')) return { results: [], truncated: false, source: 'walk', reason: "That isn't a valid search pattern." } as T
    if (params.regex === true && /[[(]$/.test(String(params.query))) return { results: [], truncated: false, source: 'git', reason: "That isn't a valid search pattern." } as T
    const query = String(params.query ?? '')
    if (query === 'nothing') return { results: [], truncated: false, source: 'git', reason: null } as T
    return { truncated: false, source: 'git', reason: null, results: [
      { path: 'src/App.tsx', line: 12, preview: `const title = "${query}"` },
      { path: 'src/App.tsx', line: 88, preview: `render(${query})` },
      { path: 'src/ui/DiffBody.tsx', line: 41, preview: `  // Uses ${query} when the mode changes` },
    ] } as T
  }
  if (method === 'project.suggestions') {
    if (!projects.some((project) => project.id === params.projectId)) throw rpcError('not_found', 'That project no longer exists.')
    const suggestions: ProjectSuggestion[] = [
      { id: 'preview-readme', kind: 'readme', title: 'Explore the invoice parser', prompt: 'Read the project overview and explain how the invoice parser is organised.', source: { file: 'README.md', detail: 'Project overview' } },
      { id: 'preview-script', kind: 'script', title: 'Review the test setup', prompt: 'Explain how the test script runs the invoice parser tests.', source: { file: 'package.json', detail: 'test' } },
      { id: 'preview-guide', kind: 'agents', title: 'Read the project instructions', prompt: 'Summarise the working rules in AGENTS.md before making changes.', source: { file: 'AGENTS.md', detail: 'Working in this project' } },
    ]
    return { suggestions, degraded: false } as T
  }
  if (method === 'session.get') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'That conversation is no longer available.')
    if (!params.beforeMessageId) return { ...session } as T
    const index = session.messages?.findIndex((message) => message.id === params.beforeMessageId) ?? -1
    if (index < 0) throw rpcError('invalid_argument', 'The message is not in this conversation.')
    const page: Session = { ...session, messages: session.messages?.slice(0, index), turns: [], tools: [], files: [], turnsTruncated: true, toolsTruncated: true, filesTruncated: true }
    return page as T
  }
  if (method === 'session.list') return { sessions: sessions.filter((session) => params.includeArchived === true || !session.archived).map((session) => ({ ...session })) } as T
  if (method === 'session.rename') {
    const session = sessions.find((item) => item.id === params.sessionId)
    const title = typeof params.title === 'string' ? params.title.trim() : ''
    if (!session) throw rpcError('not_found', 'Conversation not found.')
    if (!title || title.length > 120) throw rpcError('invalid_argument', 'Title must be between 1 and 120 characters.')
    session.title = title
    session.updatedAt = new Date().toISOString()
    notifyFixtureDaemon('session.changed', { session: { ...session } })
    return { session } as T
  }
  if (method === 'session.archive') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'Conversation not found.')
    session.archived = params.archived === true
    session.updatedAt = new Date().toISOString()
    notifyFixtureDaemon('session.changed', { session: { ...session } })
    return { session } as T
  }
  if (method === 'reach.statement') {
    // Wave 2: the daemon's plain statement of what a conversation's bot can reach.
    const session = sessions.find((item) => item.id === params.sessionId)
    const agent = agents.find((item) => item.id === session?.agentId)
    const name = agent?.name ?? 'This bot'
    const folder = session?.projectPath ?? null
    return { reach: {
      sessionId: session?.id ?? null, agentId: agent?.id ?? null, botName: name,
      statement: `${name} runs as you, as your own Windows account, in ${folder ?? 'no project folder'}, with your network and the tools you have installed.`,
      folder, isolation: 'shared', runsAs: 'your own Windows account', network: 'your network', permissionMode: 'ask', permissionSentence: 'Every tool call asks you first.', unattended: false,
      tools: [
        { id: 'files', label: 'Files in its project folder', on: !!folder },
        { id: 'commands', label: 'Commands in a terminal', on: true },
        { id: 'internet', label: 'The internet', on: true },
        { id: 'team', label: 'Messages to teammates', on: true },
        { id: 'computer', label: 'Your linked computer', on: false },
      ],
      limits: ['It runs as your own Windows account, so it can change anything you can.', 'Every tool call asks you first.', "The coding agent's own sandbox, if it has one, is its own: Retinue does not add one."],
    } } as T
  }
  if (method === 'permission.rules.list') return { rules: [...fixtureApprovalRules.values()] } as T
  if (method === 'permission.rules.update' || method === 'permission.rules.delete') {
    const id = String(params.id ?? '')
    const rule = fixtureApprovalRules.get(id)
    if (!rule) throw rpcError('not_found', 'That approval no longer exists.')
    if (method === 'permission.rules.delete') {
      fixtureApprovalRules.delete(id)
      notifyFixtureDaemon('permission.rules.changed', { agentId: rule.agentId })
      return { deleted: true } as T
    }
    const project = projects.find((item) => item.id === params.projectId)
    if (params.scope === 'project' && !project) throw rpcError('not_found', 'That project no longer exists.')
    const updated = { ...rule, projectId: project?.id ?? null, projectName: project?.name ?? null }
    fixtureApprovalRules.set(id, updated)
    notifyFixtureDaemon('permission.rules.changed', { agentId: rule.agentId })
    return { rule: updated } as T
  }
  if (method === 'session.environment' || method === 'session.workspace' || method === 'session.checkpoints') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'That conversation is no longer available.')
    if (flags().get('demo-rail-error') === method) throw rpcError('internal', 'The project folder could not be read.')
    const folder = session.projectPath ?? null
    const branch = folder ? 'main' : null
    if (method === 'session.environment') {
      const environment: Environment = { projectPath: folder, gitRepo: !!folder, branch, detached: false, host: 'Windows' }
      return environment as T
    }
    if (method === 'session.workspace') {
      const workspace: Workspace = { sessionId: session.id, mode: folder ? 'shared' : 'unknown', projectPath: folder ?? '', isGit: !!folder, repoRoot: folder, branch, head: null, worktree: null, isolationFailure: null, sharedWith: [] }
      return workspace as T
    }
    const files: Checkpoints['turns'][number]['files'] = (session.files ?? []).flatMap((file) => typeof file.id !== 'string' || typeof file.path !== 'string' ? [] : [{
      id: file.id, path: file.path, turnId: typeof file.turnId === 'string' ? file.turnId : null,
      operation: typeof file.operation === 'string' ? file.operation : null,
      additions: typeof file.additions === 'number' ? file.additions : null, deletions: typeof file.deletions === 'number' ? file.deletions : null,
      state: fixtureRevertedFiles.has(file.id) ? 'reverted' : 'available', reason: null,
    }])
    const checkpoints: Checkpoints = {
      sessionId: session.id, busy: session.state === 'working',
      capability: { fileUndo: !!folder, fileUndoReason: folder ? null : 'No project folder.', providerStateRestore: false, providerReason: 'This coding agent does not restore its own state.' },
      turns: [...new Set(files.map((file) => file.turnId))].map((turnId) => ({ turnId, files: files.filter((file) => file.turnId === turnId) })),
    }
    return checkpoints as T
  }
  if (method === 'search.messages') {
    if (flags().has('demo-search-error')) throw rpcError('internal', 'Message search failed.')
    const query = String(params.query ?? '').trim().toLowerCase()
    const results: MessageSearchResult[] = query ? sessions.filter((session) => !session.archived && (!params.sessionId || params.sessionId === session.id)).flatMap((session) => (session.messages ?? []).filter((message) => ['user', 'assistant'].includes(String(message.role)) && String(message.content ?? message.text ?? '').toLowerCase().includes(query)).map((message) => {
      const text = String(message.content ?? message.text ?? '')
      const at = text.toLowerCase().indexOf(query)
      const start = Math.max(0, at - 45)
      const agent = agents.find((a) => a.id === session.agentId)
      return { sessionId: session.id, blobId: session.agentId ?? null, blob: agent ? { id: agent.id, name: agent.name, color: agent.color, look: agent.look, archived: agent.archived } : null, title: String(session.title ?? 'Conversation'), role: String(message.role), messageId: String(message.id ?? ''), createdAt: String(message.createdAt ?? session.updatedAt ?? ''), snippet: text.slice(start, Math.max(start + 180, at + query.length + 45)) }
    })).slice(0, 50) : []
    return { results } as T
  }
  if (method === 'file.revert') {
    const session = sessions.find((s) => s.id === params.sessionId)
    const file = session?.files?.find((f) => f.id === params.fileChangeId)
    if (!file || typeof file.id !== 'string') throw rpcError('not_found', 'File change not found.')
    if (fixtureRevertedFiles.has(file.id)) throw rpcError('conflict', 'That change has already been undone.')
    fixtureRevertedFiles.add(file.id)
    return { reverted: true } as T
  }
  if (method === 'agent.purge') {
    const index = agents.findIndex((agent) => agent.id === params.agentId)
    if (index < 0) throw rpcError('not_found', 'Employee not found.')
    const agent = agents[index]
    if (params.confirm !== true || agent.locked || agent.systemRole === 'manager') throw rpcError('conflict', 'This employee cannot be deleted.')
    const related = sessions.filter((session) => session.agentId === agent.id || session.link?.kind === 'side' && session.link.peerAgentId === agent.id)
    if (related.some((session) => ['starting', 'working', 'cancelling', 'waiting_permission'].includes(String(session.state)))) throw rpcError('conflict', 'Stop the employee first.')
    for (const session of related) { const i = sessions.findIndex((item) => item.id === session.id); sessions.splice(i, 1); notifyFixtureDaemon('session.deleted', { sessionId: session.id }) }
    agents.splice(index, 1)
    notifyFixtureDaemon('agent.changed', { agentId: agent.id, action: 'deleted' })
    return { deleted: true, sessionIds: related.map((session) => session.id) } as T
  }
  if (method === 'agent.restore') {
    const agent = agents.find((a) => a.id === params.agentId)
    if (!agent) throw rpcError('not_found', `${entityNoun(themeStyle(), { capital: true })} not found.`)
    if (agents.some((a) => !a.archived && a.id !== agent.id && a.name.toLowerCase() === agent.name.toLowerCase())) throw rpcError('conflict', `Another ${entityNoun(themeStyle())} uses this name.`)
    agent.archived = false
    notifyFixtureDaemon('agent.changed', { agent })
    return { agent: { ...agent } } as T
  }
  if (method === 'session.delete.schedule') {
    const session = sessions.find((s) => s.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'Conversation not found.')
    if (['starting', 'working', 'cancelling', 'waiting_permission'].includes(String(session.state))) throw rpcError('conflict', 'Cannot delete an active conversation.')
    const existing = pendingDeletes.get(session.id)
    if (existing) return { deadline: existing.deadline } as T
    const deadline = Date.now() + 8000
    const archived = session.archived === true
    session.archived = true
    const timer = window.setTimeout(() => { pendingDeletes.delete(session.id); void rpc('session.delete', { sessionId: session.id }) }, 8000)
    pendingDeletes.set(session.id, { deadline, timer, archived })
    notifyFixtureDaemon('session.changed', { session: { ...session } })
    return { deadline } as T
  }
  if (method === 'session.delete.cancel') {
    const id = String(params.sessionId)
    const pending = pendingDeletes.get(id)
    const session = sessions.find((s) => s.id === id)
    if (!pending || !session || Date.now() >= pending.deadline) throw rpcError('conflict', 'The Undo period has ended.')
    window.clearTimeout(pending.timer)
    pendingDeletes.delete(id)
    session.archived = pending.archived
    notifyFixtureDaemon('session.changed', { session: { ...session } })
    return { cancelled: true, session: { ...session } } as T
  }
  if (method === 'session.delete') {
    const index = sessions.findIndex((item) => item.id === params.sessionId)
    if (index < 0) throw rpcError('not_found', 'Conversation not found.')
    if (['starting', 'working', 'cancelling', 'waiting_permission'].includes(String(sessions[index].state))) throw rpcError('conflict', 'Cannot delete an active conversation.')
    const [session] = sessions.splice(index, 1)
    notifyFixtureDaemon('session.deleted', { sessionId: session.id })
    return { deleted: true } as T
  }
  if (method === 'session.model.update') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'Conversation not found.')
    if (['starting', 'working', 'cancelling', 'waiting_permission'].includes(String(session.state))) throw rpcError('conflict', 'Model and effort cannot change during an active turn.')
    if (params.model != null && (typeof params.model !== 'string' || !params.model.trim())) throw rpcError('invalid_argument', 'model must be a non-empty identifier or null')
    if (params.thinking != null && (typeof params.thinking !== 'string' || !params.thinking.trim())) throw rpcError('invalid_argument', 'thinking must be a non-empty identifier or null')
    const previousModel = session.modelLock?.model ?? session.model ?? agents.find((agent) => agent.id === session.agentId)?.model ?? null
    const previousThinking = session.modelLock?.thinking ?? agents.find((agent) => agent.id === session.agentId)?.thinking ?? null
    const nextModel = !('model' in params) ? previousModel : typeof params.model === 'string' && params.model ? params.model : null
    const nextThinking = !('thinking' in params) ? previousThinking : typeof params.thinking === 'string' && params.thinking ? params.thinking : null
    const modelChanged = previousModel !== nextModel
    if (modelChanged && params.confirmModelChange !== true) throw rpcError('confirmation_required', 'Confirm the model change before continuing.')
    const modelRows = fixtureCatalog(session.runtimeId).models
    if (nextModel && !modelRows.some((item) => item.id === nextModel)) throw rpcError('invalid_argument', 'The selected model is not in the fixture catalog.')
    const selected = modelRows.find((item) => item.id === nextModel)
    if (nextThinking && selected && !selected.supportedThinking.includes(nextThinking)) throw rpcError('invalid_argument', 'The selected effort is not advertised for this model.')
    const previousLock = session.modelLock ?? {}
    const nextLock: NonNullable<Session['modelLock']> = { model: nextModel, thinking: nextThinking }
    if ('serviceTier' in previousLock && !(modelChanged && !('serviceTier' in params))) nextLock.serviceTier = previousLock.serviceTier
    if ('approvalMode' in previousLock) nextLock.approvalMode = previousLock.approvalMode
    if ('serviceTier' in params) {
      if (params.serviceTier === null) delete nextLock.serviceTier
      else if (typeof params.serviceTier === 'string' && (selected?.serviceTiers ?? []).some((tier) => (tier as { id?: string }).id === params.serviceTier)) nextLock.serviceTier = params.serviceTier
      else throw rpcError('invalid_argument', 'The selected speed is not advertised for this model.')
    }
    if ('approvalMode' in params) {
      if (params.approvalMode === null) delete nextLock.approvalMode
      else if (params.approvalMode === 'ask' || params.approvalMode === 'auto' || params.approvalMode === 'bypass') nextLock.approvalMode = params.approvalMode
      else throw rpcError('invalid_argument', 'approvalMode must be ask, auto, bypass or null')
    }
    session.modelLock = nextLock
    session.model = nextModel ?? undefined
    session.updatedAt = new Date().toISOString()
    if (modelChanged) {
      session.contextUsed = null
      session.contextSize = null
      notifyFixtureDaemon('session.context.reset', { sessionId: session.id })
    }
    if (session.id === composerPreviewSessionId) {
      try { localStorage.setItem(composerPreviewStorageKey, JSON.stringify({ modelLock:session.modelLock, contextUsed:session.contextUsed, contextSize:session.contextSize })) } catch { /* Preview fixture storage is optional. */ }
    }
    notifyFixtureDaemon('session.changed', { session: { ...session } })
    return { session: { ...session }, modelChanged } as T
  }
  // FIX39 item 19: the side-conversation permission pill in the browser preview.
  if (method === 'session.permissions.set') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'Conversation not found.')
    if (session.link?.kind === 'automation') throw rpcError('conflict', "A scheduled run's permission mode is fixed by the schedule.")
    const approval = params.approvalMode
    if (approval !== null && !['ask', 'auto', 'bypass'].includes(String(approval))) throw rpcError('invalid_argument', 'approvalMode must be ask, auto, bypass or null')
    const nextLock: NonNullable<Session['modelLock']> = { ...(session.modelLock ?? {}) }
    if (approval === null) { delete nextLock.approvalMode; delete nextLock.approvalModeSource }
    else { nextLock.approvalMode = approval as 'ask' | 'auto' | 'bypass'; delete nextLock.approvalModeSource }
    session.modelLock = nextLock
    session.updatedAt = new Date().toISOString()
    notifyFixtureDaemon('session.changed', { session: { ...session } })
    return { session: { ...session } } as T
  }
  // FIX39 item 13: Lock and Unlock in the preview, so the badge, the read-only
  // editor and the disabled composer pills can be looked at without a daemon.
  if (method === 'agent.lock.set') {
    const agent = agents.find((item) => item.id === params.agentId)
    if (!agent) throw rpcError('not_found', `That ${entityNoun(themeStyle())} is no longer available.`)
    if (agent.systemRole === 'manager' && params.locked === true) throw rpcError('conflict', `The Manager cannot be locked.`)
    const updated: Agent = { ...agent, locked: params.locked === true, updatedAt: new Date().toISOString() }
    agents = sortAgents(agents.map((item) => item.id === updated.id ? updated : item))
    sequence += 1
    notifyFixtureDaemon('agent.changed', { agent: updated })
    return { agent: updated } as T
  }
  if (method === 'message.reaction.toggle') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session || session.archived || session.link?.kind === 'side') throw rpcError('not_found', 'Message conversation is unavailable.')
    const message = (session.messages ?? []).find((item) => item.id === params.messageId)
    const emoji = String(params.emoji ?? '')
    const allowed = ['👍', '👎', '❤️', '😂', '🎉', '😮']
    if (!message || !['user', 'assistant'].includes(String(message.role)) || !allowed.includes(emoji)) throw rpcError('invalid_argument', 'Reaction or message is unsupported.')
    const prior = Array.isArray(message.reactions) ? message.reactions.filter((item): item is string => typeof item === 'string' && allowed.includes(item)) : []
    const selected = !prior.includes(emoji)
    // One reaction per message: a different emoji replaces it, the same one removes it.
    message.reactions = selected ? [emoji] : []
    const payload = { sessionId: session.id, messageId: message.id, emoji, selected, reactions: message.reactions }
    notifyFixtureDaemon('message.reactions.changed', payload)
    return { reactions: message.reactions, selected } as T
  }
  if (typeof method === 'string' && method.startsWith('task.')) return handleFixtureTaskRpc(method, params, notifyFixtureDaemon, rpcError) as T
  if (typeof method === 'string' && method.startsWith('automation.')) {
    const find = () => fixtureAutomations.find((item) => item.id === params.id)
    const publish = (item: Record<string, unknown>) => notifyFixtureDaemon('automation.changed', { automation: { ...item } })
    if (method === 'automation.list') return { automations: fixtureAutomations.filter((item) => !params.agentId || item.agentId === params.agentId).map((item) => ({ ...item })) } as T
    if (method === 'automation.preview') {
      const schedule = params.schedule as { kind?: string; cron?: string; at?: string } | undefined
      if (!schedule || (schedule.kind === 'cron' && String(schedule.cron ?? '').trim().split(/\s+/).length !== 5)) throw rpcError('invalid_argument', 'That schedule is not valid.')
      const utc = fixtureNextRuns(schedule, Math.min(10, Math.max(1, Number(params.count) || 3)))
      return { utc, local: utc } as T
    }
    if (method === 'automation.create' || method === 'automation.update') {
      const schedule = params.schedule as Record<string, unknown> | undefined
      const name = typeof params.name === 'string' ? params.name.trim() : ''
      const prompt = typeof params.prompt === 'string' ? params.prompt.trim() : ''
      if (!name || !prompt || !schedule) throw rpcError('invalid_argument', 'A name, a prompt and a schedule are required.')
      if (params.approvalMode !== undefined && params.approvalMode !== 'plan' && params.approvalMode !== 'auto') throw rpcError('invalid_argument', 'approvalMode must be plan or auto')
      const now = new Date().toISOString()
      const existing = method === 'automation.update' ? find() : undefined
      if (method === 'automation.update' && !existing) throw rpcError('not_found', 'That schedule is gone.')
      const next = { ...(existing ?? { id: `automation-${Date.now()}`, createdBy: 'user', consecutiveFailures: 0, lastRunAt: null, lastRun: null, running: false, pauseReason: null }),
        agentId: params.agentId ?? existing?.agentId, name, prompt, schedule, sessionMode: params.sessionMode ?? existing?.sessionMode ?? 'new',
        approvalMode: params.approvalMode ?? existing?.approvalMode ?? 'auto', maxRuntimeSecs: params.maxRuntimeSecs ?? existing?.maxRuntimeSecs ?? 1800,
        enabled: params.enabled === true || (existing?.enabled === true && params.enabled !== false),
        nextRunAt: new Date(Date.now() + 3 * 3_600_000).toISOString(), updatedAt: now } as Record<string, unknown>
      if (!existing) fixtureAutomations.push(next)
      else Object.assign(existing, next)
      publish(next)
      return { automation: { ...next } } as T
    }
    const target = find()
    if (!target) throw rpcError('not_found', 'That schedule is gone.')
    if (method === 'automation.get') return { automation: { ...target } } as T
    if (method === 'automation.pause' || method === 'automation.resume') {
      const resume = method === 'automation.resume'
      target.enabled = resume
      target.pauseReason = resume ? null : 'user'
      target.consecutiveFailures = 0
      target.nextRunAt = resume ? new Date(Date.now() + 3 * 3_600_000).toISOString() : null
      publish(target)
      return { automation: { ...target } } as T
    }
    if (method === 'automation.delete') {
      fixtureAutomations.splice(fixtureAutomations.indexOf(target), 1)
      notifyFixtureDaemon('automation.deleted', { id: target.id })
      return { deleted: true } as T
    }
    if (method === 'automation.run_now') {
      const run = { id: `run-${Date.now()}`, automationId: target.id, sessionId: null, status: 'completed', reason: null, scheduledFor: new Date().toISOString(), startedAt: new Date().toISOString(), finishedAt: new Date().toISOString() }
      ;(fixtureRuns[String(target.id)] ??= []).unshift(run)
      target.lastRun = run
      target.lastRunAt = run.startedAt
      publish(target)
      return { run } as T
    }
    if (method === 'automation.runs') return { runs: fixtureRuns[String(target.id)] ?? [] } as T
  }
  if (typeof method === 'string' && method.startsWith('skills.')) {
    const find = () => fixtureSkills.find((item) => item.name === params.name)
    if (method === 'skills.list') return { skills: [...fixtureSkills.map((item) => ({ ...item, disabledFor: [...(item.disabledFor as string[])] })), ...[...learnedSkills.values()].map((skill) => ({ ...skill, source: 'agent' }))] } as T
    if (method === 'skills.learned.list') {
      const review = typeof params.reviewState === 'string' ? params.reviewState.trim() || 'all' : 'all'
      if (review !== 'all' && review !== 'pending' && review !== 'approved') throw rpcError('invalid_argument', 'reviewState must be pending, approved or all')
      const skills = [...learnedSkills.values()]
        .filter((skill) => (review === 'all' || skill.reviewState === review) && (!params.agentId || skill.learnedBy === params.agentId))
        .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
      return { skills } as T
    }
    if (method === 'skills.learned.approve' || method === 'skills.learned.edit' || method === 'skills.learned.forget') {
      const name = typeof params.name === 'string' ? params.name : ''
      const previous = learnedSkills.get(name)
      if (!previous) throw rpcError('not_found', 'That skill is not in the pool.')
      const action = method === 'skills.learned.approve' ? 'approved' : method === 'skills.learned.edit' ? 'edited' : 'forgotten'
      if (action === 'forgotten') {
        if (params.confirm !== true) throw rpcError('invalid_argument', 'Confirm forgetting this skill first.')
        learnedSkills.delete(name)
        learnedBodies.delete(name)
        notifyFixtureDaemon('skills.learned.changed', { action, name, agentId: previous.learnedBy })
        return { forgotten: true } as T
      }
      const skill: LearnedSkill = { ...previous, reviewState: 'approved', updatedAt: new Date().toISOString() }
      if (action === 'edited') {
        skill.ownerEdited = true
        if (typeof params.description === 'string') skill.description = params.description
        if (typeof params.body === 'string') learnedBodies.set(name, params.body)
      }
      learnedSkills.set(name, skill)
      notifyFixtureDaemon('skills.learned.changed', { action, name, agentId: skill.learnedBy })
      return { skill } as T
    }
    if (method === 'skills.forAgent') {
      const agentId = typeof params.agentId === 'string' ? params.agentId : ''
      if (!agentId) throw rpcError('invalid_argument', 'agentId is required')
      const skills = fixtureSkills.filter((item) => item.enabled !== false && !(item.disabledFor as string[]).includes(agentId)).map((item) => ({ name: item.name, description: item.description, source: item.source }))
      return { skills } as T
    }
    if (method === 'skills.get') {
      const learned = typeof params.name === 'string' ? learnedSkills.get(params.name) : undefined
      if (learned) {
        const body = learnedBodies.get(learned.name) ?? '# Invoice tests\n\nRun the invoice parser regression suite.'
        return { name: learned.name, description: learned.description, body, files: [{ path: 'SKILL.md', bytes: new TextEncoder().encode(body).length }] } as T
      }
      const skill = find()
      if (!skill) throw rpcError('not_found', 'That skill is not in the pool.')
      return { name: skill.name, description: skill.description, body: `# ${skill.name}\n\n${skill.description}\n\n1. Read the input.\n2. Follow the steps.\n3. Report what changed.`, files: [{ path: 'SKILL.md', bytes: 1200 }, { path: 'scripts/run.py', bytes: 3400 }] } as T
    }
    if (method === 'skills.scan') return { candidates: [
      { source: 'claude', name: 'pdf', description: 'Read, fill and merge PDF files.', path: '~/.claude/skills/pdf', status: 'same' },
      { source: 'claude', name: 'brand-voice', description: 'Keep copy in the house voice: plain, short, kind.', path: '~/.claude/skills/brand-voice', status: 'new' },
      { source: 'codex', name: 'release-notes', description: 'Write release notes from the git log.', path: '~/.codex/skills/release-notes', status: 'changed' },
    ] } as T
    if (method === 'skills.import') {
      const items = Array.isArray(params.items) ? params.items as Array<{ source: string; name: string }> : []
      for (const item of items) if (!fixtureSkills.some((skill) => skill.name === item.name)) fixtureSkills.push({ name: item.name, description: 'Imported in the preview.', source: item.source, enabled: true, fileCount: 1, totalBytes: 900, disabledFor: [], nativeFor: [] })
      notifyFixtureDaemon('skills.changed', { action: 'imported' })
      return { imported: items.map((item) => item.name), skipped: [] } as T
    }
    if (method === 'skills.delete') { const index = fixtureSkills.findIndex((item) => item.name === params.name); if (index >= 0) fixtureSkills.splice(index, 1); return { deleted: true } as T }
    if (method === 'skills.toggle') {
      const skill = find()
      if (!skill) throw rpcError('not_found', 'That skill is not in the pool.')
      if (typeof params.agentId === 'string') {
        const off = new Set(skill.disabledFor as string[])
        if (params.enabled === false) off.add(params.agentId); else off.delete(params.agentId)
        skill.disabledFor = [...off]
      } else skill.enabled = params.enabled !== false
      return { skill: { ...skill } } as T
    }
  }
  if (method === 'backup.import') return { imported: { bots: 3, memory: 14, skills: 1, automations: 1, settings: 9 }, skipped: { bots: 0, memory: 0, skills: 1, automations: 0, settings: 0 }, renamed: [{ from: String(agents[0]?.name ?? 'Mochi'), to: `${String(agents[0]?.name ?? 'Mochi')} (2)` }] } as T
  if (typeof method === 'string' && method.startsWith('memory.')) {
    const live = () => fixtureMemory.filter((item) => !item.deletedAt)
    const find = () => fixtureMemory.find((item) => item.id === params.id)
    const secret = /(sk-[A-Za-z0-9]{8,}|ghp_|AKIA|-----BEGIN|bearer\s)/i
    if (method === 'memory.list') {
      const needle = typeof params.query === 'string' ? params.query.toLowerCase() : ''
      const rows = live().filter((item) => !needle || `${item.title} ${item.body}`.toLowerCase().includes(needle))
      return { records: rows.map((item) => ({ ...item })) } as T
    }
    if (method === 'memory.create') {
      const title = typeof params.title === 'string' ? params.title.trim() : ''
      const body = typeof params.body === 'string' ? params.body.trim() : ''
      if (!title || title.length > 120 || !body || body.length > 2000) throw rpcError('invalid_argument', 'A title up to 120 characters and a note up to 2000 characters are required.')
      if (secret.test(`${title} ${body}`)) throw rpcError('invalid_argument', 'That looks like a secret. Notes must not contain passwords or keys.')
      const record = { id: `memory-${Date.now()}-${fixtureMemory.length}`, scope: params.scope ?? 'shared', scopeId: params.scopeId ?? null, kind: params.kind ?? 'fact', title, body, tags: [], importance: 50, pinned: params.pinned === true, origin: 'user', reviewed: true, version: 1, updatedAt: new Date().toISOString(), deletedAt: null } as Record<string, unknown>
      fixtureMemory.push(record)
      notifyFixtureDaemon('memory.changed', { record: { ...record } })
      return { record: { ...record } } as T
    }
    const target = find()
    if (!target) throw rpcError('not_found', 'That note is gone.')
    if (method === 'memory.get') return { record: { ...target } } as T
    if (method === 'memory.update') {
      if (params.expectedVersion !== target.version) throw rpcError('conflict', 'That note changed. Reload and try again.')
      for (const key of ['title', 'body', 'kind', 'pinned']) if (key in params) target[key] = params[key]
      target.version = Number(target.version) + 1
      target.updatedAt = new Date().toISOString()
      notifyFixtureDaemon('memory.changed', { record: { ...target } })
      return { record: { ...target } } as T
    }
    if (method === 'memory.approve') { target.reviewed = true; notifyFixtureDaemon('memory.changed', { record: { ...target } }); return { record: { ...target } } as T }
    if (method === 'memory.delete') { target.deletedAt = new Date().toISOString(); notifyFixtureDaemon('memory.deleted', { id: target.id }); return { deleted: true } as T }
    if (method === 'memory.restore') { target.deletedAt = null; notifyFixtureDaemon('memory.changed', { record: { ...target } }); return { record: { ...target } } as T }
  }
  // Wave 13: watching Claude Code outside Retinue. `?preset=outside` starts with it on and two sessions.
  if (method.startsWith('outside.')) {
    const status = () => ({ enabled: fixtureOutside.enabled, installed: fixtureOutside.enabled, settingsPath: 'C:/Users/you/.claude/settings.json', bridgeAvailable: true, problem: null })
    if (method === 'outside.status') return status() as T
    if (method === 'outside.sessions') return { sessions: fixtureOutside.enabled ? fixtureOutside.sessions : [] } as T
    if (method === 'outside.preview') return { settingsPath: status().settingsPath, exists: true, before: JSON.stringify({ model: 'opus' }, null, 2), after: JSON.stringify({ hooks: { PreToolUse: [{ matcher: '*', hooks: [{ type: 'command', command: '"C:/Program Files/Retinue/retinue-hook.exe" --outside "C:/Users/you/AppData/Roaming/Retinue/outside-hook.json"', timeout: 5 }] }] }, model: 'opus' }, null, 2) } as T
    if (method === 'outside.enable') {
      if (params.confirm !== true) throw rpcError('confirmation_required', 'Confirm the change to Claude Code’s settings first.')
      fixtureOutside.enabled = true
      notifyFixtureDaemon('outside.sessions.changed', { sessions: fixtureOutside.sessions })
      return { status: status(), backupPath: 'C:/Users/you/.claude/settings.json.retinue-backup-20261028-101500' } as T
    }
    if (method === 'outside.disable') { fixtureOutside.enabled = false; notifyFixtureDaemon('outside.sessions.changed', { sessions: [] }); return { status: status(), backupPath: null, problem: null } as T }
  }
  // Phase 1B: comparisons, the stash, new projects and issue reports.
  if (method.startsWith('fanout.')) {
    const nameOf = (id: string) => agents.find((item) => item.id === id)?.name ?? 'A removed employee'
    const view = (record: typeof fixtureFanouts[number]) => {
      const age = Date.now() - Date.parse(record.createdAt)
      return { ...record, rows: record.rows.map((row, index) => {
        const done = age > 2500 + index * 2500
        return { agentId: row.agentId, agentName: nameOf(row.agentId), sessionId: row.sessionId, error: null, state: done ? 'idle' : 'working', reply: done ? `**${nameOf(row.agentId)}'s answer.**\n\nHere is how I would approach it:\n\n1. Read the code that's there.\n2. Change the smallest thing.\n3. Run the tests.` : null, durationMs: done ? 2500 + index * 2500 : null, tokens: done && index % 2 === 0 ? 1200 + index * 340 : null, worktree: record.projectId ? { path: `C:/work/.worktrees/${row.agentId}`, branch: `retinue/compare-${index + 1}` } : null }
      }) }
    }
    if (method === 'fanout.start') {
      const ids = Array.isArray(params.agentIds) ? params.agentIds as string[] : []
      if (ids.length < 2 || ids.length > 6) throw rpcError('invalid_argument', 'Choose 2 to 6 employees.')
      const text = String(params.text ?? '')
      const project = typeof params.projectId === 'string' ? projects.find((item) => item.id === params.projectId) : undefined
      const record = { id: `fanout-${fixtureFanouts.length + 1}`, title: text.slice(0, 60), text, projectId: project?.id ?? null, projectName: project?.name ?? null, createdAt: new Date().toISOString(), chosenSessionId: null as string | null, rows: ids.map((agentId, index) => ({ agentId, sessionId: `session-compare-${fixtureFanouts.length + 1}-${index}` })) }
      fixtureFanouts.unshift(record)
      notifyFixtureDaemon('fanout.changed', { id: record.id })
      return { fanout: view(record) } as T
    }
    if (method === 'fanout.list') return { fanouts: fixtureFanouts.map(view) } as T
    const record = fixtureFanouts.find((item) => item.id === params.id)
    if (!record) throw rpcError('not_found', 'That comparison no longer exists.')
    if (method === 'fanout.get') return { fanout: view(record) } as T
    if (method === 'fanout.pick') { record.chosenSessionId = typeof params.sessionId === 'string' ? params.sessionId : null; notifyFixtureDaemon('fanout.changed', { id: record.id }); return { fanout: view(record) } as T }
    if (method === 'fanout.delete') { fixtureFanouts.splice(fixtureFanouts.indexOf(record), 1); notifyFixtureDaemon('fanout.changed', { id: record.id }); return { deleted: true } as T }
  }
  if (method.startsWith('stash.')) {
    if (method === 'stash.list') return { items: fixtureStash } as T
    if (method === 'stash.add') {
      const text = String(params.text ?? '')
      const parts = Array.isArray(params.attachments) ? params.attachments as Array<{ name?: string }> : []
      if (!text && !parts.length) throw rpcError('invalid_argument', 'There is nothing to stash.')
      const agent = agents.find((item) => item.id === params.agentId)
      const project = projects.find((item) => item.id === params.projectId)
      const item = { id: `stash-${Date.now()}`, text, attachments: parts.map((part, index) => ({ file: `${index}.png`, name: part.name ?? 'image.png' })), agentId: agent?.id ?? null, agentName: agent?.name ?? null, projectId: project?.id ?? null, projectName: project?.name ?? null, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() }
      fixtureStash.unshift(item)
      notifyFixtureDaemon('stash.changed', {})
      return { item } as T
    }
    const index = fixtureStash.findIndex((item) => item.id === params.id)
    if (index < 0) throw rpcError('not_found', 'That draft is no longer in the stash.')
    if (method === 'stash.image') return { name: 'image.png', mediaType: 'image/png', dataUrl: '' } as T
    const [item] = fixtureStash.splice(index, 1)
    notifyFixtureDaemon('stash.changed', {})
    if (method === 'stash.delete') return { deleted: true } as T
    if (method === 'stash.restore') return { item: { text: item.text, attachments: item.attachments.map((part) => ({ path: `C:/Retinue/stash-restored/${part.file}`, name: part.name })) } } as T
  }
  if (method === 'project.scaffold.status') return { git: true, gh: { available: true, login: 'preview-user', loginChecked: true } } as T
  if (method === 'project.scaffold.login') return { login: 'preview-user', loginChecked: true } as T
  if (method === 'project.scaffold') {
    const failure = flags().get('demo-scaffold-step')
    if (failure === 'folder' || failure === 'first_commit') {
      const ids = ['folder', 'project', 'git_init', 'first_commit', 'github']
      const failedAt = ids.indexOf(failure)
      return { project: failure === 'folder' ? null : { id: 'project-demo-scaffold', name: params.name, path: `${params.parentPath}/${params.name}` }, githubUrl: null, steps: ids.map((id, index) => ({
        id, status: index === failedAt ? 'failed' : index < failedAt ? 'done' : 'skipped',
        detail: index === failedAt ? scaffoldFailures[failure] : index < failedAt ? null : id === 'github' && !params.github ? 'Not asked for.' : 'An earlier step failed.',
      })) } as T
    }
    if (params.confirm !== true) throw rpcError('invalid_argument', 'Confirm creating the project first.')
    const name = String(params.name ?? '').trim()
    const path = `${String(params.parentPath ?? 'C:/work')}/${name}`
    const project: Project = { id: `project-${projects.length + 1}`, name, path, sortOrder: projects.length, collapsed: false } as Project
    projects = [...projects, project]
    notifyFixtureDaemon('project.changed', { action: 'created', projectId: project.id })
    const wantsGithub = !!params.github
    return { project, githubUrl: wantsGithub ? `https://github.com/preview-user/${name}` : null, steps: [
      { id: 'folder', status: 'done', detail: null }, { id: 'project', status: 'done', detail: null },
      { id: 'git_init', status: params.git ? 'done' : 'skipped', detail: null }, { id: 'first_commit', status: params.git ? 'done' : 'skipped', detail: null },
      { id: 'github', status: wantsGithub ? 'done' : 'skipped', detail: null },
    ] } as T
  }
  if (method === 'diagnostics.issueDraft') {
    const body = '## What happened\n\n\n## Version\n\nRetinue 0.1.0-beta.43 on Windows 11\n\n## Coding agents\n\n- claude 2.1\n- codex 0.9\n\n## Health check\n\nEverything passed.\n'
    return { repo: 'sico-vibes/retinue', title: '', body, url: 'https://github.com/sico-vibes/retinue/issues/new', gh: { available: true, login: 'preview-user' } } as T
  }
  if (method === 'diagnostics.issuePost') {
    if (params.confirm !== true) throw rpcError('invalid_argument', 'Confirm posting the issue first.')
    return { url: 'https://github.com/sico-vibes/retinue/issues/1' } as T
  }
  // Wave 14: the Retinue MCP. `?preset=mcp` starts with it on and used a few minutes ago.
  if (method.startsWith('external.')) {
    const bridge = 'C:/Users/you/AppData/Local/com.retinue.desktop/retinue-mcp.exe'
    const endpoint = 'C:/Users/you/AppData/Local/com.retinue.desktop/external-mcp.json'
    const status = () => ({ enabled: fixtureExternal.enabled, tier: fixtureExternal.tier, endpointPath: fixtureExternal.enabled ? endpoint : null, bridgePath: bridge, command: `claude mcp add retinue -- "${bridge}" mcp-server --endpoint "${endpoint}"`, lastCallAt: fixtureExternal.lastCallAt, lastClient: fixtureExternal.lastClient, allowFullAccess: fixtureExternal.enabled && fixtureExternal.allowFullAccess })
    const changed = () => { notifyFixtureDaemon('external.changed', status()); return status() as T }
    if (method === 'external.status') return status() as T
    if (method === 'external.enable') {
      if (params.confirm !== true) throw rpcError('invalid_argument', 'Confirm turning on the Retinue MCP first.')
      fixtureExternal.enabled = true
      if (typeof params.tier === 'string') fixtureExternal.tier = params.tier
      return changed()
    }
    if (method === 'external.disable') { fixtureExternal.enabled = false; fixtureExternal.allowFullAccess = false; return changed() }
    if (method === 'external.setFullAccess') {
      if (!fixtureExternal.enabled) throw rpcError('conflict', 'Turn the Retinue MCP on first.')
      if (params.allow === true && params.confirm !== true) throw rpcError('invalid_argument', 'Confirm letting outside sessions give employees Full access first.')
      fixtureExternal.allowFullAccess = params.allow === true
      return changed()
    }
    if (method === 'external.setTier') {
      if (!fixtureExternal.enabled) throw rpcError('conflict', 'Turn on the Retinue MCP first.')
      if (typeof params.tier === 'string') fixtureExternal.tier = params.tier
      return changed()
    }
    if (method === 'external.rotate') return changed()
    // 22 November 2026: Connect coding agents. Claude Code is already connected under `?preset=mcp`; Cursor
    // carries a hand-made entry; Qwen Code isn't installed.
    const client = (id: string) => {
      const row = fixtureExternalClients.find((item) => item.id === id)
      if (!row) throw rpcError('invalid_argument', 'Unknown coding agent.')
      return row
    }
    if (method === 'external.clients.list') return { clients: fixtureExternalClients.map((item) => ({ ...item })) } as T
    if (method === 'external.clients.connect') {
      if (!fixtureExternal.enabled) throw rpcError('conflict', 'Turn the Retinue MCP on first.')
      if (params.confirm !== true) throw rpcError('invalid_argument', 'Confirm connecting the coding agent first.')
      const row = client(String(params.id))
      if (!row.installed) throw rpcError('conflict', `${row.name} isn't installed on this computer.`)
      Object.assign(row, { state: 'connected', addedByRetinue: true, error: null })
      notifyFixtureDaemon('external.changed', status())
      return { client: { ...row } } as T
    }
    if (method === 'external.clients.disconnect') {
      const row = client(String(params.id))
      if (row.addedByRetinue) Object.assign(row, { state: 'not_connected', addedByRetinue: false, error: null })
      notifyFixtureDaemon('external.changed', status())
      return { client: { ...row } } as T
    }
  }
  if (method === 'session.handoff.draft') {
    const source = sessions.find((item) => item.id === params.fromSessionId)
    if (!source || source.archived) throw rpcError('not_found', 'Conversation not found.')
    const owner = agents.find((item) => item.id === source.agentId)
    const recent = (source.messages ?? []).filter((message) => message.role === 'user' || message.role === 'assistant').slice(-8)
    const lines = recent.map((message) => `${message.role === 'user' ? 'You' : owner?.name ?? 'Assistant'}: ${String(message.content ?? '').slice(0, 600)}`)
    const title = String(source.title || 'Conversation')
    const note = typeof params.note === 'string' && params.note.trim() ? `\n\n${params.note.trim()}` : ''
    return { draft: `Continue from ${owner?.name ?? `another ${entityNoun(themeStyle())}`}'s conversation "${title}".${note}\n\n${lines.join('\n')}`.slice(0, 4200), carried: { messages: recent.length, truncated: false }, fromAgentId: source.agentId ?? null, title } as T
  }
  if (method === 'session.steer') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session || session.archived) throw rpcError('not_found', 'Conversation not found.')
    if (fixtureSettings['exec_gate.codex.steer'] === false) throw rpcError('unsupported', 'Steering is turned off for this runtime.')
    if (!['working', 'waiting_permission'].includes(String(session.state))) throw rpcError('conflict', 'Nothing is running to steer.')
    const text = typeof params.text === 'string' ? params.text.trim() : ''
    if (!text) throw rpcError('invalid_argument', 'text must contain 1 to 100000 UTF-8 bytes')
    const turnId = String((session.turns ?? []).at(-1)?.id ?? 'turn-preview')
    const id = `steer-${Date.now()}`
    session.messages = [...(session.messages ?? []), { id, role: 'user', content: text, turnId, meta: { kind: 'steer' } }]
    notifyFixtureDaemon('message.completed', { sessionId: session.id, turnId, id, messageId: id, role: 'user', content: text, meta: { kind: 'steer' } })
    return { steered: true, turnId } as T
  }
  if (method === 'attention.focus') return { reported: true } as T
  if (method === 'session.snooze') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session || session.archived) throw rpcError('not_found', 'Conversation not found.')
    if (params.snoozed === true) session.snoozed = true
    else delete session.snoozed
    notifyFixtureDaemon('session.changed', { session: { ...session } })
    return { sessionId: session.id, snoozed: params.snoozed === true } as T
  }
  if (method === 'provider.list') {
    const off = Array.isArray(fixtureSettings['runtimes.disabled']) ? fixtureSettings['runtimes.disabled'] as unknown[] : ['qwen', 'cursor', 'grok', 'antigravity']
    const row = (provider: string, displayName: string, main: boolean, found: boolean, version: string | null, installHint: string | null) => ({ provider, displayName, main, optional: !main, enabled: main || !off.includes(provider), found, runtimeIds: found ? [`runtime-${provider}`] : [], version, installHint })
    return { providers: [
      row('claude', 'Claude Code', true, true, '2.1.322', null),
      row('codex', 'Codex', true, true, '0.159.3', null),
      row('opencode', 'OpenCode', true, true, '2.2.7', null),
      row('qwen', 'Qwen Code', false, false, null, 'Install it from its own site, then scan again.'),
      row('cursor', 'Cursor', false, true, '1.9.0', null),
      row('grok', 'Grok Build', false, false, null, 'Install it from its own site, then scan again.'),
      row('antigravity', 'Antigravity', false, false, null, 'Install Google Antigravity, then scan again.'),
      row('pi', 'Pi', false, true, '1.5.2', null),
    ] } as T
  }
  if (method.startsWith('computer.')) {
    const address = 'https://dashboard.example.com'
    const status = () => !fixtureComputer.linked
      ? { linked: false, enabled: false, address: null, name: null, reachable: false, state: 'unknown', since: null, restarts: 0, cpuPercent: null, memoryPercent: null, memoryText: null, pids: null, diskBytes: null, limits: null, desktopUrl: null, input: null, checkedAt: null, problem: null }
      : { linked: true, enabled: fixtureComputer.enabled, address, name: 'retinue-01', reachable: !fixtureComputer.away, state: fixtureComputer.away ? 'unknown' : fixtureComputer.awake ? 'awake' : 'asleep', since: fixtureComputer.awake ? fixtureComputer.since : null, restarts: 0, cpuPercent: fixtureComputer.awake ? 7 : null, memoryPercent: fixtureComputer.awake ? 31 : null, memoryText: fixtureComputer.awake ? '1.24 GiB / 4 GiB' : null, pids: fixtureComputer.awake ? 188 : null, diskBytes: 2_340_000_000, limits: { cpu: '2', memory: '4g' }, desktopUrl: `${address}/harbor/computer/view/`, input: fixtureComputer.awake && fixtureComputer.enabled && !fixtureComputer.away ? (flags().has('computer-no-input') ? { ready: false, missing: ['xdotool', 'scrot'] } : { ready: true, missing: [] }) : null, checkedAt: new Date().toISOString(), problem: fixtureComputer.away ? 'Your server did not answer in time.' : null }
    const changed = () => { notifyFixtureDaemon('computer.changed', status()); return status() as T }
    if (method === 'computer.status') return status() as T
    if (method === 'computer.link') {
      if (params.confirm !== true) throw rpcError('invalid_argument', 'Confirm linking the computer first.')
      if (typeof params.token !== 'string' || !params.token.startsWith('hrb_')) throw rpcError('invalid_argument', 'Harbor refused that token.')
      Object.assign(fixtureComputer, { linked: true, enabled: false, awake: true, away: false })
      return changed()
    }
    if (method === 'computer.unlink') { Object.assign(fixtureComputer, { linked: false, enabled: false }); return changed() }
    if (method === 'computer.setEnabled') {
      if (params.enabled === true && params.confirm !== true) throw rpcError('invalid_argument', 'Confirm sharing the computer first.')
      fixtureComputer.enabled = params.enabled === true
      return changed()
    }
    if (method === 'computer.action') {
      if (fixtureComputer.away) throw rpcError('unavailable', 'Your server did not answer in time.')
      fixtureComputer.awake = params.action !== 'sleep'
      if (params.action !== 'sleep') fixtureComputer.since = new Date().toISOString()
      return changed()
    }
    if (method === 'computer.routines') return { routines: fixtureRoutines } as T
    if (method === 'computer.routine.run') {
      const routine = fixtureRoutines.find((item) => item.id === params.id)
      if (!routine) throw rpcError('not_found', 'That routine is gone.')
      return { ok: routine.id !== 'rt_2', output: routine.id === 'rt_2' ? 'rsync: connection timed out (110)' : 'Done in 4.2 s.' } as T
    }
    if (method === 'computer.view') return (flags().has('computer-no-view') ? { url: null, reason: 'Your server does not offer its screen to Retinue yet. Update Harbor, or open it in your browser.' } : { url: `${window.location.origin}/src/preview/computerDesktop.html`, reason: null }) as T
    if (method === 'computer.logs') return { logs: '[svc] desktop ready on :3000' } as T
  }
  if (method === 'preview.servers') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'session not found')
    const servers = session.id === SCENE_SESSION_ID && fixtureScene?.servers ? fixtureScene.servers : []
    return { sessionId: session.id, servers } as T
  }
  // Phase 10: importing conversations from Claude Code, Codex and OpenCode.
  if (method === 'import.scan') return { sinceDays: 30, scannedAt: new Date().toISOString(), sources: fixtureImportSources() } as T
  if (method === 'import.preview') {
    const source = String(params.source)
    const rows = fixtureImportRows().filter((row) => row.source === source && !row.alreadyImported)
    if (flags().has('import-seen') && source === 'claude') rows.push(fixtureImportedRow())
    const offset = Number(params.offset ?? 0)
    const limit = Number(params.limit ?? 50)
    return { source, total: rows.length, offset, limit, next: offset + limit < rows.length ? offset + limit : null, rows: rows.slice(offset, offset + limit) } as T
  }
  if (method === 'import.apply') {
    const items = Array.isArray(params.items) ? params.items as Array<{ source: string; conversationId: string }> : []
    for (const [index, item] of items.entries()) {
      window.setTimeout(() => notifyFixtureDaemon('import.progress', { importId: 'preview-import', index: index + 1, total: items.length, source: item.source, conversationId: item.conversationId, state: 'done', importedSessionId: `preview-import-${item.conversationId}` }), 400 * (index + 1))
    }
    const importedSessions = items.map((item) => `preview-import-${item.conversationId}`)
    window.setTimeout(() => notifyFixtureDaemon('import.completed', { importId: 'preview-import', imported: importedSessions.length, skipped: 0, failed: 0, conversationIds: importedSessions }), 400 * (items.length + 1))
    return { importId: 'preview-import', imported: importedSessions.length, skipped: 0, failed: 0, sessions: importedSessions } as T
  }
  if (method === 'import.distribute' && Array.isArray(params.items)) {
    const items = params.items as Array<{ source: string; conversationId: string }>
    const rows = fixtureImportRows()
    return { suggestions: items.map((item) => {
      const row = rows.find((candidate) => candidate.source === item.source && candidate.conversationId === item.conversationId)
      const agentId = item.source === 'codex' ? 'agent-codex' : item.source === 'claude' ? 'agent-claude' : item.source === 'opencode' && !flags().has('import-hire') ? 'agent-opencode' : null
      return { conversationId: item.conversationId, sessionId: null, title: row?.title ?? 'Untitled', source: item.source, agentId, reason: agentId ? 'agent' : 'balance', detail: agentId ? `uses ${row?.agentLabel ?? item.source}, so it can carry on` : 'no employee uses this coding agent; the CEO can file it', hire: agentId ? null : { name: 'OpenCode helper', runtimeId: 'runtime-opencode', presetId: null } }
    }) } as T
  }
  if (method === 'import.distribute') {
    const ids = Array.isArray(params.ids) ? params.ids.map(String) : fixtureImportedIds()
    return { suggestions: ids.map((id, index) => ({ conversationId: id, sessionId: id, title: 'Invoice parser tests', source: 'claude', agentId: index % 2 === 0 ? 'agent-codex' : null, reason: index % 2 === 0 ? 'project' : 'balance', detail: index % 2 === 0 ? 'same folder as Retinue' : 'no bot is free; the CEO can file it' })) } as T
  }
  if (method === 'import.assign') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'session not found')
    return { session: { ...session, agentId: params.agentId ?? null } } as T
  }
  if (method === 'import.projects') {
    const wanted = Array.isArray(params.sources) ? (params.sources as string[]) : ['claude', 'codex', 'opencode']
    const rows = fixtureImportRows().filter((row) => wanted.includes(row.source))
    if (flags().has('import-seen')) rows.push(fixtureImportedRow())
    const byPath = new Map<string, ImportRow[]>()
    for (const row of rows) byPath.set(row.projectPath, [...(byPath.get(row.projectPath) ?? []), row])
    const projects = [...byPath.entries()].map(([path, list]) => ({ key: path.toLowerCase(), path, name: path.split('/').pop() ?? path, exists: !path.includes('Default Project'), existingProjectId: path.endsWith('Retinue') ? 'project-retinue' : null, total: list.length + (path.endsWith('Retinue') ? 3 : 0), shown: list.length, sources: [...new Set(list.map((row) => row.source))], bytes: list.reduce((sum, row) => sum + row.bytes, 0), newest: list.map((row) => row.updatedAt).sort().pop() ?? null, conversations: list }))
    return { projects, older: 4, truncated: false, errors: [], sinceDays: Number(params.sinceDays ?? 30), scannedAt: new Date().toISOString() } as T
  }
  if (method === 'import.continue') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session?.imported) throw rpcError('invalid_argument', 'That conversation was not imported.')
    const agent = agents.find((item) => item.id === params.agentId)
    if (!agent) throw rpcError('invalid_argument', 'That bot no longer exists.')
    const provider = baseRuntimes.find((runtime) => runtime.id === agent.runtimeId)?.provider
    if (provider === session.imported.source && !flags().has('import-gone')) {
      delete session.imported
      session.agentId = agent.id
      notifyFixtureDaemon('session.changed', { session: { ...session } })
      return { mode: 'continue', session: { ...session }, draft: null, importedSessionId: session.id } as T
    }
    const created: Session = { id: `preview-carried-${session.id}`, runtimeId: agent.runtimeId, agentId: agent.id, title: session.title, projectPath: session.projectPath, state: 'idle', messages: [], updatedAt: new Date().toISOString() }
    sessions = [created, ...sessions.filter((item) => item.id !== created.id)]
    notifyFixtureDaemon('session.changed', { session: created })
    return { mode: 'carry_over', session: created, draft: { text: `Picking up "${session.title}" from Codex. The last exchange was about the failing invoice parser tests.\n\nThe full history is the imported conversation ${session.id} (from Codex). Read it with read_conversation before you start.`, importedSessionId: session.id }, importedSessionId: session.id } as T
  }
  // The coding-agent installer (`runtime.install.*`): `?preset=agent-install` runs one to the end,
  // `-progress` holds at 41%, `-failed` fails the checksum and `-damaged` shows a damaged copy.
  if (method === 'runtime.install.plan') return { plan: fixtureInstallPlan(String(params.provider)) } as T
  if (method === 'runtime.install') {
    const plan = fixtureInstallPlan(String(params.provider))
    if (params.confirm !== true) throw rpcError('invalid_argument', 'Confirm the install first: runtime.install needs confirm: true.')
    if (!plan.supported) throw rpcError('provider_unavailable', plan.reason ?? 'This coding agent cannot be installed here.')
    if (params.version !== plan.version) throw rpcError('conflict', 'The plan changed since it was shown. Review it again.')
    fixtureInstallJob = { jobId: `preview-install-${plan.provider}`, provider: plan.provider, mode: plan.mode, startedAt: Date.now(), version: plan.version, bytesTotal: plan.bytes ?? 0, cancelled: false }
    return { jobId: fixtureInstallJob.jobId, provider: plan.provider, mode: plan.mode, version: plan.version } as T
  }
  if (method === 'runtime.install.status') {
    if (!fixtureInstallJob || fixtureInstallJob.jobId !== params.jobId) throw rpcError('not_found', 'That install is not known here.')
    return { job: fixtureInstallJobValue(fixtureInstallJob) } as T
  }
  if (method === 'runtime.install.cancel') {
    if (!fixtureInstallJob || fixtureInstallJob.jobId !== params.jobId) throw rpcError('not_found', 'That install is not known here.')
    fixtureInstallJob.cancelled = true
    return { cancellationRequested: true } as T
  }
  if (method === 'runtime.install.remove') {
    if (params.confirm !== true) throw rpcError('invalid_argument', 'Confirm removing the copy first: runtime.install.remove needs confirm: true.')
    if (!fixtureManagedCopies.has(String(params.provider))) throw rpcError('not_found', 'Retinue did not install this coding agent.')
    fixtureManagedCopies.delete(String(params.provider))
    return { removed: true, provider: params.provider, kept: [] } as T
  }
  if (method === 'session.git.scopes') {
    return { sessionId: params.sessionId, scopes: [
      { scope: 'uncommitted', available: true, reason: null },
      { scope: 'turn', available: true, reason: null, lastTurnId: 'scene-turn' },
      { scope: 'branch', available: true, reason: null, base: { ref: 'main', commit: 'abc1234', kind: 'localDefault', how: 'The repository\'s default branch, main.' } },
      { scope: 'commit', available: true, reason: null },
    ] } as T
  }
  if (method === 'session.limit.resume' || method === 'session.limit.snooze') {
    // Stands in for the per-conversation resume state the daemon does not keep yet.
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'Conversation not found.')
    if (method === 'session.limit.resume') { session.limitAutoResume = params.autoResume === true; if (!session.limitAutoResume) session.limitSnoozed = false }
    else session.limitSnoozed = params.snoozed === true
    notifyFixtureDaemon('session.changed', { session: { ...session } })
    return { ok: true } as T
  }
  if (method === 'turn.retry.cancel') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'Conversation not found.')
    const queue = (fixtureQueues[session.id] ??= { items: [], paused: false })
    const at = queue.items.findIndex((entry) => entry.kind === 'retry')
    if (at < 0) throw rpcError('not_found', 'No automatic retry is pending.')
    queue.items.splice(at, 1)
    notifyFixtureDaemon('queue.changed', { sessionId: session.id, items: queue.items.map((entry) => ({ ...entry })), paused: queue.paused })
    return { cancelled: true } as T
  }
  // `?queue-paused`: two messages wait, paused, in Codex's conversation (22 November 2026).
  if (method === 'session.queue.overview') return { sessions: flags().has('queue-paused') ? [{ sessionId: 'session-codex', count: 2, paused: true }] : [] } as T
  if (typeof method === 'string' && method.startsWith('session.queue.')) {
    // An in-memory stand-in for the daemon's durable queue so the preview can show every state.
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session || session.archived) throw rpcError('not_found', 'Conversation not found.')
    const items = (fixtureQueues[session.id] ??= { items: [], paused: false })
    const publish = () => {
      items.items.forEach((entry, index) => { entry.position = index })
      notifyFixtureDaemon('queue.changed', { sessionId: session.id, items: items.items.map((entry) => ({ ...entry })), paused: items.paused })
    }
    if (session.id === SCENE_SESSION_ID && fixtureScene?.queue && !sceneQueueSeeded) {
      sceneQueueSeeded = true
      items.items = fixtureScene.queue.map((entry, position) => ({ status: 'queued', dueAt: null, error: null, ...entry, sessionId: session.id, position }))
      items.paused = fixtureScene.queuePaused === true
    }
    if (method === 'session.queue.list') {
      if (session.id === SCENE_SESSION_ID && fixtureScene?.resumeOutcome === 'unconfirmed' && !items.paused) throw rpcError('internal', 'Could not read the queue.')
      return { items: items.items.map((entry) => ({ ...entry })), paused: items.paused } as T
    }
    if (method === 'session.queue.add') {
      if (session.link?.kind === 'side') throw rpcError('read_only', 'Side conversations are read-only. Send a new message in the main conversation.')
      const text = typeof params.text === 'string' ? params.text.trim() : ''
      const attachments = Array.isArray(params.attachments) ? params.attachments as Array<{ name: string; path: string }> : []
      if (!text && !attachments.length) throw rpcError('invalid_argument', 'text must contain 1 to 1000000 UTF-8 bytes')
      if (items.items.length >= 20) throw rpcError('conflict', 'The queue is full.')
      const dueAt = typeof params.dueAt === 'string' ? params.dueAt : null
      if (dueAt) {
        const delta = Date.parse(dueAt) - Date.now()
        if (!(delta >= 5_000 && delta <= 366 * 24 * 3_600_000)) throw rpcError('invalid_argument', 'Pick a time between a few seconds and one year from now.')
      }
      const entry = { id: `queue-${Date.now()}-${items.items.length}`, sessionId: session.id, text, attachments, kind: dueAt ? 'send_later' : 'user', dueAt, status: 'queued', error: null, position: items.items.length } as Record<string, unknown>
      items.items.push(entry)
      publish()
      return { item: { ...entry } } as T
    }
    const target = items.items.findIndex((entry) => entry.id === params.id)
    if (method === 'session.queue.delete') { if (target >= 0) items.items.splice(target, 1); publish(); return { ok: true } as T }
    if (method === 'session.queue.update') {
      if (target < 0) throw rpcError('not_found', 'That queued message is gone.')
      if (typeof params.text === 'string' && params.text.trim()) items.items[target].text = params.text.trim()
      publish()
      return { item: { ...items.items[target] } } as T
    }
    if (method === 'session.queue.reorder') {
      const ids = Array.isArray(params.ids) ? params.ids.map(String) : []
      const plain = items.items.filter((entry) => entry.kind === 'user')
      if (ids.length !== plain.length || !ids.every((id) => plain.some((entry) => entry.id === id))) throw rpcError('invalid_argument', 'Reorder must list every queued message once.')
      const rest = items.items.filter((entry) => entry.kind !== 'user')
      items.items = [...ids.map((id) => plain.find((entry) => entry.id === id)!), ...rest]
      publish()
      return { ok: true } as T
    }
    if (method === 'session.queue.send_now') {
      if (target < 0) throw rpcError('not_found', 'That queued message is gone.')
      const [entry] = items.items.splice(target, 1)
      // `steer: true`: the daemon puts a plain message into the running turn and takes it off the queue.
      if (params.steer === true && ['working', 'waiting_permission'].includes(String(session.state))) {
        const canSteer = session.runtimeId === 'runtime-codex' && fixtureScene?.steer !== false && fixtureSettings['exec_gate.codex.steer'] !== false
        const text = String(entry.text ?? '')
        if (canSteer && entry.kind === 'user' && !(Array.isArray(entry.attachments) && entry.attachments.length)) {
          const turnId = String((session.turns ?? []).at(-1)?.id ?? 'turn-preview')
          const id = `steer-${Date.now()}`
          session.messages = [...(session.messages ?? []), { id, role: 'user', content: text, turnId, meta: { kind: 'steer' } }]
          publish()
          notifyFixtureDaemon('message.completed', { sessionId: session.id, turnId, id, messageId: id, role: 'user', content: text, meta: { kind: 'steer' } })
          return { steered: true } as T
        }
        items.items.unshift(entry)
        publish()
        return { steered: false, queued: true, reason: canSteer ? 'Only a plain text message can go into the running turn.' : 'This coding agent cannot take a message into a running turn.' } as T
      }
      items.items.unshift(entry)
      publish()
      return { queued: true } as T
    }
    if (method === 'session.queue.resume') {
      const outcome = session.id === SCENE_SESSION_ID ? fixtureScene?.resumeOutcome : undefined
      if (outcome) await new Promise((resolve) => setTimeout(resolve, 450))
      if (outcome === 'error') throw rpcError('permission_denied', 'Could not resume this queue.')
      items.paused = outcome === 'repaused'
      if (outcome === 'repaused' && items.items[0]) {
        Object.assign(items.items[0], { status: 'failed', error: 'The coding agent could not start the turn.' })
      }
      if (outcome === 'works') {
        const sent = items.items.shift()
        if (sent) {
          session.messages = [...(session.messages ?? []), { id: 'scene-queue-sent', role: 'user', content: `Sent from the queue: ${sent.text}`, createdAt: new Date().toISOString() }]
          notifyFixtureDaemon('session.changed', { session: { ...session } })
        }
      }
      publish()
      return { paused: false } as T
    }
  }
  if (method === 'session.cancel') {
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'Session not found.')
    const turn = fixtureTurns.get(session.id)
    if (!turn && !['working', 'starting', 'waiting_permission', 'cancelling'].includes(String(session.state))) return { cancellationRequested: false, reason: 'no_active_turn' } as T
    finishFixtureTurn(session, 'cancelled', turn?.id ?? session.turnId)
    return { cancellationRequested: true, cascaded: 0 } as T
  }
  if (method === 'session.prompt') {
    // Keep the sent message and its references before emitting the fixture turn.
    const session = sessions.find((item) => item.id === params.sessionId)
    if (!session) throw rpcError('not_found', 'session not found')
    const command = params.command as Record<string, unknown> | null | undefined
    if (command !== undefined && command !== null) {
      const kind = String(command.kind ?? '')
      const name = String(command.name ?? '')
      const owner = agents.find((agent) => agent.id === session.agentId)
      if (kind === 'skill') {
        const skill = fixtureSkills.find((item) => item.name === name)
        const off = !skill || skill.enabled === false || (skill.disabledFor as string[]).includes(String(session.agentId ?? ''))
        if (off) throw rpcError('invalid_argument', `The skill "${name}" is not available to this employee.`)
      } else if (kind === 'manager') {
        if (owner?.systemRole !== 'manager') throw rpcError('invalid_argument', 'That command is only for the Manager.')
      } else throw rpcError('invalid_argument', 'command.kind must be "skill" or "manager"')
    }
    let replySnapshot: NonNullable<MessageMeta['replyTo']> | null = null
    if (params.replyTo !== undefined && params.replyTo !== null) {
      const replyTo = params.replyTo as Record<string, unknown>
      if (replyTo.sourceSessionId !== session.id || typeof replyTo.messageId !== 'string') throw rpcError('invalid_argument', 'Replies must target a message in this writable conversation.')
      const source = (session.messages ?? []).find((message) => message.id === replyTo.messageId && ['user', 'assistant'].includes(String(message.role)))
      if (!source) throw rpcError('not_found', 'The quoted message is no longer available in this conversation.')
      const content = String(source.content ?? source.text ?? '')
      const excerptChars = Array.from(content)
      const truncated = excerptChars.length > 6000
      const author = source.role === 'user' ? 'You' : agents.find((agent) => agent.id === session.agentId)?.name ?? 'Coding agent'
      replySnapshot = { sourceSessionId: session.id, messageId: replyTo.messageId, role: source.role as 'user' | 'assistant', author, excerpt: `${excerptChars.slice(0, 6000).join('')}${truncated ? '…' : ''}`, truncated, createdAt: source.createdAt ?? null }
    }
    const refs = Array.isArray(params.conversationRefs) ? params.conversationRefs.map(String) : []
    const references = refs.map((id) => sessions.find((item) => item.id === id)).filter((item): item is Session => !!item).map((item) => ({ id: item.id, title: String(item.link?.topic ?? item.title ?? 'Conversation') }))
    const next = (session.messages ?? []).reduce((top, message) => Math.max(top, Number(message.sequence) || 0), 0) + 1
    const now = new Date().toISOString()
    if ((fixtureSettings['experimental.autoTitles'] === true || flags().has('experiments')) && session.link?.kind !== 'side' && !(session.messages ?? []).some((m) => m.role === 'user')) {
      const owner = agents.find((a) => a.id === session.agentId)
      if (owner && (session.title === owner.name || session.title === 'New chat')) session.title = String(params.text ?? '').split(/\s+/).slice(0, 6).join(' ').replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, '').slice(0, 48)
    }
    session.messages = [...(session.messages ?? []), { id: `preview-user-${next}`, turnId: `preview-turn-${next}`, sequence: next, role: 'user', content: String(params.text ?? ''), createdAt: now, ...(references.length ? { references } : {}), ...(replySnapshot ? { meta: { replyTo: replySnapshot } } : {}) }]
    const turnId = `preview-turn-${next}`
    startFixtureTurn(session, turnId, { content: references.length ? `Picking up "${references[0].title}" where it left off.` : 'Got it.', sequence: next + 1 })
    return { turnId } as T
  }
  if (method === 'runtime.capabilities') {
    const supported = { supported: true, enabled: true, scope: 'agent', evidence: 'preview' }
    // Like the real daemon, OpenCode keeps effort behind its execution check.
    const thinking = params.runtimeId === 'runtime-opencode' ? { ...supported, enabled: false, reason: 'execution entry gate is disabled' } : supported
    // Steering exists for Codex only, is open by default (FIX39 P13: the gate
    // follows the adapter's capability) and is never proven.
    const steer = params.runtimeId === 'runtime-codex' && fixtureScene?.steer !== false
      ? { supported: true, enabled: fixtureSettings['exec_gate.codex.steer'] !== false, scope: 'turn', evidence: 'none', ...(fixtureSettings['exec_gate.codex.steer'] === false ? { reason: 'execution entry gate is disabled' } : {}) }
      : { supported: false, enabled: false, scope: 'turn', evidence: 'none', reason: 'setting is unsupported by this runtime' }
    return { runtimeId: params.runtimeId, generatesImages: params.runtimeId === 'runtime-codex', settings: { model: supported, thinking, serviceTier: supported, instructions: supported, customEnv: supported, steer } } as T
  }
  if (method === 'media.preview' && typeof params.path === 'string' && /\.(md|markdown)$/i.test(params.path)) {
    // Preview-only: a research report for the document panel (?preset=document).
    const text = ['# nod (n8n labs): Telegram and WhatsApp channels, against Retinue Remote', '', '> **What this is:** a research report on how nod connects to chat apps, compared with Retinue.', '> **Status:** current', '', '## Summary', '', '1. nod talks to chat apps through two official APIs and one unofficial one.', '2. Exactly one person may talk to the bot, by `user id` or phone number.', '3. A pairing code is issued as a deep link; the bot records a *candidate* until the owner approves.', '', '---', '', '## How nod does it', '', '### Telegram', '', 'Long polling against the Bot API. No public URL is needed.', '', '- `getUpdates` with a 30 second timeout', '- an allow list of one user id', '  - stored in the app settings', '', '### WhatsApp', '', 'The Cloud API needs a webhook, so a public URL or relay.', '', '#### Risks', '', 'A stranger can message the number; replies are refused unless paired.', '', '## How Retinue does it now', '', '| Path | Reach | Pairing |', '| --- | --- | --- |', '| Tailscale | tailnet only | code + desktop approval |', '| Native | internet, TLS | password + approval |', '', '```ts', "const tier = 'chat'", '```', '', '## Open questions', '', 'Should a chat channel be its own device kind?'].join(String.fromCharCode(10))
    return { status: 'ok', name: params.path.split('/').pop(), relativePath: params.path, kind: 'text', mediaType: 'text/markdown', size: text.length, text, truncated: false, language: 'md' } as unknown as T
  }
  if (method === 'media.preview' && typeof params.mediaId === 'string') {
    const canvas = document.createElement('canvas')
    canvas.width = 640
    canvas.height = 400
    const paint = canvas.getContext('2d')!
    const sky = paint.createLinearGradient(0, 0, 0, 400)
    sky.addColorStop(0, '#2b3a67')
    sky.addColorStop(1, '#f6a65a')
    paint.fillStyle = sky
    paint.fillRect(0, 0, 640, 400)
    paint.fillStyle = '#ffe9b0'
    paint.beginPath()
    paint.arc(320, 300, 70, 0, Math.PI * 2)
    paint.fill()
    return { status: 'ok', reason: null, name: 'sunrise.png', relativePath: null, kind: 'image', mediaType: 'image/png', size: 182_400, dataUrl: canvas.toDataURL('image/png') } as T
  }
  if (method === 'media.reveal') return { ok: true } as T
  if (method === 'runtime.models') {
    const runtimeId = String(params.runtimeId ?? '')
    return fixtureCatalog(runtimeId) as T
  }
  if (method === 'agent.list') {
    const includeArchived = params.includeArchived === true
    const runtimeId = typeof params.runtimeId === 'string' ? params.runtimeId : undefined
    const list = sortAgents(agents.filter((agent) => (includeArchived || !agent.archived) && (!runtimeId || agent.runtimeId === runtimeId)))
    return { agents: list } as T
  }
  if (method === 'agent.get') {
    const agent = agents.find((item) => item.id === params.agentId)
    if (!agent) throw rpcError('not_found', `That ${entityNoun(themeStyle())} is no longer available.`)
    return { agent: { ...agent } } as T
  }
  if (method === 'manager.get') {
    return { manager: agents.find((agent) => agent.systemRole === 'manager') ?? null } as T
  }
  if (method === 'manager.ensure') {
    const existing = agents.find((agent) => agent.systemRole === 'manager')
    if (existing) return { manager: { ...existing }, created: false } as T
    const runtimeId = typeof params.runtimeId === 'string' ? params.runtimeId : ''
    if (!baseRuntimes.some((runtime) => runtime.id === runtimeId)) throw rpcError('not_found', 'That runtime is no longer available.')
    const now = new Date().toISOString()
    const name = typeof params.name === 'string' && params.name.trim() ? params.name.trim() : 'Manager'
    const manager: Agent = {
      id: `agent-manager-${agentSerial++}`,
      name,
      description: managerDescription,
      instructions: managerInstructions,
      color: 'mint',
      runtimeId,
      model: typeof params.model === 'string' ? params.model : null,
      thinking: typeof params.thinking === 'string' ? params.thinking : null,
      serviceTier: null,
      customArgs: [],
      customEnv: {},
      maxConcurrency: 1,
      defaultProject: null,
      sortOrder: agents.filter((agent) => agent.runtimeId === runtimeId && !agent.archived).reduce((max, agent) => Math.max(max, agent.sortOrder), -1) + 1,
      archived: false,
      createdAt: now,
      updatedAt: now,
      approvalMode: 'ask',
      effectiveApprovalMode: 'ask',
      projectId: null,
      role: 'Manager',
      leader: false,
      hidden: false,
      systemRole: 'manager',
    }
    agents = sortAgents([...agents, manager])
    sequence += 1
    return { manager, created: true } as T
  }
  if (method === 'manager.profile.list') {
    return { facts: fixtureManagerFacts.map((fact) => ({ ...fact })) } as T
  }
  if (method === 'manager.profile.set') {
    const category = typeof params.category === 'string' ? params.category : ''
    const key = typeof params.key === 'string' ? params.key.trim() : ''
    const value = typeof params.value === 'string' ? params.value.trim() : ''
    if (!['identity', 'workflow', 'style', 'conventions', 'design', 'other'].includes(category)) throw rpcError('invalid_argument', 'category must be identity, workflow, style, conventions, design or other.')
    if (!key || key.length > 80) throw rpcError('invalid_argument', 'key is required, up to 80 characters.')
    if (!value || value.length > 1000) throw rpcError('invalid_argument', 'value is required, up to 1000 characters.')
    if (/(sk-[A-Za-z0-9]{8,}|ghp_|AKIA|-----BEGIN|bearer\s)/i.test(value)) throw rpcError('invalid_argument', 'That looks like a credential; the profile must never store secrets.')
    const now = new Date().toISOString()
    const existing = fixtureManagerFacts.find((item) => item.category === category && item.key === key)
    if (existing) {
      existing.value = value
      existing.source = 'corrected'
      if (typeof params.locked === 'boolean') existing.locked = params.locked
      existing.updatedAt = now
      notifyFixtureDaemon('manager.profile.changed', { action: 'updated', fact: { ...existing } })
      return { fact: { ...existing } } as T
    }
    const fact = { id: `fact-${++agentSerial}`, category, key, value, source: 'said', locked: params.locked === true, createdAt: now, updatedAt: now }
    fixtureManagerFacts.push(fact)
    notifyFixtureDaemon('manager.profile.changed', { action: 'added', fact: { ...fact } })
    return { fact: { ...fact } } as T
  }
  if (method === 'manager.profile.delete') {
    const index = fixtureManagerFacts.findIndex((item) => item.id === params.id)
    if (index < 0) throw rpcError('not_found', 'That fact is gone.')
    const [removed] = fixtureManagerFacts.splice(index, 1)
    notifyFixtureDaemon('manager.profile.changed', { action: 'deleted', id: removed.id })
    return { deleted: true } as T
  }
  if (method === 'manager.playbook.get') {
    return { playbook: { ...fixturePlaybookVersions[fixturePlaybookVersions.length - 1] } } as T
  }
  if (method === 'manager.playbook.history') {
    return { versions: [...fixturePlaybookVersions].reverse().map((version) => ({ ...version })) } as T
  }
  if (method === 'manager.playbook.set') {
    const text = typeof params.text === 'string' ? params.text.trim() : ''
    if (!text || text.length > 16000) throw rpcError('invalid_argument', 'The playbook text must be 1 to 16000 characters.')
    const version = { version: fixturePlaybookVersions.length + 1, text, author: 'user', createdAt: new Date().toISOString() }
    fixturePlaybookVersions.push(version)
    notifyFixtureDaemon('manager.playbook.changed', { version: { ...version } })
    return { playbook: { ...version } } as T
  }
  if (method === 'manager.playbook.revert') {
    const target = fixturePlaybookVersions.find((version) => version.version === params.version)
    if (!target) throw rpcError('not_found', 'No such playbook version.')
    const version = { version: fixturePlaybookVersions.length + 1, text: target.text, author: 'user', createdAt: new Date().toISOString() }
    fixturePlaybookVersions.push(version)
    notifyFixtureDaemon('manager.playbook.changed', { version: { ...version } })
    return { playbook: { ...version } } as T
  }
  if (method === 'agent.create') {
    const name = typeof params.name === 'string' ? params.name.trim() : ''
    const runtimeId = typeof params.runtimeId === 'string' ? params.runtimeId : ''
    if (!name || !runtimeId) throw rpcError('invalid_argument', 'Check the name, description, colour, and runtime, then try again.')
    if (!baseRuntimes.some((runtime) => runtime.id === runtimeId)) throw rpcError('not_found', 'That runtime is no longer available.')
    if (nameTaken(name)) throw rpcError('conflict', `Another ${entityNoun(themeStyle())} already uses this name.`)
    const sortOrder = agents.filter((agent) => agent.runtimeId === runtimeId && !agent.archived).reduce((max, agent) => Math.max(max, agent.sortOrder), -1) + 1
    const now = new Date().toISOString()
    const agent: Agent = {
      id: `agent-new-${agentSerial++}`,
      name,
      description: typeof params.description === 'string' ? params.description : '',
      instructions: typeof params.instructions === 'string' ? params.instructions : '',
      color: typeof params.color === 'string' ? params.color : 'mint',
      runtimeId,
      model: typeof params.model === 'string' || params.model === null ? params.model as string | null : null,
      thinking: typeof params.thinking === 'string' || params.thinking === null ? params.thinking as string | null : null,
      serviceTier: typeof params.serviceTier === 'string' || params.serviceTier === null ? params.serviceTier as string | null : null,
      customArgs: Array.isArray(params.customArgs) ? params.customArgs.filter((item): item is string => typeof item === 'string') : [],
      customEnv: params.customEnv && typeof params.customEnv === 'object' && !Array.isArray(params.customEnv) ? params.customEnv as Record<string, string> : {},
      maxConcurrency: typeof params.maxConcurrency === 'number' ? params.maxConcurrency : 1,
      defaultProject: typeof params.defaultProject === 'string' ? params.defaultProject : null,
      sortOrder,
      archived: false,
      createdAt: now,
      updatedAt: now,
    }
    agents = sortAgents([...agents, agent])
    sequence += 1
    return { agent } as T
  }
  if (method === 'agent.update') {
    const agentId = typeof params.agentId === 'string' ? params.agentId : ''
    const existing = agents.find((agent) => agent.id === agentId)
    if (!existing) throw rpcError('not_found', `That ${entityNoun(themeStyle())} is no longer available.`)
    if (existing.archived) throw rpcError('conflict', `This ${entityNoun(themeStyle())} is archived and cannot be edited.`)
    if (typeof params.name === 'string' && nameTaken(params.name, existing.id)) throw rpcError('conflict', `Another ${entityNoun(themeStyle())} already uses this name.`)
    const nextRuntime = typeof params.runtimeId === 'string' ? params.runtimeId : existing.runtimeId
    if (!baseRuntimes.some((runtime) => runtime.id === nextRuntime)) throw rpcError('not_found', 'That runtime is no longer available.')
    const updated: Agent = {
      ...existing,
      name: typeof params.name === 'string' ? params.name.trim() : existing.name,
      description: typeof params.description === 'string' ? params.description : existing.description,
      instructions: typeof params.instructions === 'string' ? params.instructions : existing.instructions,
      color: typeof params.color === 'string' ? params.color : existing.color,
      computerWorkplace: typeof params.computerWorkplace === 'boolean' ? params.computerWorkplace : existing.computerWorkplace,
      replyIcons: typeof params.replyIcons === 'boolean' ? params.replyIcons : existing.replyIcons,
      // The look the editor saved (icon, art, profile photo); the real daemon keeps it too.
      ...(params.look && typeof params.look === 'object' && !Array.isArray(params.look) ? { look: params.look as Agent['look'] } : {}),
      runtimeId: nextRuntime,
      defaultProject: Object.prototype.hasOwnProperty.call(params, 'defaultProject') ? (typeof params.defaultProject === 'string' ? params.defaultProject : null) : existing.defaultProject,
      updatedAt: new Date().toISOString(),
    }
    agents = sortAgents(agents.map((agent) => agent.id === updated.id ? updated : agent))
    sequence += 1
    return { agent: updated } as T
  }
  if (method === 'agent.delete') {
    const agentId = typeof params.agentId === 'string' ? params.agentId : ''
    const existing = agents.find((agent) => agent.id === agentId)
    if (!existing) throw rpcError('not_found', `That ${entityNoun(themeStyle())} is no longer available.`)
    if (existing.systemRole === 'manager') throw rpcError('conflict', "The Manager can't be removed; you can rename it or change its model")
    const archived = { ...existing, archived: true, updatedAt: new Date().toISOString() }
    agents = sortAgents(agents.map((agent) => agent.id === archived.id ? archived : agent))
    sequence += 1
    return { agent: archived } as T
  }
  if (method === 'session.new') {
    const agentId = typeof params.agentId === 'string' ? params.agentId : undefined
    const runtimeId = typeof params.runtimeId === 'string' ? params.runtimeId : undefined
    if (!!agentId === !!runtimeId) throw rpcError('invalid_argument', 'Choose a project folder that exists on this device.')
    let resolvedRuntime = runtimeId
    let owner: string | null = null
    if (agentId) {
      const agent = agents.find((item) => item.id === agentId)
      if (!agent) throw rpcError('not_found', `That ${entityNoun(themeStyle())} is no longer available.`)
      if (agent.archived) throw rpcError('conflict', `This ${entityNoun(themeStyle())} is archived, so a new session cannot be started.`)
      resolvedRuntime = agent.runtimeId
      owner = agent.id
    }
    if (!resolvedRuntime || !baseRuntimes.some((runtime) => runtime.id === resolvedRuntime)) throw rpcError('not_found', `That ${entityNoun(themeStyle())} is no longer available.`)
    if (typeof params.projectPath !== 'string' || !params.projectPath.trim()) throw rpcError('invalid_argument', 'Choose a project folder that exists on this device.')
    const title = typeof params.title === 'string' && params.title.trim() ? params.title : 'New chat'
    const agent = owner ? agents.find((item) => item.id === owner) : undefined
    const catalog = fixtureCatalog(resolvedRuntime)
    const model = agent?.model ?? catalog.models.find((item) => item.isDefault === true)?.id ?? null
    const selected = catalog.models.find((item) => item.id === model)
    const thinking = agent?.thinking ?? selected?.defaultThinking ?? null
    const session: Session = { id: `session-new-${sessions.length + 1}`, runtimeId: resolvedRuntime, agentId: owner, title, projectPath: params.projectPath, state: 'idle', ...(model ? { model } : {}), modelLock:{ model, thinking }, contextUsed:null, contextSize:null, updatedAt: new Date().toISOString(), messages: [] }
    sessions = [...sessions, session]
    sequence += 1
    return { session } as T
  }
  if (method === 'agent.conversation' || method === 'agent.conversation.create') {
    const agent = agents.find((item) => item.id === params.agentId && !item.archived)
    if (!agent) throw rpcError('not_found', `That ${entityNoun(themeStyle())} is no longer available.`)
    const existing = method === 'agent.conversation' ? sessions.filter((session) => session.agentId === agent.id && !session.archived && !session.link).sort((a, b) => String(b.updatedAt).localeCompare(String(a.updatedAt)))[0] : undefined
    if (existing) return { session: { ...existing }, created: false } as T
    // Phase 1A: a conversation picks its project (an id, null for none); otherwise the bot's last, then default, project.
    const wanted = params.projectId === null ? null : typeof params.projectId === 'string' ? params.projectId : (agent.lastProjectId ?? agent.projectId ?? null)
    const project = wanted ? projects.find((item) => item.id === wanted) : undefined
    if (wanted && !project) throw rpcError('not_found', 'That project no longer exists.')
    if (project && !project.path) throw rpcError('invalid_argument', 'That project has no folder on this computer.')
    const title = typeof params.title === 'string' && params.title.trim() ? params.title.trim() : 'New chat'
    // Like the daemon, a lazily created conversation is stored untitled and named
    // from the first message, never with the bot's own name.
    const session: Session = { id: `session-new-${sessions.length + 1}`, runtimeId: agent.runtimeId, agentId: agent.id, title, projectPath: project?.path ?? `C:/Retinue/workspaces/${agent.id}`, projectId: project?.id ?? null, projectName: project?.name ?? null, state: 'idle', modelLock: { model: agent.model, thinking: agent.thinking }, contextUsed: null, contextSize: null, updatedAt: new Date().toISOString(), messages: [] }
    sessions = [...sessions, session]
    return { session, created: true } as T
  }
  // 8 November 2026: hiring (docs/plans/hiring-brief.md), kept in memory.
  if (method === 'preset.list') {
    return { departments: DEPARTMENTS, presets: PRESETS.map(({ instructions: _instructions, ...preset }) => ({ ...preset, hiredAgentId: agents.find((agent) => agent.presetId === preset.id && !agent.archived)?.id ?? null })) } as T
  }
  if (method === 'project.members') {
    if (!projects.some((project) => project.id === params.projectId)) throw rpcError('not_found', 'That project is no longer available.')
    return { members: fixtureMembers(String(params.projectId)) } as T
  }
  if (method === 'project.hire') {
    const projectId = String(params.projectId ?? '')
    if (!projects.some((project) => project.id === projectId)) throw rpcError('not_found', 'That project is no longer available.')
    let agentId = typeof params.agentId === 'string' ? params.agentId : null
    let created = false
    if (!agentId) {
      const preset = PRESETS.find((item) => item.id === params.presetId)
      if (!preset) throw rpcError('invalid_argument', 'Name an employee or a preset.')
      const existing = agents.find((agent) => agent.presetId === preset.id && !agent.archived)
      if (existing) agentId = existing.id
      else {
        const base = agents[0]
        const taken = (name: string) => agents.some((agent) => agent.name.toLowerCase() === name.toLowerCase())
        let name = preset.nickname
        for (let n = 2; taken(name); n += 1) name = `${preset.nickname} ${n}`
        const agent = { ...base, id: `agent-${preset.id}`, name, role: preset.role, description: preset.description, instructions: preset.instructions.join(String.fromCharCode(10)), color: preset.color, tags: preset.tags, presetId: preset.id, systemRole: null, projectId: null, archived: false, look: { shape: 'circle', icon: preset.portrait, photo: preset.photo }, runtimeId: typeof params.runtimeId === 'string' ? params.runtimeId : base?.runtimeId, updatedAt: new Date().toISOString() } as Agent
        agents = [...agents, agent]
        agentId = agent.id
        created = true
      }
    }
    if (!fixtureHires.some((item) => item.projectId === projectId && item.agentId === agentId)) fixtureHires.push({ projectId, agentId: agentId!, hiredAt: Date.now() })
    return { agentId, created, member: { agentId, hiredAt: Date.now(), sortOrder: 0 } } as T
  }
  if (method === 'project.release') {
    const before = fixtureHires.length
    fixtureHires = fixtureHires.filter((item) => !(item.projectId === params.projectId && item.agentId === params.agentId))
    fixtureReleased.add(`${params.projectId}:${params.agentId}`)
    return { released: fixtureHires.length < before } as T
  }
  if (method === 'agent.team.update') {
    const agent = agents.find((item) => item.id === params.agentId)
    if (!agent) throw rpcError('not_found', `That ${entityNoun(themeStyle())} is no longer available.`)
    if (agent.systemRole === 'manager' && typeof params.projectId === 'string') throw rpcError('conflict', 'The Manager works across every project')
    if (agent.systemRole === 'manager' && 'leader' in params) throw rpcError('conflict', "The Manager can't be removed; you can rename it or change its model")
    if (params.leader === true) agents = agents.map((item) => item.id === agent.id ? item : { ...item, leader: false })
    const patch: Partial<Agent> = {}
    if ('projectId' in params) patch.projectId = typeof params.projectId === 'string' ? params.projectId : null
    if (typeof params.role === 'string') patch.role = params.role.trim()
    if (typeof params.leader === 'boolean') patch.leader = params.leader
    if (typeof params.hidden === 'boolean') patch.hidden = params.hidden
    agents = agents.map((item) => item.id === agent.id ? { ...item, ...patch, updatedAt: new Date().toISOString() } : item)
    return { agent: { ...agents.find((item) => item.id === agent.id)! } } as T
  }
  if (method === 'project.create') {
    const project: Project = { id: `project-${projects.length + 1}`, name: String(params.name ?? 'Project'), path: typeof params.path === 'string' ? params.path : null, sortOrder: projects.length, collapsed: false }
    projects = [...projects, project]
    return { project } as T
  }
  if (method === 'project.update') {
    if ((params.overrides as { approvalMode?: unknown } | undefined)?.approvalMode === 'bypass' && params.confirm !== true) throw rpcError('invalid_argument', 'Confirm Full access for this project first.')
    if (params.overrides) projects = projects.map((project) => project.id === params.projectId ? { ...project, overrides: params.overrides as Project['overrides'] } : project)
    projects = projects.map((project) => project.id === params.projectId ? { ...project, ...(typeof params.name === 'string' ? { name: params.name } : {}), ...(typeof params.collapsed === 'boolean' ? { collapsed: params.collapsed } : {}), ...('path' in params ? { path: typeof params.path === 'string' ? params.path : null } : {}) } : project)
    return { project: projects.find((project) => project.id === params.projectId) } as T
  }
  if (method === 'project.icon.set') {
    projects = projects.map((project) => project.id === params.projectId ? { ...project, overrides: { ...project.overrides, icon: (params.icon ?? undefined) as NonNullable<Project['overrides']>['icon'] } } : project)
    return { project: projects.find((project) => project.id === params.projectId) } as T
  }
  if (method === 'project.favicon') {
    // `?favicon`: the preview's projects each have a favicon in their folder.
    if (!flags().has('favicon')) return { found: false } as T
    return { found: true, path: 'public/favicon.svg', mime: 'image/svg+xml', dataBase64: btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="16" height="16" rx="4" fill="#3d9bff"/><path d="M4 11V5l4 3 4-3v6" fill="none" stroke="#fff" stroke-width="1.6"/></svg>') } as T
  }
  if (method === 'project.delete') {
    projects = projects.filter((project) => project.id !== params.projectId)
    agents = agents.map((agent) => agent.projectId === params.projectId ? { ...agent, projectId: null } : agent)
    return { deleted: true } as T
  }
  if (method === 'permission.reply' && typeof params.permissionId === 'string' && params.permissionId !== 'perm-question') {
    const permission = currentSnapshot().permissions?.find((item) => item.id === params.permissionId)
    if (!permission || !permission.choices?.includes(String(params.choice))) throw rpcError('invalid_argument', 'That approval choice is unavailable.')
    const session = sessions.find((item) => item.id === permission.sessionId)
    if (session && livePermissions.has(permission.id)) {
      if (params.choice === 'deny') finishFixtureTurn(session, 'permission_denied')
      else {
        resolveFixturePermission(permission, String(params.choice))
        streamFixtureReply(session)
      }
    } else {
      resolveFixturePermission(permission, String(params.choice))
      if (session && session.state === 'waiting_permission') finishFixtureTurn(session, params.choice === 'deny' ? 'permission_denied' : 'completed')
    }
    return { resolved: true } as T
  }
  if (method === 'permission.reply' && params.permissionId === 'perm-question') {
    sessions = sessions.map((session) => session.id === teamMainId ? { ...session, state: 'idle' } : session)
    notifyFixtureDaemon('permission.resolved', { permissionId: 'perm-question' })
    return { resolved: true } as T
  }
  if (method === 'proposal.decide' && params.id === 'perm-proposal') {
    const decision = params.decision === 'approve' ? 'approved' : 'rejected'
    fixtureProposalPending = false
    notifyFixtureDaemon('permission.resolved', { permissionId: 'perm-proposal', choice: params.decision === 'approve' ? 'approve' : 'reject' })
    notifyFixtureDaemon('proposal.changed', { id: 'perm-proposal', kind: 'bot', sessionId: teamMainId, status: decision })
    return { id: 'perm-proposal', decision } as T
  }
  if (method === 'usage.analytics') return answerUsageAnalytics(params, flags().get('analytics')) as T
  recordUnknownRpc(method)
  return {} as T
}
let experimentalFixtureSeeded = false
function seedExperimentalFixtures() {
  if (!flags().has('experiments') || experimentalFixtureSeeded) return
  experimentalFixtureSeeded = true
  for (const session of sessions) {
    if (session.archived || session.link?.kind === 'side') continue
    let turnId: string | null = null
    for (const message of session.messages ?? []) {
      if (message.role === 'user') turnId = String(message.turnId ?? `fixture-turn-${session.id}-${message.id}`)
      if (message.role === 'user' || message.role === 'assistant') message.turnId ??= turnId ?? undefined
    }
    const reply = (session.messages ?? []).filter((m) => m.role === 'assistant' && m.turnId).at(-1)
    if (reply && !(session.files?.length)) session.files = [{
      id: `fixture-review-${session.id}`, turnId: reply.turnId, path: 'docs/example.md',
      detail: { path: 'docs/example.md', change: 'update', diff: '--- a/docs/example.md\n+++ b/docs/example.md\n@@ -1 +1 @@\n-Old note\n+Updated note\n' },
    }]
  }
}

export async function fetchSnapshot() {
  ensureRemoteFixture()
  seedExperimentalFixtures()
  if (fixturePreset() === 'opencode-runtime-error') setPreviewSelection('agent-opencode')
  else if (fixturePreset() === 'claude-context') setPreviewSelection('agent-claude')
  else if (fixturePreset() === 'usage-tip-90') {
    setPreviewSelection('agent-invoice')
    for (const session of sessions.filter((item) => item.agentId === 'agent-invoice')) session.state = 'idle'
  }
  if (flags().has('demo-execution')) {
    const recorded = sessions.find((session) => session.id === 'session-codex')
    if (recorded) recorded.updatedAt = new Date().toISOString()
  }
  const snapshot = currentSnapshot()
  snapshot.remote = await remoteSimulator.call('remote.status', {}) as RemoteStatus
  return snapshot
}
export async function ensureDaemon() {}
export async function startDaemonEventStream() {}
const supervisorListeners = new Set<(state: DaemonSupervisor) => void>()
const PREVIEW_SUPERVISOR: DaemonSupervisor = { state: 'running', attempt: 0, nextRetryMs: null, lastExitCode: null }
let fixtureSupervisor = PREVIEW_SUPERVISOR
export async function listenForDaemonSupervisor(handler: (state: DaemonSupervisor) => void): Promise<Unlisten> {
  supervisorListeners.add(handler)
  return () => { supervisorListeners.delete(handler) }
}
export async function daemonSupervisorState(): Promise<DaemonSupervisor> { return fixtureSupervisor }
export async function restartDaemonNow(): Promise<DaemonSupervisor> {
  setFixtureConnection(false, 'restarting')
  window.setTimeout(() => setFixtureConnection(true, 'running'), 1100)
  return fixtureSupervisor
}
export async function openProjectFolder() { return flags().has('demo-project-folder') ? 'C:/work' : null }
export async function selectLocalFile() { return null }
let stagedAttachmentSerial = 0
export async function stagePromptAttachment(bytes: Uint8Array) {
  if (flags().get('demo-clipboard') === 'stage') throw new Error('The image could not be read.')
  if (!bytes.length) throw new Error('The image could not be read.')
  stagedAttachmentSerial += 1
  return `C:/preview/attachments/staged-${stagedAttachmentSerial}.png`
}
export async function readClipboardImage() {
  const fault = flags().get('demo-clipboard')
  if (fault === 'empty') throw new Error('The clipboard does not contain a readable image.')
  const failures: Record<string, string> = clipboardReadFailures
  const message = fault && failures[fault]
  if (message) throw new Error(message)
  const response = await fetch('/src/assets/brand/retinue-tile-dark.png')
  if (!response.ok) throw new Error('The preview image is unavailable.')
  return new Uint8Array(await response.arrayBuffer())
}
export async function selectMarkdownExportPath() { return 'preview-markdown-token' }
export async function writeMarkdownExport(_selectionToken: string, _text: string) {}
export async function selectBlobExportPath(_name: string) { return 'preview-blob-export-token' }
export async function selectBlobImportPath() { return 'preview-blob-import-token' }
export async function readBlobImport(_selectionToken: string) {
  return JSON.stringify({
    format: 'retinue.blob.v1', name: 'Mochi', description: 'Reviews pull requests and keeps the test suite green.', colour: '#f6a04d',
    instructions: 'Read the diff first. Run the tests before you say something works. Keep reviews short and kind.',
    model: 'gpt-6.1-sol', thinking: 'low', speed: null, look: { shape: 'pebble', seed: 'mochi' }, defaultApprovalMode: 'ask', providerId: 'codex',
  }, null, 2)
}
export async function writeBlobExport(_selectionToken: string, _text: string) {}
// Preview stand-ins for the backup types `src/tauri.ts` exports, so the backup
// page type-checks against the fixture bridge unchanged.
export type BackupInclude = { memory?: boolean; skills?: boolean; automations?: boolean; settings?: boolean }
export type BackupExportRequest = BackupInclude & { agentIds?: string[] }
export type BackupStats = { bots: number; memory: number; skills: number; automations: number; settings: number }
export async function selectBackupExportPath(_suggestedName?: string) {
  if (flags().get('demo-backup') === 'save-dialog') throw { code: 'internal' }
  return 'preview-backup-export-token'
}
export async function selectBackupImportPath() {
  if (flags().get('demo-backup') === 'open-dialog') throw { code: 'internal' }
  return 'preview-backup-import-token'
}
export type { DiagnosticsPreview } from '../tauri'
const diagnosticsPreview: DiagnosticsPreview = {
  generatedAt: '2026-10-18T12:00:00Z', fileName: 'retinue-diagnostics-2026-10-18.zip',
  files: [
    { name: 'README.txt', sizeBytes: 900 }, { name: 'environment.json', sizeBytes: 640 },
    { name: 'settings.json', sizeBytes: 1_240 }, { name: 'events.json', sizeBytes: 12_800 },
    { name: 'logs/daemon.2026-10-18.log', sizeBytes: 33_932 },
  ],
}
export async function selectDiagnosticsExportPath() {
  if (flags().get('demo-diagnostics') === 'dialog-error') throw { code: 'internal' }
  return 'preview-diagnostics-token'
}
export async function writeDiagnosticsExport(_selectionToken: string): Promise<{ path: string; sizeBytes: number; files: { name: string; sizeBytes: number }[] }> {
  if (flags().get('demo-diagnostics') === 'write-error') throw { code: 'internal' }
  return { path: `C:/Users/you/Documents/${diagnosticsPreview.fileName}`, sizeBytes: 48_512, files: diagnosticsPreview.files }
}
export async function writeBackupExport(_selectionToken: string, _include?: BackupExportRequest) {
  if (flags().get('demo-backup') === 'write') throw { code: 'internal' }
  return { path: 'C:\\Users\\you\\Documents\\retinue-backup-2026-10-17.retinue-backup.json', stats: { bots: agents.length, memory: fixtureMemory.length, skills: fixtureSkills.length, automations: 1, settings: 9 } }
}
export async function previewBackupImport(_selectionToken: string) {
  if (flags().get('demo-backup') === 'read') throw { code: 'invalid_argument' }
  return {
    previewId: 'preview-backup',
    counts: { bots: 3, memory: 14, skills: 2, automations: 1, settings: 9 },
    conflicts: {
      bots: [{ exportId: 'export-1', name: String(agents[0]?.name ?? 'Mochi'), existingAgentId: String(agents[0]?.id ?? 'agent-1') }],
      skills: [{ name: 'pdf', same: false }, { name: 'release-notes', same: true }],
      memory: [],
      settings: [{ key: 'appearance.theme', current: 'dark', incoming: 'light' }],
    },
    warnings: ['This backup contains a leader, but a leader is already set; the restored employee will not lead.', 'One employee used Bypass approvals; it is restored as Ask.'],
  }
}
export async function inspectLocalFile(path: string) { return { path, fileName: path.split(/[\\/]/).pop() ?? path, sizeBytes: 2048 } }
export async function openInEditor() {}
export async function saveGeneratedImage(_sessionId: string, _mediaId: string) { return true }
export async function revealInExplorer() {}
export async function resolveProjectFile(path: string) { return path }
// The preview opens no window; the shape is what the sign-in sheet expects.
export async function runtimeTerminal(_provider: string, _action: string, _accountId?: string | null) { return { launched: true, terminal: 'wt' as const } }
const harnessPreviewVersions: Record<string, string> = { claude: '2.1.303', codex: '0.159.3', opencode: '1.18.34' }
export async function harnessLatestVersion(provider: string) {
  if (!fixturePreset()?.startsWith('harness-update')) throw new Error('No preview update is available.')
  return harnessPreviewVersions[provider] ?? ''
}
export async function harnessUpdate(provider: string) {
  if (!fixturePreset()?.startsWith('harness-update')) throw new Error('Harness updates are unavailable in this preview.')
  if (fixturePreset() === 'harness-update-progress') await new Promise((resolve) => setTimeout(resolve, 30_000))
  return { provider, version: harnessPreviewVersions[provider] ?? '2.1.303' }
}
export async function showMainWindow() {}
export async function showMainSettings(_sessionId?: string | null, _page?: string) {}
export async function setActiveSession() {}
export async function getActiveSession() { return composerPreviewSessionId }
export async function setActiveRuntime() {}
export async function getActiveRuntime() { return 'runtime-codex' }
export async function setCompanionVisibility() {}
export async function setCloseToTray() {}
export async function companionMonitorOptions() { return [] as string[] }
export async function currentCompanionMonitor() { return null }
export async function setCompanionMonitor() {}
export async function refreshTrayMenu() {}
const companionSizes: Record<string, [number, number]> = { petit: [344, 62], hidden: [344, 62], welcome: [640, 160], home: [640, 420], 'home-chat': [640, 420] }
/** The preview has no real window: the expanded size lives here, and the resize handles change it. */
let fixtureCompanionSize: [number, number] = [640, 420]
let fixtureCompanionMode = 'petit'
let fixtureCompanionDrag: { edge: string; x: number; y: number; size: [number, number] } | null = null
let fixturePointer: [number, number] = [0, 0]
if (typeof window !== 'undefined') for (const type of ['pointerdown', 'pointermove']) window.addEventListener(type, (event) => { fixturePointer = [(event as PointerEvent).screenX, (event as PointerEvent).screenY] }, true)
function paintFixtureCompanion() {
  const [width, height] = fixtureCompanionMode === 'petit' || fixtureCompanionMode === 'hidden' ? [344, 62] : fixtureCompanionSize
  let style = document.getElementById('fixture-companion-size')
  if (!style) { style = document.createElement('style'); style.id = 'fixture-companion-size'; document.head.append(style) }
  style.textContent = `:root.companion-surface body { background: radial-gradient(900px 500px at 70% 10%, #3b2f63, #12131a) !important; }
    .companion-root { position: fixed; left: 50%; bottom: 40px; transform: translateX(-50%); width: ${width}px !important; height: ${height}px !important; }`
}
export type CompanionEdge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw'
export async function companionResizeBegin(edge: CompanionEdge) { fixtureCompanionDrag = { edge, x: fixturePointer[0], y: fixturePointer[1], size: [...fixtureCompanionSize] } }
export async function companionResizeStep() {
  const drag = fixtureCompanionDrag
  if (!drag) return
  // The preview window grows from its bottom centre, so a side moves twice as far as the pointer.
  const dx = (fixturePointer[0] - drag.x) * (drag.edge.includes('w') ? -2 : drag.edge.includes('e') ? 2 : 0)
  const dy = (fixturePointer[1] - drag.y) * (drag.edge.includes('n') ? -1 : drag.edge.includes('s') ? 1 : 0)
  fixtureCompanionSize = [Math.min(window.innerWidth - 20, Math.max(520, drag.size[0] + dx)), Math.min(window.innerHeight - 60, Math.max(340, drag.size[1] + dy))]
  paintFixtureCompanion()
}
export async function companionResizeEnd(): Promise<[number, number] | null> { fixtureCompanionDrag = null; return fixtureCompanionSize }
export async function setCompanionSizePreset(preset: 'large' | 'normal' | 'default') {
  fixtureCompanionSize = preset === 'large' ? [Math.min(960, window.innerWidth - 20), Math.min(720, window.innerHeight - 60)] : [640, 420]
  paintFixtureCompanion()
  return preset === 'large'
}
// Next run times for the cron shapes the editor produces (daily, weekdays, one weekday, every n hours), in local time; a custom line the shapes do not cover falls back to daily at its hour and minute.
function fixtureNextRuns(schedule: { kind?: string; cron?: string; at?: string }, count: number): string[] {
  if (schedule.kind === 'once') return [new Date(String(schedule.at)).toISOString()]
  const [minute = '0', hour = '9', , , dow = '*'] = String(schedule.cron ?? '').trim().split(/\s+/)
  const every = /^\*\/(\d+)$/.exec(hour)
  const minutes = /^\d+$/.test(minute) ? Number(minute) : 0
  const hours = every ? Array.from({ length: Math.ceil(24 / Number(every[1])) }, (_, index) => index * Number(every[1])).filter((value) => value < 24) : [/^\d+$/.test(hour) ? Number(hour) : 9]
  const days = dow === '*' ? null : dow === '1-5' ? [1, 2, 3, 4, 5] : /^\d$/.test(dow) ? [Number(dow)] : null
  const now = Date.now()
  const runs: string[] = []
  for (let offset = 0; offset < 400 && runs.length < count; offset += 1) {
    const day = new Date(); day.setDate(day.getDate() + offset)
    if (days && !days.includes(day.getDay())) continue
    for (const at of hours) {
      const candidate = new Date(day.getFullYear(), day.getMonth(), day.getDate(), at, minutes, 0, 0)
      if (candidate.getTime() > now && runs.length < count) runs.push(candidate.toISOString())
    }
  }
  return runs
}
export async function setCompanionMode(mode: string, _animate?: boolean) {
  fixtureCompanionMode = mode
  const [width, height] = mode === 'petit' || mode === 'hidden' ? companionSizes.petit : fixtureCompanionSize
  let style = document.getElementById('fixture-companion-size')
  if (!style) { style = document.createElement('style'); style.id = 'fixture-companion-size'; document.head.append(style) }
  style.textContent = `:root.companion-surface body { background: radial-gradient(900px 500px at 70% 10%, #3b2f63, #12131a) !important; }
    .companion-root { position: fixed; left: 50%; bottom: 40px; transform: translateX(-50%); width: ${width}px !important; height: ${height}px !important; transition: width .34s, height .34s; }`
}
export async function quitRetinue() {}
export async function permissionsPolicyGet() {
  return { defaultMode: 'ask' as const, perAgent: [] }
}
export async function listenForActiveSession(): Promise<Unlisten> { return noop }
export async function listenForActiveRuntime(): Promise<Unlisten> { return noop }
export async function listenForOpenSettings(_handler?: (page: string | null) => void): Promise<Unlisten> { return noop }
export async function listenForDaemonEvents(handler: (event: DaemonEvent) => void): Promise<Unlisten> { daemonListeners.add(handler); scheduleLiveDemo(); return () => daemonListeners.delete(handler) }
function notifyFixtureDaemon(type: string, payload: Record<string, unknown>) {
  const event = { sequence: ++sequence, type, payload } as DaemonEvent
  for (const handler of daemonListeners) handler(event)
}
// The performance benchmark delivers daemon events exactly as the app receives them (`?bench`).
if (flags().has('bench')) (window as unknown as { __benchEmit?: typeof notifyFixtureDaemon }).__benchEmit = notifyFixtureDaemon
// Haze's solid-glass fallback while frames are long (ui/frameStrain.ts), held on for the demo (`?frame-strain`).
if (flags().has('frame-strain')) document.documentElement.dataset.frameStrain = 'true'
export async function listenForDaemonConnection(handler: (connected: boolean) => void): Promise<Unlisten> {
  connectionListeners.add(handler)
  if (!fixtureConnected) handler(false)
  return () => { connectionListeners.delete(handler) }
}

const demoLive = flags().get('demo-live')
let demoScheduled = false
let fixtureHealthRefreshed = false
const fixtureRevertedFiles = new Set<string>()
const fixtureApprovalRules = new Map<string, ApprovalRule>([
  ['preview-rule', { id: 'preview-rule', agentId: 'agent-codex', kind: 'shell', label: 'Run invoice tests', projectId: null }],
])
const livePermissions = new Map<string, PermissionRequest>()
const answeredPermissions = new Set<string>()
const fixtureTurns = new Map<string, { id: string; reply: ChatMessage; timers: number[] }>()
const connectionListeners = new Set<(connected: boolean) => void>()
let fixtureConnected = !flags().has('disconnected')

function setFixtureConnection(connected: boolean, state: DaemonSupervisor['state']) {
  fixtureConnected = connected
  fixtureSupervisor = state === 'running' ? PREVIEW_SUPERVISOR : {
    state, attempt: state === 'restarting' ? 1 : 3, nextRetryMs: state === 'restarting' ? 1100 : null, lastExitCode: 1,
  }
  for (const handler of supervisorListeners) handler(fixtureSupervisor)
  for (const handler of connectionListeners) handler(connected)
}

function resolveFixturePermission(permission: PermissionRequest, choice: string) {
  livePermissions.delete(permission.id)
  answeredPermissions.add(permission.id)
  notifyFixtureDaemon('permission.resolved', { permissionId: permission.id, choice })
}

function finishFixtureTurn(session: Session, outcome: string, turnId = fixtureTurns.get(session.id)?.id) {
  const active = fixtureTurns.get(session.id)
  active?.timers.forEach((timer) => window.clearTimeout(timer))
  fixtureTurns.delete(session.id)
  for (const permission of currentSnapshot().permissions ?? []) {
    if (permission.sessionId === session.id) resolveFixturePermission(permission, 'deny')
  }
  const failed = outcome !== 'completed' && outcome !== 'cancelled'
  const state = failed ? 'error' : outcome
  session.state = state
  delete session.turnId
  session.workingNote = null
  session.updatedAt = new Date().toISOString()
  session.turns = session.turns?.map((turn) => turn.id === turnId || (!turnId && turn.state === 'working')
    ? { ...turn, state, endedAt: session.updatedAt, ...(failed ? { failureClass: outcome } : {}) } : turn)
  session.tools = session.tools?.map((tool) => tool.state === 'running' && (!turnId || tool.turnId === turnId)
    ? { ...tool, state: failed ? 'failed' : outcome, endedAt: session.updatedAt } : tool)
  if (failed) notifyFixtureDaemon('turn.error', { sessionId: session.id, turnId, failureClass: outcome, state })
  else notifyFixtureDaemon(outcome === 'completed' ? 'turn.completed' : 'turn.cancelled', { sessionId: session.id, turnId, state })
  notifyFixtureDaemon('session.changed', { session: { ...session } })
}

function streamFixtureReply(session: Session) {
  const turn = fixtureTurns.get(session.id)
  if (!turn) return
  const turnId = turn.id
  session.state = 'working'
  const tool = { id: `${turnId}-tool`, turnId, tool: 'shell', title: 'Run invoice tests', command: 'npm test -- invoice', state: 'running', startedAt: new Date().toISOString() }
  session.tools = [...(session.tools ?? []), tool]
  notifyFixtureDaemon('tool.changed', { sessionId: session.id, tool })
  const reply = turn.reply
  const content = reply.content ?? ''
  const pieces = content.match(/\S+\s*/g) ?? [content]
  let accumulated = ''
  pieces.forEach((delta, index) => {
    turn.timers.push(window.setTimeout(() => {
      accumulated += delta
      const message = { ...reply, content: accumulated }
      session.messages = [...(session.messages ?? []).filter((item) => item.id !== reply.id), message]
      notifyFixtureDaemon('message.delta', { sessionId: session.id, turnId, messageId: reply.id, role: reply.role, message: reply, delta })
      if (index !== pieces.length - 1) return
      notifyFixtureDaemon('message.completed', { sessionId: session.id, turnId, messageId: reply.id, message })
      finishFixtureTurn(session, 'completed')
    }, 400 + 260 * index))
  })
}

function startFixtureTurn(session: Session, id: string, reply: ChatMessage, mode?: string) {
  const previous = fixtureTurns.get(session.id)
  if (previous) finishFixtureTurn(session, 'cancelled')
  const now = new Date().toISOString()
  session.state = 'working'
  session.turnId = id
  session.updatedAt = now
  session.turns = [...(session.turns ?? []), { id, state: 'working', startedAt: now }]
  const turn = { id, reply: { ...reply, id: `${id}-reply`, turnId: id, role: 'assistant', createdAt: now }, timers: [] as number[] }
  fixtureTurns.set(session.id, turn)
  notifyFixtureDaemon('session.changed', { session: { ...session } })
  if (mode === 'stoppable') return
  if (mode === 'approval') {
    const permission: PermissionRequest = {
      id: `${id}-permission`, sessionId: session.id, runtimeId: session.runtimeId, title: 'Run shell command',
      kind: 'shell', command: 'npm test -- invoice', status: 'pending', choices: ['allow_once', 'allow_session', 'deny'],
    }
    session.state = 'waiting_permission'
    livePermissions.set(permission.id, permission)
    notifyFixtureDaemon('permission.requested', { sessionId: session.id, permission })
  } else if (mode?.startsWith('error-')) {
    turn.timers.push(window.setTimeout(() => finishFixtureTurn(session, mode.slice(6)), 650))
  } else streamFixtureReply(session)
}

function scheduleLiveDemo() {
  if (ownerGrantLive && !ownerGrantScheduled) {
    ownerGrantScheduled = true
    window.setTimeout(() => {
      if (ownerGrantLive) notifyFixtureDaemon('owner_computer.grant', { sessionId: 'session-codex', agentId: 'agent-codex', state: 'live', endsAt: new Date(Date.now() + 300_000).toISOString() })
    }, 500)
  }
  if (!demoLive || demoScheduled) return
  demoScheduled = true
  window.setTimeout(() => {
    if (demoLive === 'restarting' || demoLive === 'gave_up') {
      setFixtureConnection(false, demoLive)
      return
    }
    if (demoLive === 'reconnect') {
      void restartDaemonNow()
      return
    }
    const session = sessions.find((item) => item.id === 'session-codex')
    if (!session) return
    if (demoLive === 'paused') {
      const queue = (fixtureQueues[session.id] ??= { items: [], paused: false })
      queue.paused = true
      notifyFixtureDaemon('queue.changed', { sessionId: session.id, ...queue })
    } else if (['rate_limited', 'retrying', 'sleeping'].includes(demoLive)) {
      session.state = demoLive
      notifyFixtureDaemon('session.changed', { session: { ...session } })
    } else {
      startFixtureTurn(session, 'preview-live-turn', { content: 'The invoice parser tests passed.' }, demoLive)
    }
  }, 1300)
}

const updateAvailableHandlers = new Set<(info: UpdateInfo) => void>()
const updateProgressHandlers = new Set<(progress: UpdateProgress) => void>()

function fixtureUpdateChannel(): UpdateChannel {
  return flags().get('channel') === 'stable' ? 'stable' : 'beta'
}

function fixtureUpdateInfo(channel: UpdateChannel): UpdateInfo {
  return { version: '0.2.0', notes: 'Preview notes for this release.', pubDate: '2026-10-01T12:00:00.000Z', channel }
}

function fixtureUpdatesState(): UpdatesState {
  const mode = flags().get('updates') ?? 'up_to_date'
  const channel = fixtureUpdateChannel()
  return {
    currentVersion: '0.1.0',
    channel,
    autoCheck: flags().get('autoCheck') !== '0',
    lastCheckedAt: mode === 'idle' ? null : '2026-10-02T08:00:00.000Z',
    available: mode === 'available' || mode === 'progress' ? fixtureUpdateInfo(channel) : null,
    devBuild: mode === 'dev',
    storeBuild: mode === 'store',
  }
}

let previewUpdates = fixtureUpdatesState()

export async function updatesGetState(): Promise<UpdatesState> {
  previewUpdates = { ...previewUpdates, ...fixtureUpdatesState(), channel: previewUpdates.channel, autoCheck: previewUpdates.autoCheck }
  return { ...previewUpdates, available: previewUpdates.available ? { ...previewUpdates.available } : null }
}

export async function updatesSetPreferences(preferences: { channel?: string; autoCheck?: boolean }): Promise<UpdatesState> {
  const channel = parseUpdateChannel(preferences.channel)
  if (preferences.channel != null && !channel) throw new Error('invalid_argument')
  previewUpdates = {
    ...previewUpdates,
    ...(channel ? { channel } : {}),
    ...(typeof preferences.autoCheck === 'boolean' ? { autoCheck: preferences.autoCheck } : {}),
  }
  return updatesGetState()
}

export async function updatesCheck() {
  const mode = flags().get('updates') ?? 'up_to_date'
  if (mode === 'checking') await new Promise((resolve) => setTimeout(resolve, 1200))
  const checkedAt = new Date().toISOString()
  if (previewUpdates.devBuild || previewUpdates.storeBuild || mode === 'dev' || mode === 'store') return { status: 'error' as const, checkedAt, error: 'unavailable' as const }
  if (mode === 'error') {
    const error = flags().get('updateError')
    const code = error === 'signature' || error === 'manifest' || error === 'unavailable' || error === 'network' ? error : 'network'
    return { status: 'error' as const, checkedAt, error: code }
  }
  if (mode === 'no_stable_release') {
    previewUpdates = { ...previewUpdates, available: null, lastCheckedAt: checkedAt }
    return { status: 'no_stable_release' as const, checkedAt }
  }
  if (mode === 'available' || mode === 'progress') {
    const update = fixtureUpdateInfo(previewUpdates.channel)
    previewUpdates = { ...previewUpdates, available: update, lastCheckedAt: checkedAt }
    for (const handler of updateAvailableHandlers) handler({ ...update })
    return { status: 'available' as const, checkedAt, update }
  }
  previewUpdates = { ...previewUpdates, available: null, lastCheckedAt: checkedAt }
  return { status: 'up_to_date' as const, checkedAt }
}

export async function updatesInstall() {
  if (!previewUpdates.available) throw new Error('no_update')
  const known = flags().get('updateProgress') !== 'unknown'
  const steps: UpdateProgress[] = known
    ? [
      { phase: 'downloading', downloadedBytes: 256, totalBytes: 1024 },
      { phase: 'downloading', downloadedBytes: 1024, totalBytes: 1024 },
      { phase: 'installing', downloadedBytes: 1024, totalBytes: 1024 },
    ]
    : [
      { phase: 'downloading', downloadedBytes: 256, totalBytes: null },
      { phase: 'installing', downloadedBytes: 256, totalBytes: null },
    ]
  for (const step of steps) {
    for (const handler of updateProgressHandlers) handler({ ...step })
    await new Promise((resolve) => setTimeout(resolve, 280))
  }
}

export async function listenForUpdateAvailable(handler: (info: UpdateInfo) => void): Promise<Unlisten> {
  updateAvailableHandlers.add(handler)
  return () => { updateAvailableHandlers.delete(handler) }
}

export async function listenForUpdateProgress(handler: (progress: UpdateProgress) => void): Promise<Unlisten> {
  updateProgressHandlers.add(handler)
  return () => { updateProgressHandlers.delete(handler) }
}

const updateStateHandlers = new Set<(state: UpdatePhaseState) => void>()
const UPDATE_PHASE_FLAGS = ['downloading', 'ready', 'error'] as const

// Preview flag `?update=downloading|ready|error` seeds the background-updater
// phase (version 0.1.0-beta.99, 42% while downloading) so the update badge can
// be inspected in the browser; with no flag the phase is idle.
function fixtureUpdatePhase(): UpdatePhaseState {
  const flag = flags().get('update')
  if (flag === 'downloading') return { phase: 'downloading', version: '0.1.0-beta.99', downloadedBytes: 42, totalBytes: 100, message: null }
  if (flag === 'ready') return { phase: 'ready', version: '0.1.0-beta.99', downloadedBytes: null, totalBytes: null, message: null }
  if (flag === 'error') return { phase: 'error', version: '0.1.0-beta.99', downloadedBytes: null, totalBytes: null, message: null }
  return IDLE_UPDATE_PHASE
}

export async function updatesPhase(): Promise<UpdatePhaseState> {
  return fixtureUpdatePhase()
}

export async function listenForUpdateState(handler: (state: UpdatePhaseState) => void): Promise<Unlisten> {
  updateStateHandlers.add(handler)
  const flag = flags().get('update')
  if (flag && (UPDATE_PHASE_FLAGS as readonly string[]).includes(flag)) {
    window.setTimeout(() => handler(fixtureUpdatePhase()), 0)
  }
  return () => { updateStateHandlers.delete(handler) }
}

export async function runtimeCapabilities(runtimeId: string, agentId?: string) {
  return parseCapabilities(await rpc('runtime.capabilities', agentId ? { runtimeId, agentId } : { runtimeId }))
}

export async function runtimeModels(runtimeId: string, refresh = false) {
  return parseModelCatalog(await rpc('runtime.models', refresh ? { runtimeId, refresh: true } : { runtimeId }))
}

export async function execSnapshotLatest(sessionId: string) {
  return parseExecSnapshot(await rpc('exec.snapshot.latest', { sessionId }))
}

export async function fetchUsageAnalytics(request: UsageAnalyticsRequest) {
  const params: Record<string, unknown> = { from: request.from, to: request.to, bucket: request.bucket, tz: request.tz }
  if (request.projectPath) params.projectPath = request.projectPath
  if (request.agentId) params.agentId = request.agentId
  const parsed = normalizeAnalytics(await rpc('usage.analytics', params))
  if (!parsed) throw new Error('internal: Usage analytics could not be loaded.')
  return parsed
}

export async function setApprovalBadge(_count: number): Promise<void> { /* No native taskbar in the browser preview. */ }

// The browser preview has no OS compositor behind it, so the frosted backdrop
// reports itself unavailable instead of pretending.
export async function mainWindowReady(): Promise<void> { /* the preview has no native window */ }

// The preview reports the acrylic the installed window has while its production frame is on (productionFrame.ts).
export async function applyWindowBackdrop(enabled: boolean, _dark: boolean): Promise<WindowBackdrop> {
  return previewBackdrop(enabled)
}
export async function windowBackdropState(): Promise<WindowBackdrop> {
  return previewBackdrop(document.documentElement.dataset.style === 'haze')
}

// The browser preview has no shell: terminals are a desktop-only feature.
export type TerminalShell = 'powershell' | 'pwsh' | 'cmd'
export type TerminalOpened = { id: string; pid: number | null; shell: string; cwd: string }
export type TerminalInfo = TerminalOpened & { running: boolean; exitCode: number | null }
export type TerminalOutput = { id: string; seq: number; data: string }
export type TerminalExit = { id: string; exitCode: number | null }
export async function terminalOpen(_projectPath: string, _options: { cwd?: string | null; shell?: TerminalShell; cols?: number; rows?: number } = {}): Promise<TerminalOpened> { throw new Error('Terminals run in the desktop app, not in the browser preview.') }
export async function terminalWrite(_id: string, _data: string): Promise<void> {}
export async function terminalResize(_id: string, _cols: number, _rows: number): Promise<void> {}
export async function terminalClose(_id: string): Promise<void> {}
export async function terminalList(): Promise<TerminalInfo[]> { return [] }
export async function listenForTerminalOutput(_handler: (output: TerminalOutput) => void): Promise<Unlisten> { return noop }
export async function listenForTerminalExit(_handler: (exit: TerminalExit) => void): Promise<Unlisten> { return noop }

// ---- Preview-only vault, chat apps and Pi (16 November 2026). Shapes follow docs/plans/vault-channels-pi.md.
type PreviewVaultItem = { id: string; kind: 'login' | 'card' | 'apiKey' | 'note'; title: string; origin: string | null; username: string | null; holder: string | null; brand: string | null; last4: string | null; expiry: string | null; service: string | null; hasTotp: boolean; createdAt: string; updatedAt: string; lastUsedAt: string | null; useCount: number }
const previewVaultBase = { origin: null, username: null, holder: null, brand: null, last4: null, expiry: null, service: null, hasTotp: false, createdAt: minutesAgo(3000), updatedAt: minutesAgo(3000), lastUsedAt: null, useCount: 0 }
let previewVault: PreviewVaultItem[] = flags().get('vault') === 'empty' ? [] : [
  { ...previewVaultBase, id: 'v1', kind: 'login', title: 'GitHub', origin: 'https://github.com', username: 'owner@example.com', hasTotp: true, lastUsedAt: minutesAgo(42), useCount: 6 },
  { ...previewVaultBase, id: 'v2', kind: 'card', title: 'Personal Visa', holder: 'A. Owner', brand: 'Visa', last4: '4242', expiry: '08/28' },
  { ...previewVaultBase, id: 'v3', kind: 'apiKey', title: 'OpenAI', service: 'OpenAI platform', lastUsedAt: minutesAgo(60 * 26), useCount: 2 },
  { ...previewVaultBase, id: 'v4', kind: 'note', title: 'Wi-Fi at the office' },
]
let previewVaultActivity = [
  { id: 'a1', at: minutesAgo(42), itemId: 'v1', itemTitle: 'GitHub', action: 'fill', where: 'browser', origin: 'https://github.com', bot: 'Ada', sessionId: null },
  { id: 'a2', at: minutesAgo(300), itemId: 'v3', itemTitle: 'OpenAI', action: 'denied', where: 'cloud', origin: null, bot: 'Grey', sessionId: null },
  { id: 'a3', at: minutesAgo(3000), itemId: 'v2', itemTitle: 'Personal Visa', action: 'create', where: 'desktop', origin: null, bot: null, sessionId: null },
]
let previewVaultUses = flags().get('vault') === 'request' ? [{ requestId: 'r1', sessionId: 'session-codex', bot: 'Ada', itemId: 'v1', itemTitle: 'GitHub', kind: 'login', field: 'password', surface: 'browser', origin: 'https://github.com', expiresAt: new Date(Date.now() + 50_000).toISOString() }] : []
const previewChannelOff = (kind: string): Record<string, unknown> => ({ kind, status: 'off', detail: null, account: null, peer: null, approvals: true, sessionId: null, lastMessageAt: null, qrDataUrl: null, pairing: null, candidate: null })
let previewChannels: Array<Record<string, unknown>> = [
  flags().get('channels') === 'connected' ? { ...previewChannelOff('telegram'), status: 'connected', account: '@retinue_ceo_bot', peer: { name: 'Owner', handle: '@owner' }, sessionId: 'session-codex', lastMessageAt: minutesAgo(5) } : previewChannelOff('telegram'),
  previewChannelOff('whatsapp-web'),
  previewChannelOff('whatsapp-cloud'),
]
const previewPiKeys = new Set<string>(['anthropic'])
function previewQr() {
  let rects = ''
  for (let y = 0; y < 21; y += 1) for (let x = 0; x < 21; x += 1) {
    const corner = (x < 7 && y < 7) || (x > 13 && y < 7) || (x < 7 && y > 13)
    const cx = x > 13 ? x - 14 : x, cy = y > 13 ? y - 14 : y
    const on = corner ? (cx === 0 || cx === 6 || cy === 0 || cy === 6 || (cx > 1 && cx < 5 && cy > 1 && cy < 5)) : (x * 7 + y * 13 + x * y) % 3 === 0
    if (on) rects += `<rect x="${x}" y="${y}" width="1" height="1"/>`
  }
  return `data:image/svg+xml;base64,${btoa(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21" shape-rendering="crispEdges">${rects}</svg>`)}`
}
function previewVaultChannelsPi(method: string, params: Record<string, unknown>): unknown {
  const now = new Date().toISOString()
  const log = (action: string, item: { id: string; title: string } | null) => { previewVaultActivity = [{ id: `a${Date.now()}`, at: now, itemId: item?.id ?? '', itemTitle: item?.title ?? '', action, where: 'desktop', origin: null, bot: null, sessionId: null }, ...previewVaultActivity] }
  const secrets = (params.secrets ?? {}) as Record<string, string>
  switch (method) {
    case 'vault.list': return { items: previewVault }
    case 'vault.activity': return { rows: previewVaultActivity.slice(0, Number(params.limit ?? 50)) }
    case 'vault.create': {
      const digits = String(secrets.number ?? '').replace(/\D/g, '')
      const item: PreviewVaultItem = { ...previewVaultBase, id: `v${Date.now()}`, kind: params.kind as PreviewVaultItem['kind'], title: String(params.title), origin: (params.origin as string) ?? null, username: (params.username as string) ?? null, holder: (params.holder as string) ?? null, expiry: (params.expiry as string) ?? null, service: (params.service as string) ?? null, brand: digits ? (digits.startsWith('4') ? 'Visa' : digits.startsWith('5') ? 'Mastercard' : 'Card') : null, last4: digits ? digits.slice(-4) : null, hasTotp: !!secrets.totp, createdAt: now, updatedAt: now }
      previewVault = [...previewVault, item]; log('create', item); return { item }
    }
    case 'vault.update': {
      let item: PreviewVaultItem | undefined
      previewVault = previewVault.map((row) => row.id === params.id ? (item = { ...row, ...Object.fromEntries(Object.entries(params).filter(([key]) => key !== 'id' && key !== 'secrets')), hasTotp: row.hasTotp || !!secrets.totp, updatedAt: now } as PreviewVaultItem) : row)
      if (item) log('update', item)
      return { item }
    }
    case 'vault.delete': { const item = previewVault.find((row) => row.id === params.id); previewVault = previewVault.filter((row) => row.id !== params.id); if (item) log('delete', item); return { deleted: true } }
    case 'vault.reveal': { const item = previewVault.find((row) => row.id === params.id); if (item) log('reveal', item); return { value: params.field === 'number' ? '4242 4242 4242 4242' : params.field === 'cvc' ? '123' : 'preview-only-value' } }
    case 'vault.totp': return { code: '492 817', expiresInSecs: 21 }
    case 'vault.use.list': return { requests: previewVaultUses }
    case 'vault.use.answer': previewVaultUses = previewVaultUses.filter((row) => row.requestId !== params.requestId); return { ok: true }
    case 'channel.list': return { channels: previewChannels }
    case 'channel.telegram.connect': {
      if (!/^\d+:[\w-]{10,}$/.test(String(params.botToken))) throw new Error('That doesn’t look like a Telegram bot token. It looks like 123456789:AA…')
      const pairing = { link: 'https://t.me/retinue_ceo_bot?start=K7Q2-MX9P', code: 'K7Q2-MX9P', expiresAt: new Date(Date.now() + 600_000).toISOString() }
      previewChannels = previewChannels.map((row) => row.kind === 'telegram' ? { ...row, status: 'pairing', account: '@retinue_ceo_bot', pairing } : row)
      window.setTimeout(() => {
        previewChannels = previewChannels.map((row) => row.kind === 'telegram' && row.status === 'pairing' ? { ...row, candidate: { name: 'Owner', handle: '@owner', at: new Date().toISOString() } } : row)
        notifyFixtureDaemon('channel.pairing.candidate', { kind: 'telegram' })
      }, 2500)
      return { channel: previewChannels[0], pairing }
    }
    case 'channel.whatsappWeb.start': previewChannels = previewChannels.map((row) => row.kind === 'whatsapp-web' ? { ...row, status: 'pairing', qrDataUrl: previewQr() } : row); return { channel: previewChannels[1] }
    case 'channel.whatsappCloud.connect': previewChannels = previewChannels.map((row) => row.kind === 'whatsapp-cloud' ? { ...row, status: 'connecting', account: '+44 •••• 1234', detail: 'Waiting for Meta to call the webhook.' } : row); return { channel: previewChannels[2], webhook: { url: 'https://vps.example.net/retinue/whatsapp', verifyToken: 'rtn-7f3a9c' } }
    case 'channel.pairing.answer': previewChannels = previewChannels.map((row) => {
      if (row.kind !== params.kind) return row
      const candidate = row.candidate as { name: string; handle: string | null } | null
      return params.approve ? { ...row, status: 'connected', peer: candidate ? { name: candidate.name, handle: candidate.handle } : null, candidate: null, pairing: null, sessionId: 'session-codex' } : { ...row, candidate: null }
    }); return { ok: true }
    case 'channel.update': previewChannels = previewChannels.map((row) => row.kind === params.kind ? { ...row, approvals: params.approvals ?? row.approvals } : row); return { ok: true }
    case 'channel.disconnect': previewChannels = previewChannels.map((row) => row.kind === params.kind ? previewChannelOff(String(params.kind)) : row); return { ok: true }
    case 'channel.test': return { ok: true }
    case 'pi.providers': return flags().get('pi') === 'missing' ? { installed: false, version: null, providers: [] } : { installed: true, version: '1.5.2', providers: [
      { id: 'anthropic', name: 'Anthropic', keyEnv: 'ANTHROPIC_API_KEY', hasKey: previewPiKeys.has('anthropic'), models: 9, docsUrl: 'https://console.anthropic.com/settings/keys' },
      { id: 'openai', name: 'OpenAI', keyEnv: 'OPENAI_API_KEY', hasKey: previewPiKeys.has('openai'), models: 14, docsUrl: 'https://platform.openai.com/api-keys' },
      { id: 'google', name: 'Google Gemini', keyEnv: 'GEMINI_API_KEY', hasKey: previewPiKeys.has('google'), models: 7, docsUrl: 'https://aistudio.google.com/apikey' },
      { id: 'openrouter', name: 'OpenRouter', keyEnv: 'OPENROUTER_API_KEY', hasKey: previewPiKeys.has('openrouter'), models: null, docsUrl: 'https://openrouter.ai/keys' },
    ] }
    case 'pi.key.set': previewPiKeys.add(String(params.provider)); return { ok: true }
    case 'pi.key.delete': previewPiKeys.delete(String(params.provider)); return { ok: true }
    case 'pi.key.test': return { ok: true, detail: 'The key works. 9 models available.' }
    default: throw new Error(`${method} is not in the preview`)
  }
}

// ---- Preview-only connectors (docs/plans/nod-adoption-plan.md, D). Addresses here are placeholders, not checked.
type PreviewConnector = Connector
let previewConnectors: PreviewConnector[] = flags().get('connectors') === 'empty' ? [] : [
  { id: 'c1', name: 'Linear', url: 'https://mcp.linear.app/mcp', auth: 'token', enabled: true, status: 'ok', detail: null, toolCount: 4, unattended: 'readOnly', lastCheckedAt: minutesAgo(12) },
  { id: 'c2', name: 'Internal wiki', url: 'https://wiki.example.com/mcp', auth: 'token', enabled: true, status: 'error', detail: 'The server refused the token (401).', toolCount: null, unattended: 'off', lastCheckedAt: minutesAgo(90) },
]
const oauthSeed = flags().get('demo-oauth')
if (oauthSeed) previewConnectors = [{ id: 'preview-oauth', name: 'Example connector', url: 'https://connector.example.invalid/mcp', auth: 'oauth', enabled: true, status: 'unchecked', detail: null, toolCount: null, unattended: 'off', lastCheckedAt: null }]
const oauthStates = new Map<string, ConnectorSignIn>()
const oauthTimers = new Map<string, number>()

const previewConnectorTools: Record<string, Array<{ name: string; title: string | null; description: string; readOnly: boolean | null; rule: 'ask' | 'allow' | 'deny' }>> = {
  c1: [
    { name: 'list_issues', title: 'List issues', description: 'Find issues by team, state or text.', readOnly: true, rule: 'allow' },
    { name: 'get_issue', title: 'Get issue', description: 'Read one issue with its comments.', readOnly: true, rule: 'allow' },
    { name: 'create_issue', title: 'Create issue', description: 'Open a new issue in a team.', readOnly: false, rule: 'ask' },
    { name: 'delete_issue', title: 'Delete issue', description: 'Delete an issue permanently.', readOnly: false, rule: 'deny' },
  ],
}
const previewCatalogue = [
  { id: 'github', name: 'GitHub', url: 'https://api.githubcopilot.com/mcp/', docsUrl: 'https://github.com/settings/tokens', auth: 'token', tokenHint: 'Use a fine-grained personal access token.' },
  { id: 'linear', name: 'Linear', url: 'https://mcp.linear.app/mcp', docsUrl: 'https://linear.app/settings/api', auth: 'token', tokenHint: 'Create a personal API key in Linear.' },
  { id: 'notion', name: 'Notion', url: 'https://mcp.notion.com/mcp', docsUrl: 'https://www.notion.so/profile/integrations', auth: 'token', tokenHint: 'Create an internal integration and copy its secret.' },
  { id: 'sentry', name: 'Sentry', url: 'https://mcp.sentry.dev/mcp', docsUrl: 'https://sentry.io/settings/account/api/auth-tokens/', auth: 'token', tokenHint: 'Create a user auth token.' },
]
function previewConnectorsRpc(method: string, params: Record<string, unknown>): unknown {
  switch (method) {
    case 'connector.oauth.status':
    case 'connector.oauth.start':
    case 'connector.oauth.cancel': {
      const connector = previewConnectors.find((item) => item.id === params.id)
      if (!connector) throw rpcError('not_found', 'Connector not found.')
      const { id, auth } = connector
      const update = (status: ConnectorSignIn['status'], detail: string | null = null, expiresAt: string | null = null) => {
        const state: ConnectorSignIn = { id, auth, status, detail, expiresAt }
        oauthStates.set(id, state)
        notifyFixtureDaemon('connector.oauth.changed', state)
      }
      if (method === 'connector.oauth.status') return oauthStates.get(id) ?? { id, auth, status: 'idle', detail: null, expiresAt: null } satisfies ConnectorSignIn
      const timer = oauthTimers.get(id)
      if (timer !== undefined) window.clearTimeout(timer)
      oauthTimers.delete(id)
      if (method === 'connector.oauth.cancel') {
        const cancelled = oauthStates.get(id)?.status === 'pending'
        if (cancelled) update('idle')
        return { cancelled }
      }
      if (auth !== 'oauth') throw rpcError('invalid_argument', 'This connector does not use browser sign-in.')
      const expiresAt = new Date(Date.now() + 300_000).toISOString()
      update('pending', null, expiresAt)
      if (oauthSeed !== 'pending') oauthTimers.set(id, window.setTimeout(() => {
        oauthTimers.delete(id)
        if (oauthSeed === 'error') update('error', 'The sign-in was not completed in time. Try again.')
        else update('authorized')
      }, 1200))
      return { status: 'pending', authorizationUrl: 'https://connector.example.invalid/authorize', expiresAt }
    }
    case 'connector.list': return { connectors: previewConnectors, catalogue: previewCatalogue }
    case 'connector.add': {
      const connector: PreviewConnector = { id: `c${Date.now()}`, name: String(params.name), url: String(params.url), auth: params.auth === 'oauth' ? 'oauth' : params.token ? 'token' : 'none', enabled: true, status: 'ok', detail: null, toolCount: 3, unattended: 'off', lastCheckedAt: new Date().toISOString() }
      previewConnectors = [...previewConnectors, connector]
      previewConnectorTools[connector.id] = [{ name: 'search', title: 'Search', description: 'Search this app.', readOnly: true, rule: 'ask' }, { name: 'read', title: 'Read', description: 'Read one item.', readOnly: true, rule: 'ask' }, { name: 'write', title: 'Write', description: 'Change one item.', readOnly: false, rule: 'ask' }]
      return { connector }
    }
    case 'connector.update': previewConnectors = previewConnectors.map((row) => row.id === params.id ? { ...row, ...Object.fromEntries(Object.entries(params).filter(([key]) => key !== 'id' && key !== 'token')) } as PreviewConnector : row); return { connector: previewConnectors.find((row) => row.id === params.id) }
    case 'connector.remove': previewConnectors = previewConnectors.filter((row) => row.id !== params.id); return { ok: true }
    case 'connector.test': previewConnectors = previewConnectors.map((row) => row.id === params.id ? { ...row, lastCheckedAt: new Date().toISOString() } : row); return { ok: true, detail: null, toolCount: previewConnectors.find((row) => row.id === params.id)?.toolCount ?? null }
    case 'connector.tools': return { tools: previewConnectorTools[String(params.id)] ?? [] }
    case 'connector.tool.rule': { const list = previewConnectorTools[String(params.id)] ?? []; previewConnectorTools[String(params.id)] = list.map((tool) => tool.name === params.tool ? { ...tool, rule: params.rule as 'ask' | 'allow' | 'deny' } : tool); return { ok: true } }
    default: throw new Error(`${method} is not in the preview`)
  }
}
