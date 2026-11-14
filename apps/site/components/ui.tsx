'use client'
// Shared building blocks: section header (object, title, lead, action), buttons, art, frames, dividers.
import { useState, useSyncExternalStore, type ReactNode } from 'react'
import AnimatedContent from '@/components/bits/AnimatedContent'
import { cn } from '@/lib/utils'
import { site } from '@/content/site'

const noSubscription = () => () => {}

export function Container({ children, className = '' }: { children: ReactNode; className?: string }) {
  return <div className={cn('mx-auto w-full max-w-[1240px] px-5 sm:px-8', className)}>{children}</div>
}

/** A picture Iris made. Rendered once the page is live, so a missing file shows nothing at all. */
export function Art({ src, alt = '', className = '', style }: { src: string; alt?: string; className?: string; style?: React.CSSProperties }) {
  const [failed, setFailed] = useState(false)
  const live = useSyncExternalStore(noSubscription, () => true, () => false)
  if (failed || !live) return null
  return <img src={src} alt={alt} className={className} style={style} loading="lazy" decoding="async" draggable={false} onError={() => setFailed(true)} />
}

/** Iris's ink line art: white ink in the dark theme, black in the light. */
export function Lines({ name, className = '', style }: { name: string; className?: string; style?: React.CSSProperties }) {
  return <>
    <Art src={`/art/lines/${name}-dark.webp`} className={cn('theme-dark', className)} style={style} />
    <Art src={`/art/lines/${name}-light.webp`} className={cn('theme-light', className)} style={style} />
  </>
}

/** Reveal on scroll: a short rise and fade, once. */
export function Reveal({ children, delay = 0, className = '' }: { children: ReactNode; delay?: number; className?: string }) {
  return <AnimatedContent distance={28} duration={0.9} ease="power3.out" initialOpacity={0} delay={delay} threshold={0.12} className={className}>{children}</AnimatedContent>
}

export function SectionHead({ object, eyebrow, title, lead, action, id, center = false }: { object?: string; eyebrow?: string; title: ReactNode; lead?: ReactNode; action?: ReactNode; id?: string; center?: boolean }) {
  return <div id={id} className={cn('flex scroll-mt-24 flex-col gap-6', center ? 'items-center text-center' : 'md:flex-row md:items-end md:justify-between')}>
    <Reveal className={cn('max-w-[720px]', center && 'flex flex-col items-center')}>
      {object && <Art src={object} className="mb-5 size-[92px] object-contain drop-shadow-[0_14px_24px_rgb(0_0_0/0.22)]" />}
      {eyebrow && <p className="eyebrow mb-3">{eyebrow}</p>}
      <h2 className="display text-[clamp(44px,6vw,88px)] text-ink">{title}</h2>
      {lead && <p className="mt-5 max-w-[520px] text-[18px] leading-relaxed text-ink-2">{lead}</p>}
    </Reveal>
    {action && <Reveal delay={0.1} className="shrink-0">{action}</Reveal>}
  </div>
}

export function Button({ href, children, variant = 'primary', className = '' }: { href: string; children: ReactNode; variant?: 'primary' | 'ghost'; className?: string }) {
  return <a href={href} className={cn(
    'group/button relative inline-flex h-11 items-center gap-2 overflow-hidden rounded-full px-5 text-[15px] font-medium transition-[background,color,border-color,box-shadow] duration-200',
    variant === 'primary' ? 'bg-ink text-bg shadow-[0_0_0_0_var(--accent)] hover:shadow-[0_0_0_4px_var(--accent-soft)]' : 'border border-line-strong text-ink hover:border-ink hover:bg-ink/5',
    className,
  )}>{children}</a>
}

export function DownloadButton({ label = 'Download for Windows' }: { label?: string }) {
  return <Button href={site.download}>
    <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M3 5.5 10.5 4.4v7H3zM11.5 4.3 21 3v8.4h-9.5zM3 12.5h7.5v7.1L3 18.5zM11.5 12.5H21V21l-9.5-1.3z" /></svg>
    {label}
  </Button>
}

/** A product screenshot in a window frame, one picture per theme. */
export function Frame({ shot, alt, className = '', imgClassName = '', bar = true }: { shot: string; alt: string; className?: string; imgClassName?: string; bar?: boolean }) {
  return <div className={cn('relative overflow-hidden rounded-[14px] border border-line-strong bg-raise shadow-card', className)}>
    {bar && <div className="flex h-8 items-center gap-1.5 border-b border-line px-3" aria-hidden="true">
      <i className="size-2.5 rounded-full bg-line-strong" /><i className="size-2.5 rounded-full bg-line-strong" /><i className="size-2.5 rounded-full bg-line-strong" />
    </div>}
    <img src={`/shots/${shot}-dark.webp`} alt={alt} className={cn('theme-dark block w-full', imgClassName)} loading="lazy" decoding="async" />
    <img src={`/shots/${shot}-light.webp`} alt="" aria-hidden="true" className={cn('theme-light block w-full', imgClassName)} loading="lazy" decoding="async" />
  </div>
}

/** A plant or prop hanging from the bottom edge of the element above it, swaying a little. */
export function Hanging({ src, className = '' }: { src: string; className?: string }) {
  return <Art src={src} className={cn('pointer-events-none absolute top-full z-20 w-[110px] origin-top animate-[sway_7s_ease-in-out_infinite]', className)} />
}
