'use client'

import { m, useInView, useMotionValueEvent, useScroll } from 'motion/react'
import { useRef, useState, useSyncExternalStore } from 'react'
import { Container, Heading, Lead, Eyebrow } from '@/components/ui/layout'
import { Reveal } from '@/components/ui/Reveal'
import { ProductFrame } from '@/components/ui/ProductFrame'
import { scenes } from '@/components/story/scenes'
import { stages, type Stage } from '@/content/story'
import { useReducedMotionSafe } from '@/lib/useReducedMotionSafe'
import { MotionProvider } from '@/components/ui/Motion'

const wideQuery = '(min-width: 1024px)'
const subscribeWide = (notify: () => void) => {
  const query = matchMedia(wideQuery)
  query.addEventListener('change', notify)
  return () => query.removeEventListener('change', notify)
}

/** On phones and tablets each stage is its own card, played as it scrolls into view. */
function StackedStage({ stage, reduced }: { stage: Stage; reduced: boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  const inView = useInView(ref, { once: true, margin: '0px 0px -25% 0px' })
  const Scene = scenes[stage.id]
  return (
    <Reveal>
      <div ref={ref}>
        <h3 className="text-[22px] font-medium tracking-[-0.022em]">{stage.title}</h3>
        <p className="mt-1.5 text-ink-2">{stage.body}</p>
        <ProductFrame title={stage.label} className="mt-5"><div className="min-h-[300px]"><Scene active={reduced || inView} /></div></ProductFrame>
      </div>
    </Reveal>
  )
}

/** A scroll-driven walk through one project: interview, hire, plan, build, ship. */
function IdeaToShippedSection() {
  const reduced = useReducedMotionSafe()
  const wide = useSyncExternalStore(subscribeWide, () => matchMedia(wideQuery).matches, () => true)
  const track = useRef<HTMLElement>(null)
  const { scrollYProgress } = useScroll({ target: track, offset: ['start start', 'end end'] })
  const [index, setIndex] = useState(0)
  useMotionValueEvent(scrollYProgress, 'change', (value) => setIndex(Math.min(stages.length - 1, Math.floor(value * stages.length))))

  const header = (
    <div>
      <Eyebrow>One project, start to finish</Eyebrow>
      <Heading>From idea to shipped.</Heading>
    </div>
  )

  if (!wide || reduced) {
    return (
      <section id="shipped" className="py-24 sm:py-32">
        <Container narrow>
          <Reveal className="mb-12">{header}</Reveal>
          <div className="grid grid-cols-[minmax(0,1fr)] gap-16">{stages.map((stage) => <StackedStage key={stage.id} stage={stage} reduced={reduced} />)}</div>
        </Container>
      </section>
    )
  }

  const stage = stages[index]
  return (
    <section id="shipped" ref={track} className="relative h-[480vh]">
      <div className="sticky top-0 flex h-[100svh] items-center overflow-clip">
        <Container className="grid grid-cols-[minmax(0,4fr)_minmax(0,8fr)] items-center gap-12 pt-14">
          <div>
            {header}
            <ol className="mt-10 grid gap-1">
              {stages.map((item, position) => (
                <li key={item.id} className={`rounded-xl px-4 py-3 transition-colors duration-300 ${position === index ? 'bg-raised' : ''}`}>
                  <p className={`text-[15px] font-medium transition-colors duration-300 ${position === index ? 'text-ink' : 'text-ink-3'}`}>{item.label}</p>
                </li>
              ))}
            </ol>
          </div>
          <div>
            <ProductFrame title={stage.label}>
              <div className="relative h-[min(58vh,560px)]">
                {stages.map((item, position) => {
                  const Scene = scenes[item.id]
                  const on = position === index
                  return (
                    <m.div key={item.id} initial={false} animate={{ opacity: on ? 1 : 0 }} transition={{ duration: 0.35 }} aria-hidden={!on} className={`absolute inset-0 ${on ? '' : 'pointer-events-none'}`}>
                      <Scene active={on} />
                    </m.div>
                  )
                })}
              </div>
            </ProductFrame>
            <div className="mt-6 min-h-[86px]">
              <m.div key={stage.id} initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.4 }}>
                <h3 className="text-2xl font-medium tracking-[-0.022em]">{stage.title}</h3>
                <Lead className="mt-1">{stage.body}</Lead>
              </m.div>
            </div>
          </div>
        </Container>
      </div>
    </section>
  )
}

export function IdeaToShipped() {
  return <MotionProvider><IdeaToShippedSection /></MotionProvider>
}
