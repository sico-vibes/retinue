import { clipboardReadFailures, dictationReasons, pairingFailures, scaffoldFailures, workflowRpcErrors } from './demoWorkflowErrors'
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

const demo = (id: string, area: string, label: string, route: string, steps?: readonly Step[]): DemoEntry => ({ id, area, label, url: route, steps })
const click = (clickText: string): Step => ({ clickText })
const settings = (page: string, query = '') => `/?no-intro&clean&open=settings:${page}${query}`
const fail = (entry: DemoEntry, method: string, code?: string): DemoEntry => ({
  ...entry, url: `${entry.url}&demo-rpc-error=${method}${code ? `&demo-error-code=${code}` : ''}`,
})
const health = settings('Health')
const connectors = settings('Connectors')
const skills = settings('Skills', '&demo-learned')
const launchers = settings('Agents', '&demo-profiles=saved')
const diagnostics = '/?no-intro&clean&open=diagnostics'
const addLauncher: Step[] = [
  click('Add launcher'),
  { type: ['input[aria-label="Profile name"]', 'My Codex launcher'] },
  { type: ['input[aria-label="Profile executable"]', 'C:/Tools/codex.exe'] },
  click('Save launcher'),
]
const phone = '/remote.html?phone=paired'
const changeModel: Step[] = [{ click: 'button.model-pill[aria-haspopup="menu"]' }, { click: '.model-menu [role="menuitemradio"][data-value]:not([data-value="gpt-6.1-sol"])' }]

const pairing = settings('Machines')
const probeMachine: Step[] = [click('Link a machine'), { type: ['.machine-pair input.text-input', 'https://work-laptop.example.ts.net'] }, { click: '.machine-pair button[type=submit]' }]
const pairMachine: Step[] = [...probeMachine, { type: ['.machine-code', 'K7QM-4XPD'] }, { click: '.machine-pair button[type=submit]' }]
const newProject = '/?no-intro&clean&open=new-project&demo-project-folder'
const cloneProject: Step[] = [click('Clone'), { type: ['input[placeholder="https://github.com/owner/repo.git"]', 'https://github.com/example/project.git'] }, { click: 'button[aria-describedby="clone-where"]' }, { click: '.blob-dialog-actions .primary-button' }]
const searchProject: Step[] = [{ type: ['input[aria-label="Search for"]', 'invoice'] }]
const workflowRoutes: Record<keyof typeof workflowRpcErrors, { area: string; label: string; url: string; steps: readonly Step[] }> = {
  'machine.probe': { area: 'Linked machines', label: 'Check machine address', url: pairing, steps: probeMachine },
  'machine.pair.begin': { area: 'Linked machines', label: 'Start machine pairing', url: pairing, steps: pairMachine },
  'project.clone': { area: 'Projects', label: 'Clone repository', url: newProject, steps: cloneProject },
  'project.discover': { area: 'Projects', label: 'Find repositories', url: newProject, steps: [click('Find'), { click: 'button[aria-describedby="discover-where"]' }] },
  'project.content.search': { area: 'Projects', label: 'Search project', url: conversation + '&open=project-search', steps: searchProject },
  'project.create': { area: 'Projects', label: 'Add discovered repository (select one and add it)', url: newProject, steps: [click('Find'), { click: 'button[aria-describedby="discover-where"]' }] },
  'project.update': { area: 'Projects', label: 'Save project settings', url: conversation + '&open=project-settings', steps: [{ click: '.blob-dialog-actions .primary-button' }] },
  'project.delete': { area: 'Projects', label: 'Delete project', url: conversation + '&open=delete-project', steps: [{ click: '.danger-confirm-button' }] },
  'project.sharedSettings.trust': { area: 'Projects', label: 'Trust shared settings file', url: conversation + '&open=project-settings', steps: [click('Trust this file')] },
  'project.sharedSettings.untrust': { area: 'Projects', label: 'Stop using shared settings file', url: conversation + '&open=project-settings&demo-shared=trusted', steps: [click('Stop using it')] },
  'project.sharedSettings.write': { area: 'Projects', label: 'Write shared settings file', url: conversation + '&open=project-settings&demo-shared=missing', steps: [click('Write my saved settings to the file')] },
  'project.scaffold': { area: 'Projects', label: 'Create project', url: newProject, steps: [{ type: ['input[placeholder="my-project"]', 'invoice-tests'] }, { click: 'button[aria-describedby="new-project-where"]' }, click('Create project')] },
}

