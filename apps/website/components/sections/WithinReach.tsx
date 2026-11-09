'use client'

import { AnimatePresence, m } from 'motion/react'
import Image from 'next/image'
import { useState } from 'react'
import { Avatar } from '@/components/ui/Avatar'
import { Container, Heading, Section, SectionHead } from '@/components/ui/layout'
import { Reveal, StreamText } from '@/components/ui/Reveal'
import { MotionProvider } from '@/components/ui/Motion'

const spoken = 'Add a retry to the checkout call, and a test for it.'

function DictationDemo() {
  const [playing, setPlaying] = useState(false)
  const [run, setRun] = useState(0)
  return (
    <div className="flex min-h-[230px] flex-col items-center justify-center gap-5 p-6 text-center">
      <button type="button" onClick={() => { setRun((value) => value + 1); setPlaying(true) }} aria-label="Play a dictation example" className="group flex items-center gap-2.5 rounded-xl p-1 text-ink-3">
        <kbd className="rounded-[10px] border border-line-strong bg-raised px-3.5 py-2 text-[15px] font-medium text-ink shadow-[0_2px_0_var(--line-strong)] transition-transform duration-150 group-active:translate-y-0.5">Ctrl</kbd>
        <span aria-hidden="true">+</span>
        <kbd className="rounded-[10px] border border-line-strong bg-raised px-3.5 py-2 text-[15px] font-medium text-ink shadow-[0_2px_0_var(--line-strong)] transition-transform duration-150 group-active:translate-y-0.5">E</kbd>
      </button>
      <div aria-hidden="true" className="flex h-8 items-center gap-1">
        {Array.from({ length: 16 }, (_, index) => <span key={index} className={`block h-full w-[3px] rounded-full bg-ink-2 ${playing ? 'wave-bar' : 'opacity-40'}`} style={{ '--delay': `${-index * 83}ms`, transform: playing ? undefined : 'scaleY(.3)' } as React.CSSProperties} />)}
      </div>
      <p className="min-h-[2.8em] max-w-[22em] text-lg leading-snug tracking-[-0.015em]">
        <AnimatePresence mode="wait">
          {playing ? <m.span key={run} initial={{ opacity: 1 }}><StreamText text={spoken} active step={0.14} /></m.span> : <span className="text-ink-3">Press the keys to hear it work.</span>}
        </AnimatePresence>
      </p>
    </div>
  )
}

function PhoneMock() {
  return (
    <div role="img" aria-label="Retinue on a phone: Ada is waiting for approval to run a command, and Nubi and Fifi are working." className="glass mx-auto grid w-full max-w-[300px] gap-1.5 rounded-[34px] p-3.5 text-[13px] shadow-frame">
      <div className="flex justify-between px-2.5 py-1.5"><span className="text-base font-semibold tracking-[-0.015em]">Retinue</span><span className="text-ink-3">Connected</span></div>
      <div className="grid gap-2.5 rounded-[18px] border border-line bg-raised p-3">
        <div className="flex items-center gap-2.5"><Avatar id="ada" size={36} badge={false} /><div><p className="font-medium">Ada</p><p className="text-amber">Needs you</p></div></div>
        <code className="rounded-lg bg-well px-2.5 py-2 font-mono text-xs">npm test -- invoice</code>
        <div className="flex gap-2"><span className="rounded-[9px] bg-button px-3 py-1.5 font-medium text-button-ink">Allow once</span><span className="rounded-[9px] border border-line-strong px-3 py-1.5 font-medium">Deny</span></div>
      </div>
      {[{ id: 'nubi', name: 'Nubi', line: 'Reviewing checkout' }, { id: 'fifi', name: 'Fifi', line: 'Redrawing a page' }].map((row) => (
        <div key={row.id} className="flex items-center gap-2.5 p-2">
          <Avatar id={row.id} size={36} badge={false} />
          <div className="min-w-0 flex-1"><p className="font-medium">{row.name}</p><p className="truncate text-ink-3">{row.line}</p></div>
          <span className="text-xs text-amber">Working</span>
        </div>
      ))}
    </div>
  )
}

const remoteFacts = [
  { title: 'Tailscale', body: 'Reach your computer over your own private network.' },
  { title: 'Native link', body: 'A direct, encrypted link, protected by a password and your approval.' },
  { title: 'Approved by you', body: 'Each new device is approved on your desktop, and you choose what it may do. Remote is off until you turn it on.' },
]

function WithinReachSection() {
  return (
    <Section id="reach">
      <Container>
        <Reveal>
          <SectionHead eyebrow="Always within reach" title="Stay in your work until someone needs you." />
        </Reveal>
        <div className="grid gap-4 lg:grid-cols-[minmax(0,7fr)_minmax(0,5fr)]">
          <Reveal className="glass overflow-hidden rounded-[24px] p-5 sm:p-8">
            <Image src="/shots/companion-dark.webp" alt="The floating companion, listing employees and their status beside the one waiting for an answer." width={1100} height={722} sizes="(min-width: 1024px) 680px, 92vw" className="only-dark h-auto w-full" />
            <Image src="/shots/companion-light.webp" alt="The floating companion, listing employees and their status beside the one waiting for an answer." width={1100} height={722} sizes="(min-width: 1024px) 680px, 92vw" className="only-light h-auto w-full" />
            <Heading as="h3" size="card" className="mt-6">A companion that floats above your work.</Heading>
            <p className="mt-2 max-w-[48ch] leading-relaxed text-ink-2">A small window over your other apps shows who’s working and who needs you, so you can answer without switching.</p>
          </Reveal>
          <Reveal delay={0.08} className="glass flex flex-col overflow-hidden rounded-[24px] p-5 sm:p-8">
            <DictationDemo />
            <div className="mt-auto">
              <Heading as="h3" size="card">Say it instead.</Heading>
              <p className="mt-2 leading-relaxed text-ink-2">Press Ctrl+E and speak. Your words are turned into text on your own computer.</p>
            </div>
          </Reveal>
        </div>

        <Reveal className="glass mt-4 grid items-center gap-10 rounded-[24px] p-6 sm:p-10 lg:grid-cols-2">
          <div>
            <Heading as="h3" className="text-[clamp(1.75rem,3.2vw,2.75rem)] leading-[1.08] tracking-[-0.03em]">Your team, from your phone.</Heading>
            <p className="mt-3 max-w-[46ch] leading-relaxed text-ink-2">Read what they’ve done, answer their questions and hand out the next job, wherever you are.</p>
            <dl className="mt-7 grid gap-4">
              {remoteFacts.map((fact) => (
                <div key={fact.title} className="border-t border-line pt-3.5">
                  <dt className="font-medium">{fact.title}</dt>
                  <dd className="text-[15px] leading-snug text-ink-2">{fact.body}</dd>
                </div>
              ))}
            </dl>
          </div>
          <PhoneMock />
        </Reveal>
      </Container>
    </Section>
  )
}

export function WithinReach() {
  return <MotionProvider><WithinReachSection /></MotionProvider>
}
