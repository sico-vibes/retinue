'use client'
// The working day: a grid of cards, each a real screen above two lines of words, with office props
// sitting on the cards.
import SpotlightCard from '@/components/bits/SpotlightCard'
import { useTheme } from '@/components/theme'
import { Art, Container, Reveal, SectionHead } from '@/components/ui'
import { workday } from '@/content/site'

export function Workday() {
  const theme = useTheme()
  return <section className="relative border-t border-line py-[clamp(96px,14vh,160px)]">
    <Art src="/art/props/hanging-plant-2.webp" className="pointer-events-none absolute right-[6%] top-0 z-10 hidden w-[110px] origin-top animate-[sway_8s_ease-in-out_infinite] md:block" />
    <Container>
      <SectionHead id="workday" object="/art/objects/queue.webp" eyebrow="Your working day" title={<>You stay<br />in charge</>} lead="Read, steer and stop any conversation. You decide what runs and what lands." />
      <div className="mt-16 grid gap-5 md:grid-cols-2 lg:grid-cols-3">
        {workday.map((item, index) => <Reveal key={item.title} delay={(index % 3) * 0.06}>
          <SpotlightCard theme={theme} spotlightColor={theme === 'dark' ? 'rgba(124,134,255,0.14)' : 'rgba(79,91,213,0.10)'} className="group relative h-full !overflow-visible rounded-[18px] border border-line !p-0 ![background-color:var(--bg-raise)]">
            <div className="relative h-[230px] overflow-hidden rounded-t-[18px] border-b border-line bg-panel">
              <div className="tex-halftone pointer-events-none absolute inset-0 opacity-60" aria-hidden="true" />
              <div className="absolute inset-x-6 top-8 overflow-hidden rounded-t-[10px] border border-line-strong shadow-card transition-transform duration-500 group-hover:-translate-y-1.5">
                <img src={`/shots/${item.shot}-dark.webp`} alt={item.title} className="theme-dark block w-full" loading="lazy" />
                <img src={`/shots/${item.shot}-light.webp`} alt="" aria-hidden="true" className="theme-light block w-full" loading="lazy" />
              </div>
            </div>
            {item.prop && <Art src={item.prop} className="pointer-events-none absolute -right-3 top-[180px] z-10 w-[86px] drop-shadow-[0_10px_18px_rgb(0_0_0/0.25)] transition-transform duration-500 group-hover:-translate-y-1 group-hover:rotate-3" />}
            <div className="p-6">
              <p className="eyebrow mb-2 text-accent">{item.tag}</p>
              <h3 className="display text-[34px] text-ink">{item.title}</h3>
              <p className="mt-2 text-[15px] leading-relaxed text-ink-3">{item.body}</p>
            </div>
          </SpotlightCard>
        </Reveal>)}
      </div>
    </Container>
  </section>
}
