'use client'
// Meet the team: the roster by department in the app's own hire-card style. Click anyone and their
// ID badge swings onto the lanyard and turns over to show what they do, how they talk and their skills.
import { useState } from 'react'
import { Badge } from '@/components/Badge'
import { EmployeeCard } from '@/components/employees'
import { Art, Button, Container, Reveal, SectionHead } from '@/components/ui'
import { byId, departments, people } from '@/content/roster'
import { traits } from '@/content/site'

export function Team() {
  const [department, setDepartment] = useState(departments[0].id)
  const [selected, setSelected] = useState('ada')
  const shown = people.filter((person) => person.department === department).slice(0, 6)
  const person = byId(selected) ?? people[0]
  return <section className="relative py-[clamp(96px,14vh,160px)]">
    {/* A big monstera leans in from the left edge, close to the camera. */}
    <Art src="/art/edges/monstera-left.webp" className="pointer-events-none absolute -left-10 top-24 hidden w-[min(22vw,320px)] blur-[1.5px] 2xl:block" />
    <Container className="relative">
      <SectionHead id="team" eyebrow={`${people.length} employees to hire`} title={<>Specialists,<br />ready to hire</>} lead="Each one arrives with the skills for the job. Pick anyone to see their badge." action={<Button href="/team/" variant="ghost">Meet all {people.length}</Button>} />
      <div className="mt-14 grid items-start gap-10 lg:grid-cols-[minmax(0,1fr)_380px]">
        <div>
          <div className="flex flex-wrap gap-1.5" role="tablist" aria-label="Departments">
            {departments.map((item) => <button key={item.id} type="button" role="tab" aria-selected={item.id === department} onClick={() => setDepartment(item.id)}
              className={`h-9 rounded-full px-4 text-[14px] font-medium transition ${item.id === department ? 'bg-ink text-bg' : 'border border-line text-ink-2 hover:border-line-strong hover:text-ink'}`}>{item.name}</button>)}
          </div>
          <div className="mt-6 grid min-h-[520px] content-start gap-4 sm:grid-cols-2 xl:grid-cols-3">
            {shown.map((item) => <EmployeeCard key={item.id} person={item} selected={item.id === selected} onSelect={() => setSelected(item.id)} />)}
          </div>
        </div>
        {/* The lanyard hangs from the top of its column, the badge level with the cards. */}
        <Reveal className="relative hidden lg:block">
          <Badge person={person} className="relative h-[680px] w-full" />
        </Reveal>
      </div>
      <div className="mt-16 grid gap-px overflow-hidden rounded-[18px] border border-line bg-line sm:grid-cols-2 lg:grid-cols-4">
        {traits.map((trait, index) => <Reveal key={trait.title} delay={index * 0.05} className="h-full">
          <div className="h-full bg-raise p-6">
            <span className="font-mono text-[12px] text-accent">0{index + 1}</span>
            <h3 className="mt-6 text-[18px] font-semibold text-ink">{trait.title}</h3>
            <p className="mt-2 text-[15px] leading-relaxed text-ink-3">{trait.body}</p>
          </div>
        </Reveal>)}
      </div>
    </Container>
  </section>
}
