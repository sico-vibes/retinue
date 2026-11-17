// A bot's task list for the turn (owner, 14 November 2026): what it plans to do,
// updated live as it works, like the list Claude Code keeps. The marks follow
// React Bits' Status Mark (MIT + Commons Clause, © David Haz), redrawn in CSS
// so Retinue needs no animation library: a dashed ring waits, an arc turns while
// the step runs, a tick draws itself and the text is struck through when done.
import './turnPlan.css'

export type PlanStatus = 'pending' | 'in_progress' | 'completed'
export type PlanItem = { text: string; status: PlanStatus }
export type TurnPlanData = { items: PlanItem[]; updatedAt?: string }

export function planOf(value: unknown): TurnPlanData | null {
  if (!value || typeof value !== 'object') return null
  const raw = (value as { items?: unknown }).items
  if (!Array.isArray(raw)) return null
  const items = raw.flatMap((item): PlanItem[] => {
    if (!item || typeof item !== 'object') return []
    const row = item as Record<string, unknown>
    const text = typeof row.text === 'string' ? row.text.trim() : typeof row.content === 'string' ? row.content.trim() : ''
    if (!text) return []
    const status = row.status === 'completed' || row.status === 'done' ? 'completed' : row.status === 'in_progress' || row.status === 'running' || row.status === 'inProgress' ? 'in_progress' : 'pending'
    return [{ text, status }]
  })
  return items.length ? { items, updatedAt: typeof (value as { updatedAt?: unknown }).updatedAt === 'string' ? (value as { updatedAt: string }).updatedAt : undefined } : null
}

export function StatusMark({ status }: { status: PlanStatus | 'failed' }) {
  return <svg className="plan-mark" data-status={status} viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" focusable="false">
    <circle className="plan-mark-ring" cx="12" cy="12" r="9" pathLength="100" />
    {status === 'completed' && <path className="plan-mark-glyph" d="M7.5 12.25 10.5 15.25 16.75 8.75" pathLength="100" />}
    {status === 'failed' && <path className="plan-mark-glyph" d="M8.5 8.5 15.5 15.5M15.5 8.5 8.5 15.5" pathLength="100" />}
  </svg>
}

const SPOKEN: Record<PlanStatus, string> = { pending: 'to do', in_progress: 'in progress', completed: 'done' }

/** The list itself. `stopped` marks a turn that ended with steps still open: they stay as they were, not spinning. */
export function TurnPlan({ plan, stopped = false, earlier = false }: { plan: TurnPlanData; stopped?: boolean; earlier?: boolean }) {
  const done = plan.items.filter((item) => item.status === 'completed').length
  return <section className={`turn-plan${earlier ? ' earlier' : ''}`} aria-label={`Task list${earlier ? ' from an earlier turn' : ''}, ${done} of ${plan.items.length} done`}>
    {earlier && <p className="turn-plan-earlier">From an earlier turn</p>}
    <ol>
      {plan.items.map((item, index) => {
        const status: PlanStatus = stopped && item.status === 'in_progress' ? 'pending' : item.status
        return <li key={`${index}-${item.text}`} data-status={status}>
          <StatusMark status={status} />
          <span className="turn-plan-text">{item.text}<span className="sr-only">, {SPOKEN[status]}</span></span>
        </li>
      })}
    </ol>
  </section>
}
