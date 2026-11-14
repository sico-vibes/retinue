// Every word and link on the site. Sections read from here. Short and specific: one line beats three.

export const site = {
  name: 'Retinue',
  version: '0.2 beta',
  download: 'https://github.com/sico-vibes/retinue/releases',
  github: 'https://github.com/sico-vibes/retinue',
  issues: 'https://github.com/sico-vibes/retinue/issues',
}

export const nav = [
  { label: 'CEO', href: '/#ceo' },
  { label: 'Team', href: '/team/' },
  { label: 'Anywhere', href: '/#anywhere' },
  { label: 'Docs', href: '/docs/' },
  { label: 'Changelog', href: '/changelog/' },
]

export const providers = [
  { name: 'Claude Code', logo: '/providers/claude.svg' },
  { name: 'Codex', logo: '/providers/codex-dark.svg', light: '/providers/codex-light.svg' },
  { name: 'OpenCode', logo: '/providers/opencode-dark.svg', light: '/providers/opencode-light.svg' },
  { name: 'Cursor', logo: '/providers/cursor.png' },
  { name: 'Qwen Code', logo: '/providers/qwen.png' },
  { name: 'Grok Build', logo: '/providers/grok.png' },
  { name: 'Antigravity', logo: '/providers/antigravity.png' },
]

export type Feature = { tag: string; title: string; body: string; shot: string; prop?: string }

export const workday: Feature[] = [
  { tag: 'Queue', title: 'Line up the next message', body: 'Keep typing while they work. Send now steers the running turn.', shot: 'queue', prop: '/art/props/sticky-notes.webp' },
  { tag: 'Approvals', title: 'Nothing runs without you', body: 'Commands and edits wait for one keypress.', shot: 'approve', prop: '/art/objects/approval.webp' },
  { tag: 'Review', title: 'Read every change', body: 'Diffs per turn, branch or commit. Then the pull request.', shot: 'review', prop: '/art/objects/review.webp' },
  { tag: 'Compare', title: 'Two employees, one task', body: 'Send the same brief to several. Keep the best result.', shot: 'compare', prop: '/art/props/papers.webp' },
  { tag: 'Schedules', title: 'Work that runs on time', body: 'Daily reports, weekly upgrades, nightly checks.', shot: 'schedules', prop: '/art/props/calendar.webp' },
  { tag: 'Usage limits', title: 'Picks up after the reset', body: 'Hit a limit and the conversation resumes on its own.', shot: 'main', prop: '/art/props/clock.webp' },
]

export const traits = [
  { title: 'Qualified on day one', body: 'Every hire comes with the skills for the role: test-first, debugging, review, security and more.' },
  { title: 'A personality you choose', body: 'Steady, blunt, warm or a mentor. It shapes how they talk, never what they can do.' },
  { title: 'The model that fits', body: 'Pick the coding agent and model per employee: Claude, GPT or any OpenCode model.' },
  { title: 'Memory that carries over', body: 'They remember your projects, decisions and preferences between conversations.' },
]

export const anywhere = [
  { tag: 'Linked machines', title: 'Your PC at home, from the laptop', body: 'Pair machines over Tailscale. Start, steer or take over their work from one window.', shot: 'machines' },
  { tag: 'Cloud computer', title: 'A computer of their own', body: 'Link your server. They clone, build and run routines there. You watch or take control.', shot: 'computer' },
  { tag: 'Phone and web', title: 'Answer from anywhere', body: 'Install the web app on your phone. Approve, reply and check progress.', shot: 'main-phone' },
  { tag: 'Companion', title: 'Always one glance away', body: 'A small floating window shows who is working and who needs you.', shot: 'companion' },
]

export const connect = [
  { title: 'Retinue MCP', body: 'Connect Claude Code to your team. Brief employees from any terminal.', code: 'claude mcp add retinue -- retinue-mcp.exe mcp-server' },
  { title: 'Dictation', body: 'Speak instead of typing. On-device models by default, cloud when you choose.' },
  { title: 'Seven coding agents', body: 'Claude Code, Codex and OpenCode built in. Cursor, Qwen, Grok and Antigravity behind a switch.' },
]

export const privacy = [
  { title: 'No account', body: 'Download and run. There is nothing to sign up for.' },
  { title: 'No data collection', body: 'Your code, conversations and settings stay on your computer.' },
  { title: 'Your own sign-ins', body: 'Each coding agent signs in its own way. Retinue never copies them.' },
  { title: 'Private by network', body: 'Remote access runs over your own Tailscale, device by device.' },
]

export const next = ['Autonomous projects that plan their own week', 'Routines that run across machines', 'Employees that learn your codebase overnight', 'More coding agents and models']
