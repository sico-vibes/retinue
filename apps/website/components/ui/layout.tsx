import type { ReactNode } from 'react'

// The page grid and type scale. Every section is built from these.

export function Container({ children, className = '', narrow = false }: { children: ReactNode; className?: string; narrow?: boolean }) {
  return <div className={`mx-auto w-full px-5 sm:px-8 ${narrow ? 'max-w-[960px]' : 'max-w-[1240px]'} ${className}`}>{children}</div>
}

export function Section({ id, children, className = '', flush = false }: { id?: string; children: ReactNode; className?: string; flush?: boolean }) {
  return <section id={id} className={`relative ${flush ? '' : 'py-24 sm:py-32 lg:py-40'} ${className}`}>{children}</section>
}

export function Eyebrow({ children, className = '' }: { children: ReactNode; className?: string }) {
  return <p className={`mb-5 text-[15px] font-medium tracking-[-0.005em] text-ink-3 ${className}`}>{children}</p>
}

const headingSizes = {
  display: 'text-[clamp(2.75rem,8.4vw,7rem)] leading-[0.98] tracking-[-0.045em]',
  title: 'text-[clamp(2.125rem,5vw,4.25rem)] leading-[1.04] tracking-[-0.035em]',
  card: 'text-[clamp(1.25rem,1.9vw,1.5rem)] leading-[1.2] tracking-[-0.022em]',
}

export function Heading({ as: Tag = 'h2', size = 'title', children, className = '', id }: { as?: 'h1' | 'h2' | 'h3'; size?: keyof typeof headingSizes; children: ReactNode; className?: string; id?: string }) {
  return <Tag id={id} className={`font-medium text-balance ${headingSizes[size]} ${className}`}>{children}</Tag>
}

export function Lead({ children, className = '' }: { children: ReactNode; className?: string }) {
  return <p className={`text-[clamp(1.0625rem,1.45vw,1.3rem)] leading-[1.55] tracking-[-0.012em] text-ink-2 ${className}`}>{children}</p>
}

/** The standard section opening: eyebrow, title and an optional lead. */
export function SectionHead({ eyebrow, title, lead, center = false, className = '' }: { eyebrow?: string; title: ReactNode; lead?: ReactNode; center?: boolean; className?: string }) {
  return (
    <header className={`mb-14 sm:mb-20 ${center ? 'mx-auto max-w-[820px] text-center' : 'max-w-[860px]'} ${className}`}>
      {eyebrow && <Eyebrow>{eyebrow}</Eyebrow>}
      <Heading>{title}</Heading>
      {lead && <Lead className={`mt-6 ${center ? 'mx-auto max-w-[640px]' : 'max-w-[640px]'}`}>{lead}</Lead>}
    </header>
  )
}
