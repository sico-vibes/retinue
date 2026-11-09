'use client'

import { m } from 'motion/react'
import Image from 'next/image'
import { useEffect, useState, type ReactNode } from 'react'
import { Avatar } from '@/components/ui/Avatar'
import { easeHaze } from '@/components/ui/Motion'
import { StreamText } from '@/components/ui/Reveal'
import { employee, type AgentId } from '@/content/employees'
import type { StageId } from '@/content/story'

// The five scenes of "from idea to shipped". Each plays when `active` turns on.
interface SceneProps { active: boolean }

const fade = (active: boolean, delay = 0) => ({ initial: false as const, animate: { opacity: active ? 1 : 0, y: active ? 0 : 10 }, transition: { duration: 0.45, delay: active ? delay : 0, ease: easeHaze } })

function Bubble({ children }: { children: ReactNode }) {
  return <p className="ml-auto w-fit max-w-[80%] rounded-[18px] bg-raised px-4 py-2.5 text-sm sm:text-[15px]">{children}</p>
}

function CeoMark() {
  return (
    <span className="relative mt-0.5 block h-8 w-8 shrink-0 overflow-hidden rounded-full shadow-[0_0_0_1px_var(--line-strong)]">
      <Image unoptimized src="/brand/retinue-tile-dark.webp" alt="" fill className="only-dark object-cover" />
      <Image unoptimized src="/brand/retinue-tile-light.webp" alt="" fill className="only-light object-cover" />
    </span>
  )
}

function InterviewScene({ active }: SceneProps) {
  return (
    <div className="flex h-full flex-col justify-center gap-4 p-5 sm:p-10">
      <m.div {...fade(active, 0.1)}><Bubble>I want to sell my pottery online.</Bubble></m.div>
      <div className="flex gap-3">
        <CeoMark />
        <p className="pt-1 text-sm leading-relaxed sm:text-base"><StreamText active={active} delay={0.5} text="Happy to help. Do you have a store platform in mind, and will you take payments yourself?" /></p>
      </div>
      <m.div {...fade(active, 1.7)}><Bubble>No platform yet. Stripe for payments, and I want it to feel calm and handmade.</Bubble></m.div>
      <div className="flex gap-3">
        <CeoMark />
        <p className="pt-1 text-sm leading-relaxed sm:text-base"><StreamText active={active} delay={2.4} text="Got it. I have a team in mind." /></p>
      </div>
    </div>
  )
}

const hires: { id: string; note: string }[] = [
  { id: 'ada', note: 'Backend and payments' },
  { id: 'fifi', note: 'Pages that feel handmade' },
  { id: 'nubi', note: 'Reviews every change' },
]

function HireScene({ active }: SceneProps) {
  return (
    <div className="grid h-full content-center gap-3 p-5 [perspective:1200px] sm:grid-cols-3 sm:gap-4 sm:p-10">
      {hires.map((hire, index) => {
        const person = employee(hire.id)
        return (
          <m.div key={hire.id} initial={false} animate={{ opacity: active ? 1 : 0, rotateY: active ? 0 : -90 }} transition={{ duration: 0.7, delay: active ? 0.15 + index * 0.28 : 0, ease: easeHaze }} className="flex items-center gap-3.5 rounded-2xl border border-line-strong bg-raised p-4 [backface-visibility:hidden] sm:flex-col sm:p-6 sm:text-center">
            <Avatar id={hire.id} size={72} />
            <div>
              <p className="text-lg font-medium sm:mt-1">{person.name}</p>
              <p className="text-sm text-ink-3">{person.role}</p>
              <p className="mt-1 text-sm text-ink-2">{hire.note}</p>
            </div>
            <m.span {...fade(active, 0.9 + index * 0.28)} className="ml-auto rounded-md bg-well px-2 py-1 text-xs font-medium text-green sm:mx-auto sm:mt-2">Hired</m.span>
          </m.div>
        )
      })}
    </div>
  )
}

const columns = ['To do', 'Working', 'In review', 'Done']
const tasks: { title: string; who: string; offset: number }[] = [
  { title: 'Product page', who: 'fifi', offset: 0 },
  { title: 'Stripe checkout', who: 'ada', offset: 1 },
  { title: 'Review checkout', who: 'nubi', offset: 2 },
]

function BoardScene({ active }: SceneProps) {
  const [tick, setTick] = useState(0)
  useEffect(() => {
    if (!active) return
    const timer = setInterval(() => setTick((value) => value + 1), 1050)
    return () => { clearInterval(timer); setTick(0) }
  }, [active])
  return (
    <div className="flex h-full flex-col justify-center p-4 sm:p-10">
      <div className="grid grid-cols-4 gap-1.5 sm:gap-3">
        {columns.map((column) => (
          <div key={column} className="rounded-xl bg-well p-1.5 sm:p-2.5">
            <p className="mb-1.5 truncate px-1 text-[11px] font-medium text-ink-3 sm:text-[13px]">{column}</p>
            <div className="h-[186px] sm:h-[250px]" />
          </div>
        ))}
      </div>
      <div className="pointer-events-none relative -mt-[186px] grid grid-cols-4 gap-1.5 px-1.5 sm:-mt-[250px] sm:gap-3 sm:px-2.5" style={{ marginBottom: 0 }}>
        {tasks.map((task, index) => {
          const column = Math.min(3, Math.max(0, tick - task.offset))
          return (
            <m.div
              key={task.title}
              initial={false}
              animate={{ opacity: active ? 1 : 0, x: `calc(${column * 100}% + ${column} * var(--board-gap))` }}
              transition={{ duration: 0.65, ease: easeHaze }}
              className="relative col-start-1 flex flex-col gap-1.5 rounded-lg border border-line-strong bg-glass-solid p-1.5 [--board-gap:0.75rem] max-sm:[--board-gap:0.375rem] sm:p-2.5"
              style={{ gridRow: index + 1, marginTop: index === 0 ? 0 : 6 }}
            >
              <p className="text-[11px] leading-tight font-medium sm:text-sm">{task.title}</p>
              <Avatar id={task.who} size={26} badge={false} />
            </m.div>
          )
        })}
      </div>
    </div>
  )
}

