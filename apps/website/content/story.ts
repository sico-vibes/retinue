// The company story: the org chart's acts, the example company and the idea-to-shipped stages.

export interface Act {
  title: string
  body: string
}

export const acts: Act[] = [
  { title: 'You own the company.', body: 'You decide what gets built. Everyone in Retinue ultimately answers to you.' },
  { title: 'The CEO runs it.', body: 'Describe a project. The CEO interviews you, recommends whom to hire, and proposes a new specialist when nobody fits.' },
  { title: 'Projects are the org chart.', body: 'Each project has its own team, so you can see who is on what at a glance.' },
  { title: 'Hire specialists in.', body: 'Twenty-eight presets across seven departments, each with a name, a role, skills and a face.' },
  { title: 'One employee, many projects.', body: 'Nubi reviews all three. Hire someone once and put them wherever they’re needed.' },
  { title: 'Everyone has their conversations.', body: 'Ada has three going at once. Each one keeps its own history, so the work is there when you open it.' },
]

export interface ProjectTeam {
  name: string
  /** Rendered top to bottom. The last person lines up across all teams. */
  team: string[]
}

export const orgProjects: ProjectTeam[] = [
  { name: 'Storefront', team: ['ada', 'fifi', 'jo', 'nubi'] },
  { name: 'Docs site', team: ['will', 'cleo', 'ludo', 'nubi'] },
  { name: 'Mobile app', team: ['nova', 'lan', 'wu', 'nubi'] },
]

export const adaConversations = [
  { title: 'Checkout summary', state: 'Needs you', tone: 'amber' as const },
  { title: 'Invoice parser tests', state: 'Working', tone: 'amber' as const },
  { title: 'Payment retries', state: 'Idle', tone: 'muted' as const },
]

export type StageId = 'interview' | 'hire' | 'plan' | 'build' | 'ship'

export interface Stage {
  id: StageId
  label: string
  title: string
  body: string
}

export const stages: Stage[] = [
  { id: 'interview', label: 'Interview', title: 'It starts with a conversation.', body: 'You say what you want. The CEO asks what it needs to know.' },
  { id: 'hire', label: 'Hire', title: 'The right people join.', body: 'You approve the team, and each new employee joins the project.' },
  { id: 'plan', label: 'Plan', title: 'Work moves across the board.', body: 'Tasks get an owner and move on their own as the work gets done.' },
  { id: 'build', label: 'Build', title: 'Every change lands in the open.', body: 'See the diff for each turn, and undo it if it isn’t right.' },
  { id: 'ship', label: 'Ship', title: 'A pull request, ready for you.', body: 'A reviewer signs off, and you make the final call.' },
]
