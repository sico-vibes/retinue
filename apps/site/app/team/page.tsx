import type { Metadata } from 'next'
import { PageShell } from '@/components/shell'
import { EmployeeCard } from '@/components/employees'
import { Container, DownloadButton } from '@/components/ui'
import { departments, people } from '@/content/roster'

export const metadata: Metadata = { title: 'The team · Retinue', description: `Every employee you can hire in Retinue: ${people.length} specialists across ${departments.length} departments.` }

export default function TeamPage() {
  return <PageShell>
    <Container className="pb-10 pt-[clamp(72px,12vh,120px)]">
      <p className="eyebrow">{people.length} employees · {departments.length} departments</p>
      <h1 className="display mt-3 text-[clamp(56px,8vw,120px)] text-ink">The whole team</h1>
      <p className="mt-5 max-w-[560px] text-[18px] leading-relaxed text-ink-2">Every one comes with the skills for the role and a personality you can change. Click anyone to meet them.</p>
      <nav aria-label="Departments" className="mt-8 flex flex-wrap gap-1.5">{departments.map((department) => <a key={department.id} href={`#${department.id}`} className="h-9 rounded-full border border-line px-4 text-[14px] font-medium leading-9 text-ink-2 transition hover:border-line-strong hover:text-ink">{department.name}</a>)}</nav>
    </Container>
    <Container className="pb-24">
      {departments.map((department) => {
        const list = people.filter((person) => person.department === department.id)
        if (!list.length) return null
        return <section key={department.id} id={department.id} className="scroll-mt-24 border-t border-line py-12">
          <div className="mb-6 flex items-baseline justify-between"><h2 className="display text-[44px] text-ink">{department.name}</h2><span className="font-mono text-[12px] text-ink-3">{list.length}</span></div>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">{list.map((person) => <EmployeeCard key={person.id} person={person} />)}</div>
        </section>
      })}
      <div className="mt-8 flex flex-col items-center gap-5 rounded-[22px] border border-line bg-raise p-12 text-center">
        <h2 className="display text-[48px] text-ink">Hire them in Retinue</h2>
        <DownloadButton />
      </div>
    </Container>
  </PageShell>
}
