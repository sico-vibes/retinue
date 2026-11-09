import Image from 'next/image'
import { agentNames, employee as findEmployee, type AgentId } from '@/content/employees'

function AgentBadge({ agent, size }: { agent: AgentId; size: number }) {
  const logo = (src: string, className = '') => <Image unoptimized src={src} alt="" width={size} height={size} className={`h-[66%] w-[66%] ${className}`} />
  return (
    <span title={agentNames[agent]} className="absolute -right-[3%] -bottom-[3%] grid place-items-center rounded-full bg-page shadow-[0_0_0_1.5px_var(--line-strong)]" style={{ width: size, height: size }}>
      {agent === 'claude' ? logo('/providers/claude.svg') : (
        <>
          {logo(`/providers/${agent}-dark.svg`, 'only-dark')}
          {logo(`/providers/${agent}-light.svg`, 'only-light')}
        </>
      )}
    </span>
  )
}

/** A round profile photo with the small coding-agent badge, as in Slack or Teams. */
export function Avatar({ id, size = 48, badge = true, className = '', priority = false }: { id: string; size?: number; badge?: boolean; className?: string; priority?: boolean }) {
  const person = findEmployee(id)
  const badgeSize = Math.max(14, Math.round(size * 0.34))
  return (
    <span className={`relative inline-block shrink-0 ${className}`} style={{ width: size, height: size }}>
      {person.photo ? (
        <Image src={`/people/${id}.webp`} alt={person.name} width={size} height={size} priority={priority} className="h-full w-full rounded-full object-cover shadow-[0_0_0_1px_var(--line-strong)]" />
      ) : (
        <Monogram name={person.name} size={size} />
      )}
      {badge && <AgentBadge agent={person.agent} size={badgeSize} />}
    </span>
  )
}

/** For someone without a photo yet, such as a proposed new specialist. */
export function Monogram({ name, size = 48 }: { name: string; size?: number }) {
  return (
    <span aria-hidden="true" className="grid h-full w-full place-items-center rounded-full bg-raised font-medium text-ink-2 shadow-[0_0_0_1px_var(--line-strong)]" style={{ fontSize: size * 0.4, width: size, height: size }}>
      {name[0]}
    </span>
  )
}
