'use client'
import Grainient from '@/components/bits/Grainient'
import { WhenVisible } from '@/components/motion'
import { useTheme } from '@/components/theme'
import { Art, Container, DownloadButton, Reveal } from '@/components/ui'
import { site } from '@/content/site'

/** The last word: the team standing on top of a panel with a slow grainy gradient. */
export function Closing() {
  const theme = useTheme()
  return <section className="relative pb-24 pt-[clamp(260px,40vw,520px)]">
    <Container>
      <Reveal className="relative">
        {/* The team posing together for a photo, standing on the top edge of the panel. */}
        <Art src="/art/scenes/team-photo.webp" alt="The team, posing together" className="pointer-events-none absolute bottom-[calc(100%-10px)] left-1/2 z-10 w-[min(88%,860px)] -translate-x-1/2" />
        <div className="relative overflow-hidden rounded-[24px] border border-line-strong bg-panel px-[clamp(24px,6vw,80px)] py-[clamp(56px,9vw,112px)] text-center">
          <WhenVisible className="absolute inset-0">
            <Grainient key={theme} lightMode={theme === 'light'} colors={theme === 'dark' ? ['#0b0b0c', '#20244f', '#5a63d8'] : ['#f6f5f2', '#dfe1fb', '#9aa2ff']} grainAmount={0.08} speed={0.4} />
          </WhenVisible>
          <div className="relative">
            <h2 className="display mx-auto max-w-[900px] text-[clamp(52px,8vw,120px)] text-ink">Hire your first employee today</h2>
            <p className="mx-auto mt-6 max-w-[520px] text-[17px] leading-relaxed text-ink-2">Free while in beta. Windows 10 and 11. Bring the coding agents you already pay for.</p>
            <div className="mt-10 flex flex-wrap justify-center gap-3">
              <DownloadButton />
              <a href={site.github} className="inline-flex h-11 items-center rounded-full border border-line-strong px-5 text-[15px] font-medium text-ink transition hover:border-ink">View on GitHub</a>
            </div>
          </div>
        </div>
      </Reveal>
    </Container>
  </section>
}
