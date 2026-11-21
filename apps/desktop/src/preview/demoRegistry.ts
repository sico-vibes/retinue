import { GROUPS, type Step } from './inspector/scenarios'

export type DemoEntry = {
  id: string
  area: string
  label: string
  url: string
  steps?: readonly Step[]
  errorCode?: string
}

const conversation = '/?no-intro&clean&idle&session=session-codex&agent=agent-codex'
const live = (id: string, area: string, label: string): DemoEntry => ({ id, area, label, url: `${conversation}&demo-live=${id}` })

// Explicit so a new code or alias is caught by the coverage test.
const errorCodes = [
  'invalid_argument', 'invalidargument', 'provider_unavailable', 'providerunavailable', 'no_adapter', 'not_installed',
  'unauthenticated', 'not_signed_in', 'notsignedin', 'auth_required', 'authrequired', 'unauthorized',
  'rate_limited', 'ratelimited', 'overloaded', 'too_many_requests', 'model_unavailable', 'modelunavailable', 'invalid_model',
  'context', 'context_exhausted', 'context_overflow', 'timeout', 'process_exited', 'processexited', 'process_error',
  'permission_denied', 'permissiondenied', 'permission', 'quota', 'quota_exceeded', 'usage_limit', 'budget', 'budget_stop',
  'not_found', 'notfound', 'conflict', 'internal', 'cancelled', 'canceled', 'aborterror', 'provider_error', 'provider',
  'protocol_error', 'config_unsupported', 'config', 'unsupported',
]

const areas: Record<string, string> = {
  main: 'Main shell', launch: 'First run and launch', messages: 'Conversation',
  approvals: 'Approvals', phone: 'Remote', machines: 'Linked machines',
}

export const demoEntries: readonly DemoEntry[] = [
  live('stoppable', 'Conversation', 'Stop a running turn'),
  live('streaming', 'Conversation', 'Stream a reply and finish'),
  live('approval', 'Approvals', 'Receive and answer a live approval'),
  live('restarting', 'Connection and service', 'Service restarting'),
  live('gave_up', 'Connection and service', 'Service gave up'),
  live('reconnect', 'Connection and service', 'Disconnect and come back online'),
  live('rate_limited', 'Conversation', 'Rate limited'),
  live('retrying', 'Conversation', 'Retrying'),
  ...['rate_limited', 'retrying', 'sleeping'].map((state): DemoEntry => ({
    id: 'companion-demo-' + state, area: 'Companion',
    label: { rate_limited: 'Rate limited', retrying: 'Retrying', sleeping: 'Sleeping' }[state] ?? state,
    url: conversation + '&companion&mode=home&view=chat&demo-live=' + state,
  })),
  live('paused', 'Conversation', 'Prompt queue paused'),
  { id: 'first-run', area: 'First run and launch', label: 'First run in the app', url: '/?no-intro&demo-first-run' },
  { id: 'scan-error', area: 'First run and launch', label: 'Coding agent scan fails', url: '/?no-intro&demo-first-run&demo-scan-error', steps: [{ clickText: 'Scan again' }] },
  { id: 'search-results', area: 'Conversation', label: 'Message search results', url: '/?no-intro&clean&open=search', steps: [{ type: ['input[aria-label="Search message text"]', 'invoice'] }] },
  { id: 'health-checks', area: 'Health', label: 'Health check with a fix', url: '/?no-intro&clean&open=settings:Health' },
  { id: 'health-error', area: 'Health', label: 'Health check could not run', url: '/?no-intro&clean&open=settings:Health&demo-health=error' },
  { id: 'launch-in-app', area: 'First run and launch', label: 'Launch in the main window', url: '/' },
  { id: 'gallery', area: 'Gallery', label: 'Preview gallery', url: '/gallery.html' },
  { id: 'gallery-characters', area: 'Gallery', label: 'Character gallery', url: '/preview.html' },
  { id: 'workspace-shared', area: 'Session rail', label: 'Shared workspace', url: '/?no-intro&clean&idle&session=session-codex&agent=agent-codex&open=inspector' },
  { id: 'workspace-error', area: 'Session rail', label: 'Workspace could not be read', url: '/?no-intro&clean&idle&session=session-codex&agent=agent-codex&open=inspector&demo-rail-error=session.workspace' },
  { id: 'checkpoints', area: 'Session rail', label: 'File checkpoints', url: '/?no-intro&clean&idle&session=session-codex&agent=agent-codex&open=inspector' },
  { id: 'checkpoints-error', area: 'Session rail', label: 'File checkpoints could not be read', url: '/?no-intro&clean&idle&session=session-codex&agent=agent-codex&open=inspector&demo-rail-error=session.checkpoints' },
  { id: 'search-error', area: 'Conversation', label: 'Message search fails', url: '/?no-intro&clean&open=search&demo-search-error', steps: [{ type: ['input[aria-label="Search message text"]', 'invoice'] }] },
  { id: 'native-remote-off', area: 'Remote', label: 'Native remote setup', url: '/?no-intro&clean&open=settings:Remote&remote-method=native' },
  { id: 'native-remote-on', area: 'Remote', label: 'Native remote with devices', url: '/?no-intro&clean&open=settings:Remote&remote-method=native&remote=on&devices=3' },
  { id: 'vault-list', area: 'Vault', label: 'Saved vault items', url: '/?no-intro&clean&open=settings:Vault' },
  { id: 'vault-empty', area: 'Vault', label: 'Empty vault', url: '/?no-intro&clean&open=settings:Vault&vault=empty' },
  { id: 'vault-request', area: 'Vault', label: 'Waiting vault use request', url: '/?no-intro&clean&session=session-codex&agent=agent-codex&vault=request' },
  { id: 'usage-analytics-empty', area: 'Usage and limits', label: 'Usage analytics with no data', url: '/?no-intro&clean&open=usage&analytics=empty' },
  { id: 'usage-analytics-error', area: 'Usage and limits', label: 'Usage analytics could not load', url: '/?no-intro&clean&open=usage&analytics=error' },
  { id: 'project-start-suggestions', area: 'Conversation', label: 'Suggestions for a new project conversation', url: '/?no-intro&clean&scene=empty-conversation' },
  ...errorCodes.map((errorCode): DemoEntry => ({ ...live(`error-${errorCode}`, 'Provider errors', errorCode), errorCode })),
  ...GROUPS.flatMap((group) => group.scenarios.map((scenario): DemoEntry => {
    const area = scenario.id === 'main-disconnected' ? 'Connection and service'
      : /[?&]open=schedules(?:&|$)/.test(scenario.url) ? 'Schedules and automations'
      : /[?&](?:open=usage|usage-bar|quota-error|no-quota)(?:&|$)/.test(scenario.url) ? 'Usage and limits'
      : /[?&]open=tasks(?:&|$)/.test(scenario.url) ? 'Tasks'
      : /[?&](?:remote=|open=settings:Remote)/.test(scenario.url) ? 'Remote'
      : areas[group.id] ?? group.title
    return {
      id: scenario.id, area, label: scenario.title,
      url: scenario.url.startsWith('/?') || scenario.url === '/'
        ? `${scenario.url}${scenario.url.includes('?') ? '&' : '?'}no-intro${scenario.raw ? '' : '&clean'}`
        : scenario.url,
      steps: scenario.steps,
    }
  })),
]

