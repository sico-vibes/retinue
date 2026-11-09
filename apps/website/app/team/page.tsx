import type { Metadata } from 'next'
import { EmployeeCard } from '@/components/ui/EmployeeCard'
import { Container, Heading, Lead } from '@/components/ui/layout'
import { Reveal } from '@/components/ui/Reveal'
import { departments, inDepartment } from '@/content/employees'

export const metadata: Metadata = {
  title: 'The team',
  description: 'The twenty-eight specialists you can hire in Retinue, by department.',
}

export default function TeamPage() {
  return (
    <Container className="pt-36 pb-16 sm:pt-44">
      <header className="max-w-[820px]">
        <Heading as="h1" size="display" className="rise">The team.</Heading>
        <Lead className="rise mt-6 max-w-[620px]">Twenty-eight specialists across seven departments. Hire any of them into a project, upload your own photo, or create someone new.</Lead>
      </header>
      {departments.map((department) => (
        <section key={department.id} className="mt-20 sm:mt-28" aria-labelledby={`department-${department.id}`}>
          <Reveal>
            <Heading id={`department-${department.id}`} className="mb-8 text-[clamp(1.5rem,3vw,2.25rem)] leading-tight tracking-[-0.028em]">{department.name}</Heading>
          </Reveal>
          <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {inDepartment(department.id).map((person) => (
              <li key={person.id}><Reveal className="h-full"><EmployeeCard id={person.id} /></Reveal></li>
            ))}
          </ul>
        </section>
      ))}
    </Container>
  )
}
