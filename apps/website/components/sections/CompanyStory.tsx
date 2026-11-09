'use client'

import { AnimatePresence, m, useMotionValue, useMotionValueEvent, useScroll } from 'motion/react'
import { useRef, useState } from 'react'
import { Container, Eyebrow, Heading, Lead } from '@/components/ui/layout'
import { easeHaze } from '@/components/ui/Motion'
import { ACT_STARTS, OrgChart, SETTLED_PROGRESS } from '@/components/ui/OrgChart'
import { acts } from '@/content/story'
import { useReducedMotionSafe } from '@/lib/useReducedMotionSafe'
import { MotionProvider } from '@/components/ui/Motion'

const actAt = (progress: number) => ACT_STARTS.reduce((current, start, index) => (progress >= start ? index : current), 0)

/** The heart of the page: a pinned org chart that scroll builds, one act at a time. */
function CompanyStorySection() {
  const reduced = useReducedMotionSafe()
  const track = useRef<HTMLElement>(null)
  const { scrollYProgress } = useScroll({ target: track, offset: ['start start', 'end end'] })
  const settled = useMotionValue(SETTLED_PROGRESS)
  const [act, setAct] = useState(0)
  useMotionValueEvent(scrollYProgress, 'change', (value) => setAct(actAt(value)))

  if (reduced) {
    return (
      <section id="company" className="py-24 sm:py-32">
        <Container className="grid gap-14 lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)] lg:items-center">
          <div>
            <Eyebrow>How it works</Eyebrow>
            <Heading>Retinue runs like a company.</Heading>
            <ol className="mt-10 grid gap-5">
              {acts.map((item) => (
                <li key={item.title}>
                  <h3 className="font-medium">{item.title}</h3>
                  <p className="mt-1 text-ink-2">{item.body}</p>
                </li>
              ))}
            </ol>
          </div>
          <OrgChart progress={settled} />
        </Container>
      </section>
    )
  }

  return (
    <section id="company" ref={track} className="relative h-[560vh]" aria-label="How Retinue runs like a company">
      <div className="sticky top-0 flex h-[100svh] items-center overflow-clip">
        <Container className="grid gap-5 pt-[76px] lg:grid-cols-[minmax(0,4fr)_minmax(0,7fr)] lg:items-center lg:gap-12 lg:pt-16">
          <div>
            <Eyebrow className="mb-2 lg:mb-5">How it works</Eyebrow>
            <Heading className="max-lg:text-[clamp(1.6rem,7.2vw,2.5rem)]">Retinue runs like a company.</Heading>
            <div className="relative mt-3 min-h-[92px] lg:mt-9 lg:min-h-[176px]" aria-live="polite">
              <AnimatePresence mode="wait" initial={false}>
                <m.div key={act} initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }} transition={{ duration: 0.34, ease: easeHaze }}>
                  <h3 className="text-[17px] font-medium tracking-[-0.018em] lg:text-[26px] lg:leading-[1.2]">{acts[act].title}</h3>
                  <Lead className="mt-1 max-lg:text-[15px] max-lg:leading-snug lg:mt-3">{acts[act].body}</Lead>
                </m.div>
              </AnimatePresence>
            </div>
            <div className="mt-2 flex gap-1.5 lg:mt-8" aria-hidden="true">
              {acts.map((item, index) => (
                <span key={item.title} className={`h-[3px] flex-1 rounded-full transition-colors duration-300 ${index <= act ? 'bg-ink' : 'bg-line-strong'}`} />
              ))}
            </div>
          </div>
          <div className="rounded-[28px] lg:glass lg:p-8 lg:shadow-soft"><OrgChart progress={scrollYProgress} /></div>
        </Container>
      </div>
    </section>
  )
}

export function CompanyStory() {
  return <MotionProvider><CompanyStorySection /></MotionProvider>
}
