'use client'
// It lives with you: other machines, a cloud computer of their own, the phone and web app, and the
// companion window. A slow veil shader behind, under a bordered two-by-two grid.
import DarkVeil from '@/components/bits/DarkVeil'
import { WhenVisible } from '@/components/motion'
import { useTheme } from '@/components/theme'
import { Art, Container, Reveal, SectionHead } from '@/components/ui'
import { anywhere } from '@/content/site'


export function Anywhere() {
  const theme = useTheme()
  return <section className="relative overflow-hidden border-t border-line py-[clamp(96px,14vh,160px)]">
    <WhenVisible className="pointer-events-none absolute inset-0 opacity-70 [mask-image:linear-gradient(to_bottom,transparent,black_20%,black_80%,transparent)]">
      <DarkVeil key={theme} lightMode={theme === 'light'} color="#7c86ff" speed={0.35} />
    </WhenVisible>
    {/* The corner of a desk comes in from the right edge, close to the camera. */}
    <Art src="/art/edges/desk-right.webp" className="pointer-events-none absolute -right-8 bottom-0 z-10 hidden w-[min(26vw,380px)] 2xl:block" />
    <Container className="relative">
      <SectionHead id="anywhere" eyebrow="It lives with you" title={<>Wherever<br />you work</>} lead="On this PC, your other machines, a server of your own and your phone." />
      <div className="mt-16 grid gap-5 md:grid-cols-2">
        {anywhere.map((item, index) => <Reveal key={item.title} delay={(index % 2) * 0.08}>
          <article className="group relative h-full overflow-hidden rounded-[18px] border border-line bg-raise/85 backdrop-blur">
            <div className="relative h-[280px] overflow-hidden border-b border-line bg-panel/70">
              <img src={`/shots/${item.shot}-dark.webp`} alt={item.title} className="theme-dark absolute inset-x-8 top-8 w-[calc(100%-4rem)] rounded-t-[10px] border border-line-strong shadow-card transition-transform duration-500 group-hover:-translate-y-1.5" loading="lazy" />
              <img src={`/shots/${item.shot}-light.webp`} alt="" aria-hidden="true" className="theme-light absolute inset-x-8 top-8 w-[calc(100%-4rem)] rounded-t-[10px] border border-line-strong shadow-card transition-transform duration-500 group-hover:-translate-y-1.5" loading="lazy" />
            </div>
            <div className="p-7">
              <p className="eyebrow mb-2 text-accent">{item.tag}</p>
              <h3 className="display text-[36px] text-ink">{item.title}</h3>
              <p className="mt-2 max-w-[460px] text-[15px] leading-relaxed text-ink-3">{item.body}</p>
            </div>
          </article>
        </Reveal>)}
      </div>
    </Container>
  </section>
}
