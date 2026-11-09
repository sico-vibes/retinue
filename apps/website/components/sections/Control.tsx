'use client'

import { m } from 'motion/react'
import { useState } from 'react'
import { Avatar } from '@/components/ui/Avatar'
import { Container, Section, SectionHead } from '@/components/ui/layout'
import { easeHaze } from '@/components/ui/Motion'
import { Reveal } from '@/components/ui/Reveal'
import { employee } from '@/content/employees'
import { permissionModes, permissionRows, principles, usageRows, type PermissionMode } from '@/content/features'
import { MotionProvider } from '@/components/ui/Motion'

function PermissionsDemo() {
  const [modes, setModes] = useState(() => Object.fromEntries(permissionRows.map((row) => [row.id, row.mode])) as Record<string, PermissionMode>)
  const [focus, setFocus] = useState<PermissionMode>('Ask first')
  const detail = permissionModes.find((item) => item.mode === focus)?.detail

  return (
    <div className="glass rounded-[24px] p-4 shadow-soft sm:p-6">
      <h3 className="text-xl font-medium tracking-[-0.02em]">Permissions, per employee</h3>
      <p className="mt-1 text-[15px] text-ink-2">{detail}</p>
      <ul className="mt-5 grid gap-2">
        {permissionRows.map((row) => {
          const person = employee(row.id)
          return (
            <li key={row.id} className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-xl bg-well p-2.5">
              <Avatar id={row.id} size={36} badge={false} />
              <span className="min-w-[110px] flex-1 text-[15px] font-medium">{person.name}<span className="block text-[13px] font-normal text-ink-3">{person.role}</span></span>
              <div role="group" aria-label={`Permission mode for ${person.name}`} className="flex rounded-[10px] bg-raised p-0.5">
                {permissionModes.map(({ mode }) => {
                  const on = modes[row.id] === mode
                  return (
                    <button key={mode} type="button" aria-pressed={on} onMouseEnter={() => setFocus(mode)} onFocus={() => setFocus(mode)} onClick={() => setModes((all) => ({ ...all, [row.id]: mode }))} className={`min-h-9 rounded-lg px-2 text-xs font-medium whitespace-nowrap transition-colors duration-150 sm:px-2.5 sm:text-[13px] ${on ? 'bg-button text-button-ink' : 'text-ink-2 hover:text-ink'}`}>
                      {mode}
                    </button>
                  )
                })}
              </div>
            </li>
          )
        })}
        <li className="flex items-center gap-3 rounded-xl border border-dashed border-line-strong p-2.5">
          <span aria-hidden="true" className="grid h-9 w-9 place-items-center rounded-full bg-raised text-ink-2">
            <svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.5"><circle cx="8" cy="8" r="6" /><path d="M8 4.8V8l2.2 1.4" strokeLinecap="round" /></svg>
          </span>
          <span className="flex-1 text-[15px] font-medium">Scheduled work</span>
          <span className="text-[13px] font-medium text-ink-2">Never in bypass</span>
        </li>
      </ul>
    </div>
  )
}

function UsageCard() {
  return (
    <div className="glass rounded-[24px] p-4 sm:p-6">
      <h3 className="text-xl font-medium tracking-[-0.02em]">Limits at a glance</h3>
      <ul className="mt-5 grid gap-4">
        {usageRows.map((row, index) => (
          <li key={row.agent}>
            <div className="mb-1.5 flex justify-between text-sm"><span className="text-ink-2">{row.agent}</span><span className="font-medium tabular-nums">{row.used}%</span></div>
            <div className="h-1.5 overflow-hidden rounded-full bg-line-strong" role="img" aria-label={`${row.agent}: ${row.used} percent used`}>
              <m.div data-reveal className="h-full origin-left rounded-full bg-ink" style={{ width: '100%' }} initial={{ scaleX: 0 }} whileInView={{ scaleX: row.used / 100 }} viewport={{ once: true }} transition={{ duration: 1.1, delay: index * 0.12, ease: easeHaze }} />
            </div>
          </li>
        ))}
      </ul>
      <p className="mt-4 text-xs text-ink-3">Example values.</p>
    </div>
  )
}

function ControlSection() {
  return (
    <Section id="control">
      <Container>
        <Reveal>
          <SectionHead eyebrow="You stay in control" title="It’s your company. It runs on your computer." lead="Decide how much each employee may do on their own. Everything else waits for you." />
        </Reveal>
        <div className="grid gap-4 lg:grid-cols-[minmax(0,7fr)_minmax(0,5fr)]">
          <Reveal><PermissionsDemo /></Reveal>
          <Reveal delay={0.1} className="grid content-start gap-4">
            <UsageCard />
            <ul className="glass grid gap-4 rounded-[24px] p-4 sm:p-6">
              {principles.map((item) => (
                <li key={item.title}>
                  <h3 className="font-medium">{item.title}</h3>
                  <p className="mt-0.5 text-[15px] leading-snug text-ink-2">{item.body}</p>
                </li>
              ))}
            </ul>
          </Reveal>
        </div>
      </Container>
    </Section>
  )
}

export function Control() {
  return <MotionProvider><ControlSection /></MotionProvider>
}
