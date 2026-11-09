import type { ReactNode } from 'react'
import { Heading } from './layout'

/** A numbered capability: a visual above, then its title and one or two lines. */
export function FeatureCard({ index, title, body, children, className = '' }: { index: number; title: string; body: string; children: ReactNode; className?: string }) {
  return (
    <article className={`flex h-full flex-col ${className}`}>
      <div className="glass flex-1 overflow-hidden rounded-[22px] shadow-soft">{children}</div>
      <div className="mt-6 flex gap-4 px-1">
        <span aria-hidden="true" className="pt-1.5 font-mono text-xs text-ink-3">{String(index).padStart(2, '0')}</span>
        <div>
          <Heading as="h3" size="card">{title}</Heading>
          <p className="mt-2 max-w-[46ch] leading-relaxed text-ink-2">{body}</p>
        </div>
      </div>
    </article>
  )
}
