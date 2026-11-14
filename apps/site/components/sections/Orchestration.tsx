'use client'
// How it works: the manager stands at the left of the board it runs. Your request, the work split
// between named employees who talk to each other, and the result that comes back to you. The
// section ends on an ink skyline that stands on the section's bottom line.
import { useEffect, useState } from 'react'
import { AnimatePresence, motion } from 'motion/react'
import { ArrowRight, Check, GitPullRequest } from 'lucide-react'
import { Art, Container, Lines, Reveal, SectionHead } from '@/components/ui'
import { byId } from '@/content/roster'

const threads = [
  { who: 'ada', task: 'Payment API with retries', agent: 'Claude Code', state: 'Working' },
  { who: 'wu', task: 'Checkout page', agent: 'Codex', state: 'Done' },
  { who: 'nova', task: 'Mobile checkout flow', agent: 'Claude Code', state: 'Working' },
  { who: 'mira', task: 'Conversion dashboard', agent: 'OpenCode', state: 'Done' },
  { who: 'volt', task: 'Staging deploy', agent: 'Codex', state: 'Queued' },
]

const chatter = [
  { from: 'ada', to: 'wu', text: 'A repeated order now answers 409. Show the saved order instead of an error.' },
  { from: 'wu', to: 'ada', text: 'Done. The form keeps what was typed.' },
  { from: 'mira', to: 'manager', text: 'Conversion tracking is live on staging.' },
]

function Face({ id, size = 26 }: { id: string; size?: number }) {
  if (id === 'manager') return <span className="grid shrink-0 place-items-center rounded-full bg-ink font-mono text-[9px] font-bold text-bg" style={{ width: size, height: size }}>MGR</span>
  return <img src={byId(id)?.photo ?? ''} alt="" className="shrink-0 rounded-full object-cover" style={{ width: size, height: size }} />
}

function Chatter() {
  const [index, setIndex] = useState(0)
  useEffect(() => { const timer = window.setInterval(() => setIndex((value) => (value + 1) % chatter.length), 3200); return () => window.clearInterval(timer) }, [])
  const line = chatter[index]
  return <div className="relative h-[86px] overflow-hidden border-t border-line px-4 py-3" aria-live="polite">
    <AnimatePresence mode="wait">
      <motion.div key={index} initial={{ opacity: 0, y: 14 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -14 }} transition={{ duration: 0.35 }} className="flex gap-2.5">
        <Face id={line.from} />
        <div className="min-w-0 text-[13px] leading-snug">
          <p className="font-medium text-ink">{byId(line.from)?.name} <span className="font-normal text-ink-3">to {line.to === 'manager' ? 'the manager' : byId(line.to)?.name}</span></p>
          <p className="mt-0.5 text-ink-2">{line.text}</p>
        </div>
      </motion.div>
    </AnimatePresence>
  </div>
}

const card = 'relative rounded-[16px] border border-line-strong bg-raise shadow-card'

export function Orchestration() {
  return <section className="relative overflow-hidden border-y border-line pt-[clamp(96px,14vh,160px)]">
    <Container>
      <SectionHead id="how" eyebrow="How it works" title={<>One manager.<br />A whole team.</>} lead="Ask once. The manager plans the work, briefs each employee and only brings you what needs you." />
      <div className="relative mt-14 grid items-end gap-6 lg:grid-cols-[260px_1fr]">
        {/* The manager, standing at the board. */}
        <Reveal className="relative hidden h-[460px] lg:block">
          <Art src="/art/characters/ceo.webp" alt="The manager" className="pointer-events-none absolute bottom-0 left-1/2 h-full w-auto max-w-none -translate-x-1/2 object-contain object-bottom" />
        </Reveal>
        <Reveal delay={0.1}>
          <div className="relative overflow-hidden rounded-[22px] border border-line bg-panel px-5 py-10 sm:px-8">
            <div className="tex-halftone pointer-events-none absolute inset-0 opacity-50" aria-hidden="true" />
            <div className="relative grid items-center gap-5 xl:grid-cols-[1fr_auto_1.35fr_auto_1fr]">
              <div className={`${card} p-5`}>
                <p className="eyebrow">You</p>
                <p className="mt-2 rounded-[12px] bg-panel px-3 py-2 text-[15px] text-ink">Ship the new checkout before Friday.</p>
                <div className="mt-4 flex items-center gap-2"><Face id="manager" /><span className="text-[14px] font-medium text-ink">Manager</span></div>
                <ul className="mt-2 space-y-1.5 text-[13px] text-ink-3"><li>Read <span className="text-ink-2">roadmap.md</span></li><li>Planned <span className="text-ink-2">5 tasks</span></li><li>Briefed <span className="text-ink-2">5 employees</span></li></ul>
              </div>
              <ArrowRight aria-hidden="true" className="mx-auto hidden text-ink-3 xl:block" />
              <div className={`${card} overflow-hidden`}>
                <div className="flex items-center justify-between border-b border-line px-4 py-2.5"><span className="text-[13px] font-medium text-ink">Conversations</span><span className="font-mono text-[11px] text-ink-3">5 running</span></div>
                <ul>{threads.map((thread) => <li key={thread.who} className="flex items-center gap-3 px-4 py-2.5 [&+&]:border-t [&+&]:border-line">
                  <Face id={thread.who} />
                  <div className="min-w-0 flex-1"><p className="truncate text-[14px] text-ink">{thread.task}</p><p className="text-[12px] text-ink-3">{byId(thread.who)?.name} · {thread.agent}</p></div>
                  <span className={`font-mono text-[11px] ${thread.state === 'Done' ? 'text-ok' : thread.state === 'Working' ? 'text-warn' : 'text-ink-3'}`}>{thread.state}</span>
                </li>)}</ul>
                <Chatter />
              </div>
              <ArrowRight aria-hidden="true" className="mx-auto hidden text-ink-3 xl:block" />
              <div className={`${card} p-5`}>
                <div className="flex items-center gap-2"><Face id="manager" /><span className="text-[14px] font-medium text-ink">Manager</span></div>
                <p className="mt-3 text-[15px] text-ink">3 changes are ready.</p>
                <ul className="mt-3 space-y-1.5 text-[13px] text-ink-2">{['Checkout page', 'Payment retries', 'Dashboard'].map((item) => <li key={item} className="flex items-center gap-2"><Check size={14} className="text-ok" aria-hidden="true" />{item}</li>)}</ul>
                <span className="mt-4 inline-flex h-9 w-full items-center justify-center gap-2 rounded-full bg-accent text-[14px] font-medium text-on-accent"><GitPullRequest size={15} aria-hidden="true" />Review all</span>
              </div>
            </div>
          </div>
        </Reveal>
      </div>
    </Container>
    {/* The skyline stands on the section's bottom line: no gap below it. */}
    <Lines name="skyline" className="pointer-events-none relative mx-auto mt-20 block w-full max-w-[1500px] translate-y-px opacity-80" />
  </section>
}
