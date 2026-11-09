import { Avatar } from '@/components/ui/Avatar'
import { Shot } from '@/components/ui/ProductFrame'
import type { FeatureVisual } from '@/content/features'

// Illustrations for the working-day cards. Example values, drawn in the Haze look.

function TerminalVisual() {
  return (
    <div className="grid h-full min-h-[240px] content-center gap-1.5 p-6 font-mono text-[13px] leading-relaxed">
      <p><span className="text-ink-3">PS C:\shop&gt;</span> npm test -- checkout</p>
      <p className="text-ink-2"><span className="text-green">✓</span> src/checkout.test.ts (14)</p>
      <p className="text-ink-2"><span className="text-green">✓</span> src/cart.test.ts (9)</p>
      <p><span className="text-ink-3">Tests</span> 23 passed (23)</p>
    </div>
  )
}

const schedule = [
  { job: 'Dependency check', who: 'robin', when: 'Mondays, 09:00' },
  { job: 'Release notes', who: 'will', when: 'Fridays, 16:30' },
  { job: 'Flaky test sweep', who: 'lan', when: 'Every night' },
]

function SchedulesVisual() {
  return (
    <ul className="grid h-full min-h-[240px] content-center gap-2 p-5">
      {schedule.map((row) => (
        <li key={row.job} className="flex items-center gap-3 rounded-xl bg-well px-3 py-2.5">
          <Avatar id={row.who} size={32} badge={false} />
          <span className="min-w-0 flex-1 truncate text-sm font-medium">{row.job}</span>
          <span className="shrink-0 text-[13px] text-ink-3">{row.when}</span>
        </li>
      ))}
    </ul>
  )
}

function DelegationVisual() {
  return (
    <div className="grid h-full min-h-[240px] content-center gap-3 p-5 text-sm">
      <div className="flex items-start gap-3">
        <Avatar id="ada" size={34} />
        <p className="rounded-[16px] bg-raised px-3.5 py-2 leading-snug">Fifi, can you find a good pattern for the checkout summary?</p>
      </div>
      <p className="flex items-center justify-center gap-2 text-[13px] text-ink-3"><Avatar id="fifi" size={20} badge={false} /> Fifi is on it</p>
      <div className="flex flex-row-reverse items-start gap-3">
        <Avatar id="fifi" size={34} />
        <p className="rounded-[16px] bg-raised px-3.5 py-2 leading-snug">Found two. I’d keep the totals in view while people fill in the form.</p>
      </div>
    </div>
  )
}

function PullRequestVisual() {
  return (
    <div className="grid h-full min-h-[240px] content-center p-5">
      <div className="rounded-xl bg-well p-4">
        <p className="text-[13px] font-medium text-green">Ready to open</p>
        <p className="mt-1 font-medium tracking-[-0.01em]">Tidy the checkout summary</p>
        <p className="mt-0.5 font-mono text-xs text-ink-3">feature/summary → main · 6 files</p>
        <span className="mt-3.5 inline-block rounded-[10px] bg-button px-3.5 py-2 text-[13px] font-medium text-button-ink">Create pull request</span>
      </div>
    </div>
  )
}

function TasksVisual() {
  return <Shot name="tasks" width={1600} height={952} sizes="(min-width: 1280px) 1200px, 96vw" alt="The task board: unassigned tasks on the left, and each employee on the right with what they’re working on." />
}

function DiffsVisual() {
  return <Shot name="review" width={1500} height={673} sizes="(min-width: 1280px) 1200px, 96vw" alt="A coloured diff of a checkout function, with buttons to stage or discard it and to create a pull request." />
}

export const featureVisuals: Record<FeatureVisual, () => React.ReactNode> = {
  tasks: TasksVisual,
  schedules: SchedulesVisual,
  diffs: DiffsVisual,
  terminal: TerminalVisual,
  pullRequests: PullRequestVisual,
  delegation: DelegationVisual,
}
