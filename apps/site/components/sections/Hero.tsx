'use client'
import GhostFibers from '@/components/bits/GhostFibers'
import BlurText from '@/components/bits/BlurText'
import ShinyText from '@/components/bits/ShinyText'
import { WhenVisible } from '@/components/motion'
import { useTheme } from '@/components/theme'
import { Art, Button, Container, DownloadButton, Frame, Reveal } from '@/components/ui'
import { site } from '@/content/site'

export function Hero() {
  const theme = useTheme()
  return <section id="top" className="relative overflow-hidden pb-[clamp(72px,10vh,120px)] pt-16">
    <WhenVisible className="pointer-events-none absolute inset-x-0 top-0 h-[1000px] [mask-image:linear-gradient(to_bottom,black_50%,transparent)]">
      <GhostFibers key={theme} lightMode={theme === 'light'} glowColor="#7c86ff" lineColor={theme === 'dark' ? '#c7cbff' : '#4f5bd5'} secondaryColor={theme === 'dark' ? '#ffffff' : '#0b0b0c'} speed={0.5} />
    </WhenVisible>
    <Container className="relative pt-[clamp(56px,9vh,104px)] text-center">
      <Reveal>
        <a href="/changelog/" className="eyebrow inline-flex items-center gap-2 rounded-full border border-line bg-bg/60 px-3 py-1.5 backdrop-blur transition hover:border-line-strong">
          <i className="size-1.5 rounded-full bg-ok" />
          <ShinyText text={`New in ${site.version}: work on your other machines`} speed={3} color={theme === 'dark' ? '#8b8b94' : '#6b6b73'} shineColor={theme === 'dark' ? '#ffffff' : '#0b0b0c'} />
        </a>
      </Reveal>
      <h1 className="display mx-auto mt-6 max-w-[920px] text-[clamp(48px,6.6vw,100px)] text-ink">
        <BlurText text="Run a company of AI engineers" animateBy="words" delay={80} direction="bottom" className="justify-center" trigger="load" />
      </h1>
      <Reveal delay={0.3}>
        <p className="mx-auto mt-5 max-w-[480px] text-[18px] leading-relaxed text-ink-2">A manager, a team of specialists, and your approval on every change.</p>
        <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
          <DownloadButton />
          <Button href="#how" variant="ghost">How it works</Button>
        </div>
      </Reveal>
    </Container>
    <Container className="relative mt-[clamp(150px,20vh,230px)]">
      <Reveal delay={0.15} className="relative">
        {/* The team leans over the window's top edge; a mug stands beside it on the same desk line, and a sticky note is pressed onto its right edge. */}
        <Art src="/art/scenes/hero-team.webp" alt="" className="pointer-events-none absolute bottom-[calc(100%-56px)] left-1/2 z-0 w-[clamp(300px,36vw,520px)] -translate-x-1/2" />
        <Frame shot="main" alt="Retinue: your team on the left, Ada briefing Fifi in the middle" className="relative z-10" />
        <Art src="/art/props/mug.webp" className="pointer-events-none absolute bottom-0 right-[calc(100%+20px)] z-20 hidden w-[120px] xl:block drop-shadow-[0_18px_24px_rgb(0_0_0/0.35)]" />
        <Art src="/art/props/sticky-notes.webp" className="pointer-events-none absolute left-[calc(100%-30px)] top-24 z-20 hidden w-[104px] rotate-6 drop-shadow-[0_14px_20px_rgb(0_0_0/0.3)] lg:block" />
      </Reveal>
    </Container>
  </section>
}
