// The working day and the control section. A new capability is a row here plus,
// if it needs one, a visual registered in components/sections/WorkingDay.tsx.
export type FeatureVisual = 'tasks' | 'schedules' | 'diffs' | 'terminal' | 'pullRequests' | 'delegation'

export interface Feature {
  title: string
  body: string
  visual: FeatureVisual
  /** Spans the full width of the grid. */
  wide?: boolean
}

export const features: Feature[] = [
  { title: 'A board for the work.', body: 'Line up tasks, then drag them onto an employee. See who’s busy, who’s free and what’s waiting for you.', visual: 'tasks', wide: true },
  { title: 'Work on a schedule.', body: 'Put recurring jobs on a timetable. Unattended work has its own limits and never runs in bypass.', visual: 'schedules' },
  { title: 'Employees delegate.', body: 'Ada can ask Fifi for a design, get the answer back, and carry on. You can read every exchange.', visual: 'delegation' },
  { title: 'Every change, turn by turn.', body: 'See exactly what each turn changed, and undo it if you don’t like it.', visual: 'diffs', wide: true },
  { title: 'A terminal beside the work.', body: 'Run a command next to the conversation without leaving the app.', visual: 'terminal' },
  { title: 'Pull requests from the same place.', body: 'When the work is good, open a pull request right there.', visual: 'pullRequests' },
]

export type PermissionMode = 'Ask first' | 'Accept edits' | 'Auto'

export const permissionModes: { mode: PermissionMode; detail: string }[] = [
  { mode: 'Ask first', detail: 'You approve each command and file change.' },
  { mode: 'Accept edits', detail: 'Reading and editing files inside the project run. Commands still ask.' },
  { mode: 'Auto', detail: 'Low-risk actions inside the project run. Anything else asks.' },
]

export const permissionRows: { id: string; mode: PermissionMode }[] = [
  { id: 'ada', mode: 'Accept edits' },
  { id: 'nubi', mode: 'Ask first' },
  { id: 'volt', mode: 'Ask first' },
  { id: 'will', mode: 'Auto' },
]

export const usageRows = [
  { agent: 'Claude Code', used: 44 },
  { agent: 'Codex', used: 18 },
  { agent: 'OpenCode', used: 62 },
]

export const principles = [
  { title: 'Local first', body: 'Retinue runs on your own machine and drives the coding agents you’ve already installed.' },
  { title: 'Your sign-ins stay yours', body: 'It uses the agents you’ve signed in to. It doesn’t copy your provider accounts.' },
  { title: 'No unattended bypass', body: 'Scheduled and unattended work never runs in bypass mode, whatever else you’ve set.' },
]
