import type { ReactNode } from 'react'

/** An endless row. Pure CSS: two copies moved half their width, paused on hover, static with reduced motion. */
export function Marquee({ children, duration = 60, reverse = false, label }: { children: ReactNode; duration?: number; reverse?: boolean; label?: string }) {
  return (
    <div className="marquee-row marquee-mask overflow-hidden py-1.5" role={label ? 'group' : undefined} aria-label={label}>
      <div className="marquee-track flex w-max" data-reverse={reverse} style={{ '--duration': `${duration}s` } as React.CSSProperties}>
        <div className="flex shrink-0 gap-3 pr-3">{children}</div>
        <div className="flex shrink-0 gap-3 pr-3" aria-hidden="true" data-copy="true">{children}</div>
      </div>
    </div>
  )
}
