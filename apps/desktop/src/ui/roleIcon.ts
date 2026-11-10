// The icon that stands for an employee's job (owner, 9 November 2026): drawn large and faded
// behind the employee page's header. Matched from the role text, so custom roles work too.
import { Accessibility, Activity, ArrowRightLeft, Blocks, BriefcaseBusiness, Bug, Cable, ChartLine, ClipboardCheck, Compass, Container, Crown, Database, FlaskConical, Gauge, GitPullRequest, Layers, Lightbulb, Megaphone, Package, Palette, PanelsTopLeft, PenLine, Rocket, Server, ShieldCheck, Siren, Smartphone, SquareKanban, Telescope, type LucideIcon } from 'lucide-react'

const RULES: ReadonlyArray<[RegExp, LucideIcon]> = [
  [/\b(ceo|chief executive|manager of the team)\b/i, Crown],
  [/accessib/i, Accessibility],
  [/secur/i, ShieldCheck],
  [/load|stress/i, Activity],
  [/perform|profil|speed/i, Gauge],
  [/debug/i, Bug],
  [/review/i, GitPullRequest],
  [/\bqa\b|usabil/i, ClipboardCheck],
  [/test/i, FlaskConical],
  [/on-call|incident/i, Siren],
  [/release/i, Rocket],
  [/devops|infra|cloud|pipeline/i, Container],
  [/database|\bdba\b|sql/i, Database],
  [/migration|data engineer/i, ArrowRightLeft],
  [/data scien|analy/i, ChartLine],
  [/depend|licen/i, Package],
  [/api|integration/i, Cable],
  [/architect/i, Blocks],
  [/mobile|ios|android/i, Smartphone],
  [/full-?stack|generalist/i, Layers],
  [/front-?end/i, PanelsTopLeft],
  [/back-?end|server/i, Server],
  [/design/i, Palette],
  [/product/i, Lightbulb],
  [/project manag|planning|scrum/i, SquareKanban],
  [/research/i, Telescope],
  [/lead|cto/i, Compass],
  [/copy|docs|writ/i, PenLine],
  [/market|growth|social/i, Megaphone],
]

export function roleIcon(role: string | null | undefined, manager = false): LucideIcon {
  if (manager) return Crown
  const text = (role ?? '').trim()
  return RULES.find(([pattern]) => pattern.test(text))?.[1] ?? BriefcaseBusiness
}
