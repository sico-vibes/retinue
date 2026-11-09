import { ButtonLink } from '@/components/ui/Button'
import { EmployeeTile } from '@/components/ui/EmployeeCard'
import { Container, Section, SectionHead } from '@/components/ui/layout'
import { Marquee } from '@/components/ui/Marquee'
import { Reveal } from '@/components/ui/Reveal'
import { departments, inDepartment, type DepartmentId } from '@/content/employees'

const rows: { departments: DepartmentId[]; duration: number; reverse?: boolean }[] = [
  { departments: ['engineering', 'infrastructure'], duration: 62 },
  { departments: ['quality', 'data'], duration: 74, reverse: true },
  { departments: ['product', 'design', 'growth'], duration: 58 },
]

function DepartmentTile({ id }: { id: DepartmentId }) {
  const department = departments.find((item) => item.id === id)
  return (
    <div className="flex w-[170px] shrink-0 flex-col justify-center rounded-2xl border border-line-strong px-4">
      <p className="text-[15px] leading-tight font-medium tracking-[-0.015em]">{department?.name}</p>
    </div>
  )
}

export function MeetTheTeam() {
  return (
    <Section id="team" flush className="pb-24 sm:pb-32 lg:pb-40">
      <Container>
        <Reveal>
          <SectionHead eyebrow="Meet the team" title="Twenty-eight specialists, ready to start." lead="Each one has a name, a role, skills and a profile photo, the way a colleague does in Slack or Teams. Upload your own photo, or create someone new." />
        </Reveal>
      </Container>
      <div className="grid gap-1.5">
        {rows.map((row) => (
          <Marquee key={row.departments.join()} duration={row.duration} reverse={row.reverse} label={row.departments.map((id) => departments.find((item) => item.id === id)?.name).join(' and ')}>
            {row.departments.flatMap((id) => [
              <DepartmentTile key={id} id={id} />,
              ...inDepartment(id).filter((person) => person.photo).map((person) => <EmployeeTile key={person.id} id={person.id} />),
            ])}
          </Marquee>
        ))}
      </div>
      <Container className="mt-10 text-center">
        <ButtonLink href="/team/" look="ghost">See everyone and what they do</ButtonLink>
      </Container>
    </Section>
  )
}
