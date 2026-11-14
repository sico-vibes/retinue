'use client'
// Meet the team: a real ID badge swinging on its lanyard, the roster by department in the app's
// own hire-card style (click any card for the employee), and what every employee comes with.
import { useState } from 'react'
import dynamic from 'next/dynamic'
import GhostFibers from '@/components/bits/GhostFibers'
import { WhenVisible } from '@/components/motion'
import { useTheme } from '@/components/theme'
import { EmployeeCard } from '@/components/employees'
import { Button, Container, Reveal, SectionHead } from '@/components/ui'
import { departments, people } from '@/content/roster'
import { traits } from '@/content/site'

// The badge is WebGL (three.js): loaded only in the browser, and only near the screen.
const Lanyard = dynamic(() => import('@/components/bits/Lanyard'), { ssr: false })

export function Team() {
  const theme = useTheme()
  const [department, setDepartment] = useState(departments[0].id)
  const shown = people.filter((person) => person.department === department).slice(0, 6)
  const badge = shown[0] ?? people[0]
  return <section className="relative overflow-hidden border-t border-line py-[clamp(96px,14vh,160px)]">
    <WhenVisible className="pointer-events-none absolute inset-0 opacity-60 [mask-image:radial-gradient(ellipse_at_70%_30%,black,transparent_70%)]">
      <GhostFibers key={theme} lightMode={theme === 'light'} glowColor="#7c86ff" lineColor={theme === 'dark' ? '#c7cbff' : '#4f5bd5'} speed={0.4} />
    </WhenVisible>
    <Container className="relative">
      <SectionHead id="team" eyebrow={`${people.length} employees to hire`} title={<>Specialists,<br />ready to hire</>} lead="Backend, design, security, data, writing. Each one arrives with the skills for the job." action={<Button href="/team/" variant="ghost">Meet all {people.length}</Button>} />
      <div className="mt-14 grid gap-8 lg:grid-cols-[360px_1fr]">
        <Reveal className="relative hidden h-[520px] lg:block">
          <WhenVisible className="absolute inset-0">
            <Lanyard key={badge.id} frontImage={`/badges/${badge.id}.png`} backImage={`/badges/${badge.id}.png`} cardColor="#0b0b0c" strapColor={badge.color} finish="glossy" />
          </WhenVisible>
        </Reveal>
        <div>
          <div className="flex flex-wrap gap-1.5" role="tablist" aria-label="Departments">
            {departments.map((item) => <button key={item.id} type="button" role="tab" aria-selected={item.id === department} onClick={() => setDepartment(item.id)}
              className={`h-9 rounded-full px-4 text-[14px] font-medium transition ${item.id === department ? 'bg-ink text-bg' : 'border border-line text-ink-2 hover:border-line-strong hover:text-ink'}`}>{item.name}</button>)}
          </div>
          <div className="mt-6 grid min-h-[480px] content-start gap-4 sm:grid-cols-2 xl:grid-cols-3">
            {shown.map((person) => <EmployeeCard key={person.id} person={person} />)}
          </div>
        </div>
      </div>
      <div className="mt-16 grid gap-px overflow-hidden rounded-[18px] border border-line bg-line sm:grid-cols-2 lg:grid-cols-4">
        {traits.map((trait, index) => <Reveal key={trait.title} delay={index * 0.05} className="h-full">
          <div className="h-full bg-raise/90 p-6 backdrop-blur">
            <span className="font-mono text-[12px] text-accent">0{index + 1}</span>
            <h3 className="mt-6 text-[18px] font-semibold text-ink">{trait.title}</h3>
            <p className="mt-2 text-[15px] leading-relaxed text-ink-3">{trait.body}</p>
          </div>
        </Reveal>)}
      </div>
    </Container>
  </section>
}
