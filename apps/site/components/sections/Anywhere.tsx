'use client'
// It lives with you: other machines, a cloud computer of their own, the phone and web app, and the
// companion window. A slow veil shader behind, under a bordered two-by-two grid.
import DarkVeil from '@/components/bits/DarkVeil'
import { WhenVisible } from '@/components/motion'
import { useTheme } from '@/components/theme'
import { Art, Container, Reveal, SectionHead } from '@/components/ui'
import { anywhere } from '@/content/site'

const objects: Record<string, string> = { machines: '/art/objects/machines.webp', computer: '/art/objects/cloud-computer.webp', 'main-phone': '/art/objects/phone.webp', companion: '/art/props/headphones.webp' }

export function Anywhere() {
  const theme = useTheme()
  return <section className="relative overflow-hidden border-t border-line py-[clamp(96px,14vh,160px)]">
    <WhenVisible className="pointer-events-none absolute inset-0 opacity-70 [mask-image:linear-gradient(to_bottom,transparent,black_20%,black_80%,transparent)]">
      <DarkVeil key={theme} lightMode={theme === 'light'} color="#7c86ff" speed={0.35} />
    </WhenVisible>
    <Container className="relative">
      <SectionHead id="anywhere" eyebrow="It lives with you" title={<>Wherever<br />you work</>} lead="On this PC, your other machines, a server of your own and your phone." />
      <div className="mt-16 grid gap-5 md:grid-cols-2">
        {anywhere.map((item, index) => <Reveal key={item.title} delay={(index % 2) * 0.08}>
          <article className="group relative h-full overflow-hidden rounded-[18px] border border-line bg-raise/85 backdrop-blur">
            <div className="relative h-[280px] overflow-hidden border-b border-line bg-panel/70">
              <img src={`/shots/${item.shot}-dark.webp`} alt={item.title} className="theme-dark absolute inset-x-8 top-8 w-[calc(100%-4rem)] rounded-t-[10px] border border-line-strong shadow-card transition-transform duration-500 group-hover:-translate-y-1.5" loading="lazy" />
              <img src={`/shots/${item.shot}-light.webp`} alt="" aria-hidden="true" className="theme-light absolute inset-x-8 top-8 w-[calc(100%-4rem)] rounded-t-[10px] border border-line-strong shadow-card transition-transform duration-500 group-hover:-translate-y-1.5" loading="lazy" />
              <Art src={objects[item.shot]} className="pointer-events-none absolute bottom-3 right-4 z-10 w-[104px] drop-shadow-[0_12px_20px_rgb(0_0_0/0.3)]" />
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
