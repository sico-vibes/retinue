'use client'

import { m, useTransform, type MotionValue } from 'motion/react'
import Image from 'next/image'
import { useState } from 'react'
import { employee as findEmployee } from '@/content/employees'
import { adaConversations, orgProjects } from '@/content/story'
import { Avatar } from './Avatar'
import { EmployeePeek } from './EmployeeCard'

/** Scroll position (0 to 1) where each part of the chart appears. */
export const ACT_STARTS = [0, 0.12, 0.3, 0.46, 0.66, 0.82]
/** A progress value with everyone present and nobody highlighted: the static, reduced-motion state. */
export const SETTLED_PROGRESS = 0.65

const NUBI_START = ACT_STARTS[4]
const ADA_START = ACT_STARTS[5]
const DIMMED = 0.24

function useRange(progress: MotionValue<number>, from: number, to: number) {
  return useTransform(progress, [from, to], [0, 1])
}

function Riser({ progress, from, to, className = 'h-4 lg:h-6' }: { progress: MotionValue<number>; from: number; to: number; className?: string }) {
  const scaleY = useRange(progress, from, to)
  return <m.div data-reveal aria-hidden="true" style={{ scaleY, originY: 0 }} className={`mx-auto w-px bg-line-strong ${className}`} />
}

function Bar({ progress, from, to }: { progress: MotionValue<number>; from: number; to: number }) {
  const scaleX = useRange(progress, from, to)
  return (
    <div className="relative h-px" aria-hidden="true">
      <m.div data-reveal style={{ scaleX }} className="absolute inset-x-[16.666%] top-0 h-px bg-line-strong" />
    </div>
  )
}

function Node({ progress, from, to, children, className = '' }: { progress: MotionValue<number>; from: number; to: number; children: React.ReactNode; className?: string }) {
  const opacity = useRange(progress, from, to)
  const y = useTransform(progress, [from, to], [14, 0])
  return <m.div data-reveal style={{ opacity, y }} className={className}>{children}</m.div>
}

function OrgPerson({ id, column, row, progress, peek, onPeek }: { id: string; column: number; row: number; progress: MotionValue<number>; peek: boolean; onPeek: (open: boolean) => void }) {
  const person = findEmployee(id)
  const from = ACT_STARTS[3] + column * 0.025 + row * 0.025
  const isNubi = id === 'nubi'
  const isAda = id === 'ada'
  const opacity = useTransform(
    progress,
    [from, from + 0.06, NUBI_START, NUBI_START + 0.04, ADA_START - 0.02, ADA_START + 0.02],
    [0, 1, 1, isNubi ? 1 : DIMMED, isNubi ? 1 : DIMMED, isAda ? 1 : DIMMED],
  )
  const y = useTransform(progress, [from, from + 0.06], [12, 0])
  const scale = useTransform(progress, [from, from + 0.06], [0.82, 1])
  const ring = useTransform(progress, [NUBI_START, NUBI_START + 0.04, ADA_START - 0.02, ADA_START + 0.02], [0, 1, 1, 0])
  const conversations = useRange(progress, ADA_START + 0.02, ADA_START + 0.07)
  // Rows in the last column open their card towards the middle of the chart.
  const peekSide = column === 2 ? 'right-0 lg:right-full lg:mr-2' : 'left-0 lg:left-full lg:ml-2'

  return (
    <li className={`relative ${row === 2 ? 'hidden lg:block' : ''} ${peek ? 'z-40' : ''}`}>
      <m.div data-reveal style={{ opacity, y }}>
        <button
          type="button"
          onMouseEnter={() => onPeek(true)}
          onMouseLeave={() => onPeek(false)}
          onFocus={() => onPeek(true)}
          onBlur={() => onPeek(false)}
          onClick={() => onPeek(!peek)}
          aria-label={`${person.name}, ${person.role}. Skills: ${person.skills.join(', ')}`}
          className="flex w-full flex-col items-center gap-1 rounded-xl py-1 text-center lg:flex-row lg:gap-3 lg:px-1.5 lg:text-left"
        >
          <m.span data-reveal style={{ scale }} className="relative block">
            <Avatar id={id} size={56} className="max-lg:h-[44px]! max-lg:w-[44px]!" />
            {isNubi && <m.span data-reveal aria-hidden="true" style={{ opacity: ring }} className="absolute -inset-[3px] rounded-full border-2 border-ink lg:-inset-[3px]" />}
          </m.span>
          <span className="min-w-0">
            <span className="block truncate text-xs font-medium lg:text-[15px]">{person.name}</span>
            <span className="hidden truncate text-[13px] text-ink-3 xl:block">{person.role}</span>
          </span>
        </button>
      </m.div>
      {isAda && (
        <m.div data-reveal style={{ opacity: conversations }} aria-hidden="true" className="pointer-events-none absolute top-full left-0 z-30 w-[210px] lg:top-1/2 lg:left-[66px] lg:-translate-y-1/2">
          <div className="rounded-xl border border-line-strong bg-glass-solid p-1.5 shadow-soft">
            {adaConversations.map((conversation) => (
              <div key={conversation.title} className="flex items-center justify-between gap-2 rounded-lg px-2 py-1.5">
                <span className="truncate text-xs font-medium">{conversation.title}</span>
                <span className={`shrink-0 text-[11px] ${conversation.tone === 'amber' ? 'text-amber' : 'text-ink-3'}`}>{conversation.state}</span>
              </div>
            ))}
          </div>
        </m.div>
      )}
      {peek && <div className={`absolute top-full z-50 lg:top-1/2 lg:-translate-y-1/2 ${peekSide}`}><EmployeePeek id={id} /></div>}
    </li>
  )
}

