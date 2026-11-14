'use client'
// Private by design, with the office building's facade drawn up the edges of the page.
import { Container, Lines, Reveal, SectionHead } from '@/components/ui'
import { privacy } from '@/content/site'

export function Privacy() {
  return <section className="relative overflow-hidden border-t border-line py-[clamp(96px,14vh,160px)]">
    <Lines name="facade" className="pointer-events-none absolute -left-24 top-0 hidden h-full w-auto opacity-25 xl:block" />
    <Lines name="facade" className="pointer-events-none absolute -right-24 top-0 hidden h-full w-auto -scale-x-100 opacity-25 xl:block" />
    <Container className="relative">
      <SectionHead id="privacy" eyebrow="Private by design" title={<>No account.<br />No data collected.</>} center />
      <div className="mt-16 grid gap-px overflow-hidden rounded-[18px] border border-line bg-line sm:grid-cols-2 lg:grid-cols-4">
        {privacy.map((item, index) => <Reveal key={item.title} delay={index * 0.06} className="h-full">
          <div className="h-full bg-raise p-7">
            <span className="font-mono text-[12px] text-ink-3">0{index + 1}</span>
            <h3 className="mt-8 text-[19px] font-semibold text-ink">{item.title}</h3>
            <p className="mt-2 text-[15px] leading-relaxed text-ink-3">{item.body}</p>
          </div>
        </Reveal>)}
      </div>
    </Container>
  </section>
}
