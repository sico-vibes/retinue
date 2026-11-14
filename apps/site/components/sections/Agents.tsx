'use client'
import LogoLoop from '@/components/bits/LogoLoop'
import { Container } from '@/components/ui'
import { providers } from '@/content/site'

/** The coding agents Retinue runs, as a slow loop of their own marks. */
export function Agents() {
  const logos = providers.map((provider) => ({
    node: <span className="flex items-center gap-2.5 text-[15px] font-medium text-ink-2">
      <img src={provider.logo} alt="" className={`size-5 object-contain ${provider.light ? 'theme-dark' : ''}`} />
      {provider.light && <img src={provider.light} alt="" className="theme-light size-5 object-contain" />}
      {provider.name}
    </span>,
    title: provider.name,
  }))
  return <section className="border-y border-line py-10" aria-label="Coding agents Retinue works with">
    <Container className="flex flex-col items-center gap-6 md:flex-row md:gap-10">
      <p className="eyebrow shrink-0 text-center md:text-left">Works with the agents<br className="hidden md:block" /> you already use</p>
      <div className="w-full min-w-0 flex-1">
        <LogoLoop logos={logos} speed={36} logoHeight={22} gap={56} pauseOnHover fadeOut fadeOutColor="var(--bg)" ariaLabel="Coding agents" />
      </div>
    </Container>
  </section>
}