const diff: { kind: 'keep' | 'del' | 'add'; text: string }[] = [
  { kind: 'keep', text: 'export async function chargeOrder(order: Order) {' },
  { kind: 'keep', text: '  const total = sumLines(order.lines)' },
  { kind: 'del', text: '  const intent = await stripe.paymentIntents.create({ amount: total })' },
  { kind: 'add', text: '  if (total <= 0) throw new CheckoutError("empty_order")' },
  { kind: 'add', text: '  const intent = await stripe.paymentIntents.create(' },
  { kind: 'add', text: '    { amount: total, metadata: { orderId: order.id } },' },
  { kind: 'add', text: '    { idempotencyKey: `order-${order.id}` },' },
  { kind: 'add', text: '  )' },
  { kind: 'keep', text: '  return intent.id' },
  { kind: 'keep', text: '}' },
]

const diffTone = { keep: 'text-ink-2', del: 'bg-[var(--diff-del)] text-red', add: 'bg-[var(--diff-add)] text-green' }
const diffSign = { keep: ' ', del: '-', add: '+' }

function DiffScene({ active }: SceneProps) {
  const [undone, setUndone] = useState(false)
  const lines = undone ? diff.filter((line) => line.kind !== 'add').map((line) => (line.kind === 'del' ? { ...line, kind: 'keep' as const } : line)) : diff
  return (
    <div className="flex h-full flex-col justify-center p-4 sm:p-10">
      <div className="overflow-hidden rounded-xl border border-line bg-well">
        <div className="flex items-center gap-3 border-b border-line px-3.5 py-2.5 text-[13px]">
          <span className="font-mono text-ink-2">src/checkout.ts</span>
          {!undone && <span className="font-mono text-green">+5</span>}
          {!undone && <span className="font-mono text-red">-1</span>}
          <button type="button" tabIndex={active ? 0 : -1} onClick={() => setUndone((value) => !value)} className="ml-auto min-h-8 rounded-lg border border-line-strong px-3 text-xs font-medium text-ink hover:bg-raised">{undone ? 'Redo' : 'Undo this turn'}</button>
        </div>
        <pre className="overflow-hidden py-2 font-mono text-[10.5px] leading-[1.7] sm:text-[13px]">
          {lines.map((line, index) => (
            <m.div key={`${line.text}-${undone}`} initial={false} animate={{ opacity: active ? 1 : 0 }} transition={{ duration: 0.3, delay: active ? 0.1 + index * 0.09 : 0 }} className={`truncate px-3.5 ${diffTone[line.kind]}`}>
              {diffSign[line.kind]} {line.text}
            </m.div>
          ))}
        </pre>
      </div>
    </div>
  )
}

const checks = ['Type check', 'Unit tests', 'Build']

function ShipScene({ active }: SceneProps) {
  const reviewer: { id: string; agent: AgentId } = { id: 'nubi', agent: 'codex' }
  return (
    <div className="flex h-full flex-col justify-center p-5 sm:p-10">
      <div className="mx-auto w-full max-w-[620px] rounded-2xl border border-line-strong bg-raised p-5 sm:p-7">
        <m.p {...fade(active, 0.1)} className="flex items-center gap-2 text-sm font-medium text-green">
          <svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><circle cx="4" cy="3.5" r="1.7" /><circle cx="4" cy="12.5" r="1.7" /><circle cx="12" cy="12.5" r="1.7" /><path d="M4 5.2v5.6M12 10.8V6.5A2 2 0 0 0 10 4.5H8" /></svg>
          Pull request opened
        </m.p>
        <m.h4 {...fade(active, 0.25)} className="mt-2 text-xl font-medium tracking-[-0.02em] sm:text-2xl">Add Stripe checkout with idempotent charges</m.h4>
        <m.p {...fade(active, 0.35)} className="mt-1 font-mono text-[13px] text-ink-3">feature/checkout → main</m.p>
        <ul className="mt-5 grid gap-2">
          {checks.map((check, index) => (
            <m.li key={check} {...fade(active, 0.6 + index * 0.32)} className="flex items-center justify-between rounded-lg bg-well px-3 py-2 text-sm">
              {check}
              <span className="text-[13px] font-medium text-green">Passed</span>
            </m.li>
          ))}
        </ul>
        <m.div {...fade(active, 1.7)} className="mt-4 flex items-center gap-3 border-t border-line pt-4">
          <Avatar id={reviewer.id} size={38} />
          <p className="text-sm"><span className="font-medium">Nubi</span> <span className="text-ink-2">approved these changes</span></p>
          <span className="ml-auto rounded-[10px] bg-button px-3.5 py-2 text-[13px] font-medium text-button-ink">Merge</span>
        </m.div>
      </div>
    </div>
  )
}

export const scenes: Record<StageId, (props: SceneProps) => ReactNode> = {
  interview: InterviewScene,
  hire: HireScene,
  plan: BoardScene,
  build: DiffScene,
  ship: ShipScene,
}
