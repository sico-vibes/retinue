// The preset specialists, from the app's own list. `agent` is only the example badge shown on the site:
// in the app you choose which coding agent each employee runs on.
export type AgentId = 'claude' | 'codex' | 'opencode'
export type DepartmentId = 'engineering' | 'infrastructure' | 'data' | 'quality' | 'design' | 'product' | 'growth'

export interface Employee {
  id: string
  name: string
  role: string
  department: DepartmentId
  summary: string
  skills: string[]
  photo: boolean
  agent: AgentId
}

export const departments: { id: DepartmentId; name: string }[] = [
  { id: "engineering", name: "Engineering" },
  { id: "infrastructure", name: "Infrastructure" },
  { id: "data", name: "Data" },
  { id: "quality", name: "Quality and security" },
  { id: "design", name: "Design" },
  { id: "product", name: "Product and planning" },
  { id: "growth", name: "Writing and growth" },
]

export const employees: Employee[] = [
  { id: "ada", name: "Ada", role: "Backend engineer", department: "engineering", summary: "Builds and changes server code, APIs and business logic, with tests.", skills: ["backend","api","server","database"], photo: true, agent: "codex" },
  { id: "wolfie", name: "Wolfie", role: "Frontend engineer", department: "engineering", summary: "Builds user interfaces: components, styles, state and interaction.", skills: ["frontend","ui","react","css"], photo: false, agent: "claude" },
  { id: "wu", name: "Wu", role: "Full-stack generalist", department: "engineering", summary: "Takes a feature end to end, from data to screen, and gets it working fast.", skills: ["fullstack","prototype","generalist"], photo: true, agent: "opencode" },
  { id: "nova", name: "Nova", role: "Mobile engineer", department: "engineering", summary: "Builds and fixes mobile apps, with care for devices, offline use and app stores.", skills: ["mobile","ios","android","react-native"], photo: true, agent: "codex" },
  { id: "bert", name: "Bert", role: "Software architect", department: "engineering", summary: "Designs how a system fits together and plans large changes before they start.", skills: ["architecture","design","refactor","planning"], photo: true, agent: "claude" },
  { id: "chief", name: "Chief", role: "APIs and integrations", department: "engineering", summary: "Connects the project to outside services: APIs, webhooks, SDKs and auth flows.", skills: ["integration","api","webhooks","sdk"], photo: true, agent: "opencode" },
  { id: "volt", name: "Volt", role: "DevOps and infrastructure", department: "infrastructure", summary: "Builds pipelines, containers and cloud setup so the project builds and deploys reliably.", skills: ["devops","ci","docker","cloud","infra"], photo: true, agent: "opencode" },
  { id: "amy", name: "Amy", role: "Release manager", department: "infrastructure", summary: "Prepares releases: versions, changelogs, checks and the steps to ship safely.", skills: ["release","versioning","changelog"], photo: true, agent: "claude" },
  { id: "ace", name: "Ace", role: "On-call and incidents", department: "infrastructure", summary: "Responds when something breaks in production: finds the cause, stops the damage, writes it up.", skills: ["incident","monitoring","hotfix","logs"], photo: true, agent: "opencode" },
  { id: "zip", name: "Zip", role: "Performance engineer", department: "infrastructure", summary: "Makes things faster and lighter, guided by measurements rather than guesses.", skills: ["performance","profiling","speed","memory"], photo: true, agent: "codex" },
  { id: "mira", name: "Mira", role: "Data scientist", department: "data", summary: "Analyses data, runs experiments and explains what the numbers really say.", skills: ["data","analysis","statistics","python"], photo: true, agent: "claude" },
  { id: "vlad", name: "Vlad", role: "Database administrator", department: "data", summary: "Designs schemas, writes safe migrations and keeps queries fast.", skills: ["database","sql","schema","indexes"], photo: true, agent: "opencode" },
  { id: "art", name: "Art", role: "Data engineer and migrations", department: "data", summary: "Moves and reshapes data: imports, exports, pipelines and one-off migrations.", skills: ["etl","pipelines","migration","import"], photo: true, agent: "codex" },
  { id: "sleuth", name: "Sleuth", role: "Debugger", department: "quality", summary: "Finds the real cause of a bug from the evidence, then fixes it properly.", skills: ["debugging","bugs","root-cause"], photo: true, agent: "claude" },
  { id: "nubi", name: "Nubi", role: "Code reviewer", department: "quality", summary: "Reviews changes for correctness, readability and risk before they are merged.", skills: ["review","quality","readability"], photo: true, agent: "codex" },
  { id: "lan", name: "Lan", role: "Test engineer", department: "quality", summary: "Writes tests that pin real behaviour and catch regressions.", skills: ["testing","unit-tests","e2e","coverage"], photo: true, agent: "codex" },
  { id: "charlie", name: "Charlie", role: "QA and usability tester", department: "quality", summary: "Uses the product like a real person and reports what is confusing or broken.", skills: ["qa","usability","manual-testing"], photo: true, agent: "claude" },
  { id: "brute", name: "Brute", role: "Load and stress tester", department: "quality", summary: "Pushes the system to its limits to find where it breaks before users do.", skills: ["load-testing","stress","limits"], photo: true, agent: "opencode" },
  { id: "jo", name: "Jo", role: "Security engineer", department: "quality", summary: "Finds and fixes security weaknesses: auth, input handling, secrets and dependencies.", skills: ["security","auth","vulnerabilities","audit"], photo: true, agent: "codex" },
  { id: "robin", name: "Robin", role: "Dependencies and licences", department: "quality", summary: "Keeps dependencies up to date, lean and legally safe to use.", skills: ["dependencies","licences","updates","supply-chain"], photo: true, agent: "claude" },
  { id: "fifi", name: "Fifi", role: "UI designer", department: "design", summary: "Shapes how screens look and feel: layout, type, colour, spacing and motion.", skills: ["design","ui","visual","layout"], photo: true, agent: "opencode" },
  { id: "ludo", name: "Ludo", role: "Accessibility specialist", department: "design", summary: "Makes the product usable for everyone: keyboard, screen readers, contrast and motion.", skills: ["accessibility","a11y","keyboard","screen-reader"], photo: true, agent: "codex" },
  { id: "ben", name: "Ben", role: "Product manager", department: "product", summary: "Turns ideas into clear requirements and decides what matters most.", skills: ["product","requirements","priorities","roadmap"], photo: true, agent: "claude" },
  { id: "bess", name: "Bess", role: "Project manager", department: "product", summary: "Plans the work, tracks progress and keeps everyone on the team in step.", skills: ["planning","tracking","coordination","status"], photo: true, agent: "opencode" },
  { id: "grey", name: "Grey", role: "Research and planning", department: "product", summary: "Researches options and unknowns, then recommends a path with reasons.", skills: ["research","planning","options","investigation"], photo: true, agent: "codex" },
  { id: "thun", name: "Thun", role: "Technical lead", department: "product", summary: "Sets technical direction for the project and keeps the codebase healthy.", skills: ["tech-lead","standards","mentoring","decisions"], photo: false, agent: "claude" },
  { id: "will", name: "Will", role: "Copy and docs", department: "growth", summary: "Writes clear docs, guides, interface copy and release notes.", skills: ["docs","copy","writing","readme"], photo: true, agent: "claude" },
  { id: "cleo", name: "Cleo", role: "Marketing and growth", department: "growth", summary: "Plans launches, writes marketing copy and finds ways to reach more users.", skills: ["marketing","growth","seo","launch"], photo: true, agent: "codex" },
]

const byId = new Map(employees.map((employee) => [employee.id, employee]))

export function employee(id: string): Employee {
  const found = byId.get(id)
  if (!found) throw new Error(`Unknown employee: ${id}`)
  return found
}

export const inDepartment = (id: DepartmentId) => employees.filter((person) => person.department === id)
export const agentNames: Record<AgentId, string> = { claude: 'Claude Code', codex: 'Codex', opencode: 'OpenCode' }
