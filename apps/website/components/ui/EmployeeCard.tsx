import { Avatar } from './Avatar'
import { employee as findEmployee } from '@/content/employees'

export function SkillChips({ skills, className = '' }: { skills: string[]; className?: string }) {
  return (
    <ul className={`flex flex-wrap gap-1.5 ${className}`} aria-label="Skills">
      {skills.map((skill) => <li key={skill} className="rounded-md bg-raised px-2 py-0.5 text-xs text-ink-2">{skill}</li>)}
    </ul>
  )
}

/** A compact person: the tile in the marquee. */
export function EmployeeTile({ id }: { id: string }) {
  const person = findEmployee(id)
  return (
    <div className="glass flex w-[250px] shrink-0 items-center gap-3.5 rounded-2xl p-3.5">
      <Avatar id={id} size={52} />
      <div className="min-w-0">
        <p className="truncate text-[15px] font-medium tracking-[-0.01em]">{person.name}</p>
        <p className="truncate text-[13px] text-ink-3">{person.role}</p>
      </div>
    </div>
  )
}

/** The full card: photo, role, summary and skills. */
export function EmployeeCard({ id }: { id: string }) {
  const person = findEmployee(id)
  return (
    <article className="glass flex h-full flex-col rounded-2xl p-5">
      <div className="flex items-center gap-4">
        <Avatar id={id} size={64} />
        <div className="min-w-0">
          <h3 className="text-lg font-medium tracking-[-0.015em]">{person.name}</h3>
          <p className="text-sm text-ink-3">{person.role}</p>
        </div>
      </div>
      <p className="mt-4 flex-1 text-[15px] leading-relaxed text-ink-2">{person.summary}</p>
      <SkillChips skills={person.skills} className="mt-4" />
    </article>
  )
}

/** The small card shown when you hover or focus someone on the org chart. */
export function EmployeePeek({ id }: { id: string }) {
  const person = findEmployee(id)
  return (
    <div className="w-[230px] rounded-xl border border-line-strong bg-glass-solid p-3.5 shadow-soft">
      <p className="text-sm font-medium">{person.name}</p>
      <p className="text-[13px] text-ink-3">{person.role}</p>
      <SkillChips skills={person.skills.slice(0, 4)} className="mt-2.5" />
    </div>
  )
}