/** The company as a chart: you, the CEO, the projects and their teams. Scroll progress builds it. */
export function OrgChart({ progress }: { progress: MotionValue<number> }) {
  const [peek, setPeek] = useState<string | null>(null)
  const bubble = useTransform(progress, [ACT_STARTS[1] + 0.09, ACT_STARTS[1] + 0.12, ACT_STARTS[2] - 0.02, ACT_STARTS[2] + 0.02], [0, 1, 1, 0])
  const bracket = useTransform(progress, [NUBI_START + 0.02, NUBI_START + 0.06, ADA_START - 0.02, ADA_START + 0.02], [0, 1, 1, 0])

  return (
    <div className="mx-auto w-full max-w-[780px] lg:origin-center lg:[@media(max-height:820px)]:scale-[0.84]">
      <Node progress={progress} from={0} to={0.05} className="flex justify-center">
        <div className="flex items-center gap-3 rounded-2xl border border-line-strong bg-raised py-2 pr-5 pl-2">
          <span aria-hidden="true" className="grid h-9 w-9 place-items-center rounded-full bg-button text-sm font-semibold text-button-ink lg:h-12 lg:w-12">You</span>
          <span className="text-left">
            <span className="block text-sm font-medium lg:text-base">The owner</span>
            <span className="block text-xs text-ink-3 lg:text-[13px]">That’s you</span>
          </span>
        </div>
      </Node>

      <Riser progress={progress} from={ACT_STARTS[1]} to={ACT_STARTS[1] + 0.05} />

      <Node progress={progress} from={ACT_STARTS[1] + 0.03} to={ACT_STARTS[1] + 0.09} className="relative flex justify-center">
        <div className="flex items-center gap-3 rounded-2xl border border-line-strong bg-raised py-2 pr-5 pl-2">
          <span className="relative block h-9 w-9 overflow-hidden rounded-full shadow-[0_0_0_1px_var(--line-strong)] lg:h-12 lg:w-12">
            <Image unoptimized src="/brand/retinue-tile-dark.webp" alt="" fill className="only-dark object-cover" />
            <Image unoptimized src="/brand/retinue-tile-light.webp" alt="" fill className="only-light object-cover" />
          </span>
          <span className="text-left">
            <span className="block text-sm font-medium lg:text-base">The CEO</span>
            <span className="block text-xs text-ink-3 lg:text-[13px]">Interviews and hires</span>
          </span>
        </div>
        <m.div data-reveal aria-hidden="true" style={{ opacity: bubble }} className="pointer-events-none absolute top-[calc(100%+8px)] left-1/2 z-20 w-[210px] -translate-x-1/2 lg:top-1/2 lg:left-[calc(50%+128px)] lg:w-[230px] lg:translate-x-0 lg:-translate-y-1/2">
          <div className="rounded-xl border border-line-strong bg-glass-solid px-3 py-2 text-center text-xs leading-snug shadow-soft lg:rounded-bl-sm lg:text-left lg:text-sm">What are we building, and who is it for?</div>
        </m.div>
      </Node>

      <Riser progress={progress} from={ACT_STARTS[2]} to={ACT_STARTS[2] + 0.03} />
      <Bar progress={progress} from={ACT_STARTS[2] + 0.02} to={ACT_STARTS[2] + 0.07} />

      <div className="grid grid-cols-3">
        {orgProjects.map((project, column) => (
          <div key={project.name} className="px-1 lg:px-2.5">
            <Riser progress={progress} from={ACT_STARTS[2] + 0.06} to={ACT_STARTS[2] + 0.09} />
            <Node progress={progress} from={ACT_STARTS[2] + 0.08 + column * 0.015} to={ACT_STARTS[2] + 0.13 + column * 0.015}>
              <div className="glass rounded-xl px-2 py-2 text-center lg:rounded-2xl lg:py-3">
                <h3 className="truncate text-[13px] font-medium tracking-[-0.01em] lg:text-[17px]">{project.name}</h3>
              </div>
            </Node>
            <ul className="mt-1.5 grid lg:mt-3 lg:gap-1.5">
              {project.team.map((id, row) => {
                const key = `${column}-${id}`
                return <OrgPerson key={key} id={id} column={column} row={row} progress={progress} peek={peek === key} onPeek={(open) => setPeek(open ? key : null)} />
              })}
            </ul>
          </div>
        ))}
      </div>

      <m.div data-reveal aria-hidden="true" style={{ opacity: bracket }} className="pointer-events-none px-[16.666%] pt-1">
        <div className="h-2.5 border-x border-b border-ink lg:h-3" />
        <p className="mt-1.5 text-center text-xs font-medium lg:text-sm">Nubi reviews all three</p>
      </m.div>
    </div>
  )
}
