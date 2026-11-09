'use client'

import { AnimatePresence, m, useInView } from 'motion/react'
import { useRef, useState } from 'react'
import { Avatar, Monogram } from '@/components/ui/Avatar'
import { SkillChips } from '@/components/ui/EmployeeCard'
import { Container, Section, SectionHead } from '@/components/ui/layout'
import { easeHaze } from '@/components/ui/Motion'
import { Reveal, StreamText } from '@/components/ui/Reveal'
import { employee } from '@/content/employees'
import { hiringScenarios, type HiringScenario } from '@/content/hiring'
import { MotionProvider } from '@/components/ui/Motion'

type Decision = 'approved' | 'rejected'

function Proposal({ scenario, decision, onDecide }: { scenario: HiringScenario; decision?: Decision; onDecide: (decision: Decision) => void }) {
  const { specialist } = scenario
  return (
    <m.div initial={{ opacity: 0, y: 14 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 1.1, duration: 0.5, ease: easeHaze }} className="mt-3 rounded-2xl border border-line-strong bg-raised p-4">
      <p className="text-[13px] text-ink-3">CEO proposes a new specialist</p>
      <div className="mt-2.5 flex flex-wrap items-center gap-x-3.5 gap-y-3">
        <Monogram name={specialist.name} size={46} />
        <div className="min-w-[180px] flex-1">
          <p className="font-medium">{specialist.name} <span className="font-normal text-ink-3">· {specialist.role}</span></p>
          <p className="text-sm leading-snug text-ink-2">{specialist.why}</p>
        </div>
        {decision ? (
          <p role="status" className={`text-sm font-medium ${decision === 'approved' ? 'text-green' : 'text-ink-3'}`}>{decision === 'approved' ? `${specialist.name} is hired` : 'Not hired'}</p>
        ) : (
          <div className="flex gap-2">
            <button type="button" onClick={() => onDecide('rejected')} className="min-h-11 rounded-[11px] px-3.5 text-sm font-medium text-ink-2 hover:text-ink">Reject</button>
            <button type="button" onClick={() => onDecide('approved')} className="min-h-11 rounded-[11px] bg-button px-4 text-sm font-medium text-button-ink transition-opacity hover:opacity-90">Approve</button>
          </div>
        )}
      </div>
    </m.div>
  )
}

/** Pick a project type and watch the CEO assemble a team. All of it comes from content/hiring.ts. */
function HiringSimulatorSection() {
  const panel = useRef<HTMLDivElement>(null)
  const visible = useInView(panel, { once: true, margin: '0px 0px -20% 0px' })
  const [selected, setSelected] = useState(hiringScenarios[0].id)
  const [decisions, setDecisions] = useState<Record<string, Decision>>({})
  const scenario = hiringScenarios.find((item) => item.id === selected) ?? hiringScenarios[0]

  return (
    <Section id="hiring">
      <Container>
        <Reveal>
          <SectionHead eyebrow="Try it" title="Tell the CEO what you’re building." lead="Pick a project and watch it put a team together, the same way it does in the app." />
        </Reveal>
        <Reveal className="grid gap-4 lg:grid-cols-[minmax(0,4fr)_minmax(0,9fr)]">
          <div role="group" aria-label="Project type" className="flex gap-2 overflow-x-auto pb-1 lg:flex-col lg:overflow-visible lg:pb-0">
            {hiringScenarios.map((item) => {
              const on = item.id === selected
              return (
                <button key={item.id} type="button" aria-pressed={on} onClick={() => setSelected(item.id)} className={`shrink-0 rounded-[14px] border px-4 py-3 text-left text-[15px] font-medium tracking-[-0.01em] transition-colors duration-200 lg:px-5 lg:py-4 lg:text-[17px] ${on ? 'border-transparent bg-button text-button-ink' : 'border-line bg-glass text-ink-2 hover:text-ink'}`}>
                  {item.label}
                </button>
              )
            })}
          </div>

          <div ref={panel} className="glass min-h-[560px] rounded-[24px] p-4 shadow-soft sm:p-7">
            <AnimatePresence mode="wait">
              <m.div key={scenario.id} initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.2 }}>
                <p className="ml-auto w-fit max-w-[85%] rounded-[18px] bg-raised px-4 py-2.5 text-[15px]">{scenario.prompt}</p>
                <div className="mt-4 flex items-start gap-3">
                  <span className="mt-0.5 grid h-9 w-9 shrink-0 place-items-center rounded-full bg-button text-[11px] font-semibold text-button-ink" aria-hidden="true">CEO</span>
                  <p className="pt-1.5 text-[15px] leading-relaxed sm:text-base"><StreamText text={scenario.ceo} active={visible} /></p>
                </div>

                <ul className="mt-5 grid grid-cols-2 gap-2.5 [perspective:1100px] lg:grid-cols-4">
                  {scenario.team.map((id, index) => {
                    const person = employee(id)
                    return (
                      <m.li key={id} initial={{ opacity: 0, rotateY: -80 }} animate={visible ? { opacity: 1, rotateY: 0 } : undefined} transition={{ delay: 0.45 + index * 0.13, duration: 0.65, ease: easeHaze }} className="rounded-2xl border border-line bg-raised p-3.5 [backface-visibility:hidden]">
                        <Avatar id={id} size={54} />
                        <p className="mt-2.5 font-medium">{person.name}</p>
                        <p className="min-h-[2.5em] text-[13px] leading-tight text-ink-3">{person.role}</p>
                        <SkillChips skills={person.skills.slice(0, 2)} className="mt-2" />
                      </m.li>
                    )
                  })}
                </ul>

                {visible && <Proposal scenario={scenario} decision={decisions[scenario.id]} onDecide={(decision) => setDecisions((all) => ({ ...all, [scenario.id]: decision }))} />}
              </m.div>
            </AnimatePresence>
          </div>
        </Reveal>
      </Container>
    </Section>
  )
}

export function HiringSimulator() {
  return <MotionProvider><HiringSimulatorSection /></MotionProvider>
}
