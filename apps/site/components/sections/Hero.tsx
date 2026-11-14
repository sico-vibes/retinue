'use client'
import Lightfall from '@/components/bits/Lightfall'
import BlurText from '@/components/bits/BlurText'
import ShinyText from '@/components/bits/ShinyText'
import { WhenVisible } from '@/components/motion'
import { useTheme } from '@/components/theme'
import { Art, Button, Container, DownloadButton, Frame, Reveal } from '@/components/ui'
import { site } from '@/content/site'

export function Hero() {
  const theme = useTheme()
  return <section id="top" className="relative overflow-hidden pt-16">
    <WhenVisible className="pointer-events-none absolute inset-x-0 top-0 h-[900px] [mask-image:linear-gradient(to_bottom,black_45%,transparent)]">
      <Lightfall key={theme} lightMode={theme === 'light'} colors={theme === 'dark' ? ['#7c86ff', '#c7cbff', '#ffffff'] : ['#4f5bd5', '#7c86ff', '#0b0b0c']} backgroundColor={theme === 'dark' ? '#0b0b0c' : '#f6f5f2'} speed={0.6} density={0.7} opacity={theme === 'dark' ? 0.9 : 0.55} />
    </WhenVisible>
    {/* Office props drift at the edges of the hero. */}
    <Art src="/art/props/paper-plane.webp" className="float pointer-events-none absolute left-[6%] top-[30%] hidden w-[110px] [--tilt:-8deg] lg:block" />
    <Art src="/art/props/mug.webp" className="float pointer-events-none absolute right-[7%] top-[22%] hidden w-[120px] [animation-delay:-3s] lg:block" />
    <Container className="relative pt-[clamp(64px,10vh,120px)] text-center">
      <Reveal>
        <a href="/changelog/" className="eyebrow inline-flex items-center gap-2 rounded-full border border-line bg-bg/60 px-3 py-1.5 backdrop-blur transition hover:border-line-strong">
          <i className="size-1.5 rounded-full bg-ok" />
          <ShinyText text={`New in ${site.version}: talk to employees on your other machines`} speed={3} color={theme === 'dark' ? '#8b8b94' : '#6b6b73'} shineColor={theme === 'dark' ? '#ffffff' : '#0b0b0c'} />
        </a>
      </Reveal>
      <h1 className="display mx-auto mt-7 max-w-[1000px] text-[clamp(52px,7.4vw,116px)] text-ink">
        <BlurText text="Run a company of AI engineers" animateBy="words" delay={80} direction="bottom" className="justify-center" trigger="load" />
      </h1>
      <Reveal delay={0.3}>
        <p className="mx-auto mt-6 max-w-[520px] text-[19px] leading-relaxed text-ink-2">One CEO, a team of specialists, and your approval on every change. On your desktop.</p>
        <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
          <DownloadButton />
          <Button href="#ceo" variant="ghost">See how it works</Button>
        </div>
      </Reveal>
    </Container>
    <Container className="relative mt-[clamp(170px,22vh,260px)]">
      <Reveal delay={0.15} className="relative">
        <Art src="/art/scenes/hero-team.webp" alt="" className="pointer-events-none absolute bottom-[calc(100%-56px)] left-1/2 z-0 w-[clamp(300px,38vw,540px)] -translate-x-1/2" />
        <Frame shot="main" alt="Retinue: your team on the left, a conversation in the middle, its work on the right" className="relative z-10" />
      </Reveal>
    </Container>
  </section>
}