export const demoEntries: readonly DemoEntry[] = [
  ...Object.entries(workflowRoutes).flatMap(([key, route]) => {
    const method = key as keyof typeof workflowRpcErrors
    return Object.keys(workflowRpcErrors[method]).map((code) => fail(demo(`workflow-${method}-${code}`, route.area, `${route.label}: ${code.replaceAll('_', ' ')}`, route.url, route.steps), method, code))
  }),
  demo('machine-invalid-address', 'Linked machines', 'Address fails form validation', pairing, [click('Link a machine'), { type: ['.machine-pair input.text-input', 'not an address'] }]),
  demo('machine-unreachable', 'Linked machines', 'Machine address is unreachable', pairing + '&demo-machine-probe=unreachable', probeMachine),
  demo('machine-wrong-server', 'Linked machines', 'Address is not a Retinue machine', pairing + '&demo-machine-probe=wrong-server', probeMachine),
  ...['expired', 'rejected', 'cancelled', 'failed'].map((state) => demo(`machine-pair-${state}`, 'Linked machines', `Pairing ${state}`, `${pairing}&demo-machine-pair=${state}`, pairMachine)),
  ...Object.entries(pairingFailures).map(([reason, detail]) => demo(`machine-pair-${reason}`, 'Linked machines', detail, `${pairing}&demo-machine-pair=${reason}`, pairMachine)),
  demo('project-shared-settings-invalid', 'Projects', 'Shared settings file is invalid', conversation + '&open=project-settings&demo-shared=invalid'),
  demo('project-invalid-name', 'Projects', 'Project folder name is invalid', newProject, [{ type: ['input[placeholder="my-project"]', 'invalid/name'] }]),
  demo('project-clone-job-failed', 'Projects', 'Clone job fails after starting', newProject + '&clone=fail', cloneProject),
  demo('project-search-invalid-pattern', 'Projects', 'Search pattern is invalid', conversation + '&open=project-search&demo-search-pattern', [{ click: 'button[aria-label="Regular expression"]' }, { type: ['input[aria-label="Search for"]', '('] }]),
  ...Object.entries(clipboardReadFailures).map(([id, message]) => demo(
    `clipboard-image-${id}-error`, 'Clipboard', message, `${conversation}&demo-clipboard=${id}`, [{ paste: '.composer-box textarea' }],
  )),
  demo('clipboard-connect-command-error', 'Clipboard', 'Connect command could not be copied', settings('Connections', '&preset=mcp&demo-clipboard=write'), [{ click: '.connect-command-copy' }]),
  demo('clipboard-image-empty', 'Clipboard', 'Paste with no image (stays quiet)', conversation + '&demo-clipboard=empty', [{ paste: '.composer-box textarea' }]),
  demo('clipboard-image-stage-error', 'Clipboard', 'Pasted image could not be staged', conversation + '&demo-clipboard=stage', [{ paste: '.composer-box textarea' }]),
  ...Object.entries(scaffoldFailures).map(([id, detail]) => demo(`project-scaffold-step-${id}`, 'Projects', detail, newProject + `&demo-scaffold-step=${id}`, workflowRoutes['project.scaffold'].steps)),
  fail(demo('machine-name-save-error', 'Linked machines', 'This machine name could not be saved (edit the name)', pairing), 'machine.self.rename'),
  fail(demo('machine-link-refresh-error', 'Linked machines', 'Linked machine could not be checked (refresh a link)', settings('Machines', '&machines=1')), 'machine.refresh'),
  fail(demo('outside-preview-error', 'Accounts', 'Claude Code settings could not be changed (open outside sessions setup)', settings('General')), 'outside.preview'),
  fail(demo('computer-action-error', 'Companion', 'Linked computer action fails (use a power control)', settings('Computer', '&preset=computer')), 'computer.action'),
  fail(demo('subbot-stop-error', 'Conversation', 'Helpers could not be stopped', conversation + '&subbots=1', [click('Stop all')]), 'delegation.cancelGroup'),
  ...['bubble', 'companion'].flatMap((surface) => [
    demo(`dictation-error-${surface}`, 'Dictation', `Microphone unavailable (${surface})`, `/dictation-preview.html?state=error&surface=${surface}`),
    ...dictationReasons.flatMap((reason, index) => ['missing', 'failed'].map((fallback) => demo(
      `dictation-${surface}-${index}-${fallback}`, 'Dictation', `${reason}: local model ${fallback} (${surface})`,
      `/dictation-preview.html?state=error&surface=${surface}&reason=${encodeURIComponent(reason)}&fallback=${fallback}`,
    ))),
  ]),
  demo('roster-reorder', 'Main shell', 'Reorder Codex employees by dragging in the roster', conversation),
  fail(demo('roster-stale', 'Main shell', 'Roster order out of date (drag an employee)', conversation), 'agent.reorder', 'invalid_argument'),
  fail(demo('roster-runtime-missing', 'Main shell', 'Roster runtime unavailable (drag an employee)', conversation), 'agent.reorder', 'not_found'),
  demo('oauth-idle', 'Connectors', 'Browser sign-in ready', connectors + '&demo-oauth=idle'),
  demo('oauth-pending', 'Connectors', 'Browser sign-in pending (can cancel)', connectors + '&demo-oauth=pending', [click('Sign in')]),
  demo('oauth-authorized', 'Connectors', 'Browser sign-in completed', connectors + '&demo-oauth=authorized', [click('Sign in')]),
  demo('oauth-error', 'Connectors', 'Browser sign-in failed', connectors + '&demo-oauth=error', [click('Sign in')]),
  fail(demo('oauth-start-error', 'Connectors', 'Browser sign-in could not start', connectors + '&demo-oauth=idle', [click('Sign in')]), 'connector.oauth.start'),
  fail(demo('oauth-status-error', 'Connectors', 'Browser sign-in status unavailable', connectors + '&demo-oauth=idle'), 'connector.oauth.status'),
  fail(demo('connectors-load-error', 'Connectors', 'Connectors could not be read', connectors), 'connector.list'),
  demo('diagnostics-saved', 'Health', 'Diagnostics saved', diagnostics, [click('Save as')]),
  fail(demo('diagnostics-list-error', 'Health', 'Diagnostics list could not be read', diagnostics), 'diagnostics.preview'),
  demo('diagnostics-dialog-error', 'Health', 'Diagnostics save dialog could not open', diagnostics + '&demo-diagnostics=dialog-error', [click('Save as')]),
  demo('diagnostics-write-error', 'Health', 'Diagnostics file could not be saved', diagnostics + '&demo-diagnostics=write-error', [click('Save as')]),
  fail(demo('worktree-list-error', 'Health', 'Task copies could not be listed', health), 'worktree.abandoned'),
  demo('execution-recorded', 'Conversation', 'Recorded effective execution settings', conversation + '&open=bot-edit&demo-execution', [click('Technical details')]),
  demo('execution-empty', 'Conversation', 'No recorded execution settings', conversation + '&open=bot-edit', [click('Technical details')]),
  fail(demo('execution-load-error', 'Conversation', 'Effective execution settings unavailable', conversation + '&open=bot-edit', [click('Technical details')]), 'exec.snapshot.latest'),
  demo('model-change-plan', 'Conversation', 'Plan a conversation model change', conversation, changeModel),
  fail(demo('model-change-plan-error', 'Conversation', 'Model change plan unavailable (choose a different model)', conversation, changeModel), 'session.model.plan', 'internal'),
  demo('manager-troubleshoot', 'Conversation', 'Ask the CEO to troubleshoot', conversation + '&demo-troubleshoot', [click('Ask the CEO to troubleshoot')]),
  fail(demo('manager-troubleshoot-error', 'Conversation', 'Troubleshooting could not start', conversation + '&demo-troubleshoot', [click('Ask the CEO to troubleshoot')]), 'manager.troubleshoot', 'internal'),
  demo('message-image-open', 'Conversation', 'Attached image and lightbox', conversation + '&demo-image=ready', [{ click: 'button[aria-label="Open invoice.png"]' }]),
  demo('message-image-missing', 'Conversation', 'Attached image is no longer available', conversation + '&demo-image=missing'),
  demo('owner-grant-live', 'Companion', 'Employee is controlling this computer', conversation + '&demo-owner-grant'),
  demo('owner-grant-stop', 'Companion', 'Stop employee control of this computer', conversation + '&demo-owner-grant', [{ click: '.owner-computer-stop' }]),
  fail(demo('owner-grant-stop-error', 'Companion', 'Stop request fails and the grant stays live', conversation + '&demo-owner-grant', [{ click: '.owner-computer-stop' }]), 'owner_computer.stop'),
  demo('provider-auth-clear', 'Accounts', 'Check sign-in again', settings('Agents', '&demo-auth'), [{ click: '.runtime-sign-in .sign-in-help-actions button' }, { click: '.runtime-sign-in .sign-in-help-actions button + button' }]),
  fail(demo('provider-auth-clear-error', 'Accounts', 'Sign-in recheck failed', settings('Agents', '&demo-auth'), [{ click: '.runtime-sign-in .sign-in-help-actions button' }, { click: '.runtime-sign-in .sign-in-help-actions button + button' }]), 'runtime.auth.clear', 'internal'),
  demo('launcher-empty', 'Accounts', 'No custom launchers', settings('Agents')),
  demo('launcher-saved', 'Accounts', 'Saved custom launcher (can remove)', launchers),
  demo('launcher-add', 'Accounts', 'Save a custom launcher', launchers, addLauncher),
  fail(demo('launcher-list-error', 'Accounts', 'Custom launchers could not load', launchers), 'runtime.profile.list', 'internal'),
  fail(demo('launcher-save-error', 'Accounts', 'Custom launcher could not be saved', launchers, addLauncher), 'runtime.profile.save', 'invalid_argument'),
  fail(demo('launcher-delete-error', 'Accounts', 'Remove launcher fails on confirmation', launchers, [click('Remove')]), 'runtime.profile.delete', 'internal'),
  demo('learned-pending', 'Skills', 'Learned skill awaiting review', skills),
  demo('learned-approved', 'Skills', 'Approve a learned skill', skills, [{ click: '.skills-learned .primary-button' }]),
  demo('learned-edit', 'Skills', 'Edit and approve a learned skill', skills, [{ click: '.skills-learned .secondary-button' }, click('Save and approve')]),
  demo('learned-forget', 'Skills', 'Forget a learned skill', skills, [click('Forget'), { click: '[role="dialog"] .danger-confirm-button' }]),
  fail(demo('learned-list-error', 'Skills', 'Learned skill review unavailable', skills), 'skills.learned.list'),
  fail(demo('learned-approve-error', 'Skills', 'Learned skill could not be approved', skills, [{ click: '.skills-learned .primary-button' }]), 'skills.learned.approve'),
  fail(demo('learned-edit-error', 'Skills', 'Learned skill could not be saved', skills, [{ click: '.skills-learned .secondary-button' }, click('Save and approve')]), 'skills.learned.edit'),
  fail(demo('learned-forget-error', 'Skills', 'Learned skill could not be forgotten', skills, [click('Forget'), { click: '[role="dialog"] .danger-confirm-button' }]), 'skills.learned.forget'),
  fail(demo('skills-load-error', 'Skills', 'Skills could not be loaded', skills), 'skills.list'),
  fail(demo('skills-detail-error', 'Skills', 'This skill could not be read', skills, [{ click: 'button[aria-label="Read invoice-tests"]' }]), 'skills.get'),
  fail(demo('connections-load-error', 'Remote', 'Connection settings could not be read', settings('Connections')), 'external.status'),
  fail(demo('approvals-list-error', 'Approvals', 'Saved approvals could not be read', settings('Approvals')), 'permission.rules.list'),
  fail(demo('approvals-delete-error', 'Approvals', 'Saved approval could not be removed (remove a rule)', settings('Approvals')), 'permission.rules.delete'),
  fail(demo('approvals-update-error', 'Approvals', 'Saved approval could not be changed (change the scope of a rule)', settings('Approvals')), 'permission.rules.update'),
  fail(demo('changes-repository-error', 'Session rail', 'Repository could not be read', conversation + '&open=changes'), 'session.repository'),
  fail(demo('changes-diff-error', 'Session rail', 'Diff could not be read (select a file)', conversation + '&open=changes'), 'session.git.diff'),
  fail(demo('changes-stage-error', 'Session rail', 'Changes could not be staged (stage a file)', conversation + '&open=changes'), 'session.git.stage'),
  fail(demo('changes-revert-error', 'Session rail', 'Changes could not be discarded (discard a file change)', conversation + '&open=changes'), 'session.git.revert'),
  fail(demo('skills-scan-error', 'Skills', 'Skills from coding agents could not be read (scan for skills)', skills), 'skills.scan'),
  fail(demo('connector-tools-error', 'Connectors', 'Connector tools could not be listed (open Tools)', connectors), 'connector.tools'),
  fail(demo('connector-rule-error', 'Connectors', 'Tool rule could not be saved (change a tool rule)', connectors), 'connector.tool.rule'),
  fail(demo('connector-test-error', 'Connectors', 'Connector could not be tested (test a connector)', connectors), 'connector.test'),
  fail(demo('connector-add-error', 'Connectors', 'Connector could not be added (submit the connector form)', connectors), 'connector.add'),
  fail(demo('vault-reveal-error', 'Vault', 'Vault value could not be revealed (reveal a field)', settings('Vault')), 'vault.reveal'),
  fail(demo('vault-save-error', 'Vault', 'Vault item could not be saved (add an item)', settings('Vault')), 'vault.create'),
  fail(demo('vault-request-error', 'Vault', 'Vault request could not be answered (answer the request)', conversation + '&vault=request'), 'vault.use.answer'),
  fail(demo('remote-push-setting-error', 'Remote', 'Host declined the notification setting (change the setting)', settings('Remote')), 'remote.push.set'),
  demo('backup-save-dialog-error', 'Backup', 'Backup save dialog could not open', settings('Backup', '&demo-backup=save-dialog'), [click('Export a backup')]),
  demo('backup-write-error', 'Backup', 'Backup could not be saved', settings('Backup', '&demo-backup=write'), [click('Export a backup')]),
  demo('template-write-error', 'Backup', 'Template could not be saved', settings('Backup', '&demo-backup=write'), [click('Export a template')]),
  demo('backup-file-dialog-error', 'Backup', 'Backup file dialog could not open', settings('Backup', '&demo-backup=open-dialog'), [click('Choose a backup')]),
  demo('backup-read-error', 'Backup', 'Backup could not be read', settings('Backup', '&demo-backup=read'), [click('Choose a backup')]),
  fail(demo('backup-restore-error', 'Backup', 'Backup could not be restored', settings('Backup'), [click('Choose a backup'), click('Restore')]), 'backup.import'),
  ...['unsupported', 'off-at-computer', 'blocked', 'off', 'on'].map((state) => demo(
    `push-${state}`, 'Remote', `Phone notifications: ${state.replaceAll('-', ' ')}`, `${phone}&demo-push=${state}`, [click('This device')],
  )),
  demo('push-subscribe', 'Remote', 'Turn phone notifications on', phone + '&demo-push=off', [click('This device'), click('Turn on')]),
  demo('push-unsubscribe', 'Remote', 'Turn phone notifications off', phone + '&demo-push=on', [click('This device'), click('Turn off')]),
  fail(demo('push-key-error', 'Remote', 'Phone notification configuration unavailable', phone + '&demo-push=off', [click('This device')]), 'push.key'),
  fail(demo('push-subscribe-error', 'Remote', 'Phone notifications could not be turned on', phone + '&demo-push=off', [click('This device'), click('Turn on')]), 'push.subscribe'),
  fail(demo('push-unsubscribe-error', 'Remote', 'Phone notifications could not be turned off', phone + '&demo-push=on', [click('This device'), click('Turn off')]), 'push.unsubscribe'),
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
  { id: 'haze-frame-strain', area: 'Main shell', label: 'Settings while the window is struggling (solid glass)', url: '/?no-intro&clean&open=settings:General&frame-strain' },
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

