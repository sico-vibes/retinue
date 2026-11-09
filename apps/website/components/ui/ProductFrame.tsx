import Image from 'next/image'
import type { ReactNode } from 'react'

/** A Windows app window in the Haze look. Holds a real screenshot or a live mock. */
export function ProductFrame({ title, children, className = '' }: { title?: string; children: ReactNode; className?: string }) {
  return (
    <div className={`overflow-hidden rounded-[18px] border border-line-strong bg-page shadow-frame sm:rounded-[22px] ${className}`}>
      <div className="flex h-9 items-center border-b border-line px-3.5 sm:h-10">
        {title && <span className="truncate text-[13px] font-medium text-ink-2">{title}</span>}
        <span aria-hidden="true" className="ml-auto flex items-center gap-4 text-ink-3">
          <span className="block h-px w-2.5 bg-current" />
          <span className="block h-2.5 w-2.5 rounded-[2px] border border-current" />
          <svg viewBox="0 0 10 10" className="h-2.5 w-2.5" fill="none" stroke="currentColor"><path d="M1 1l8 8M9 1l-8 8" /></svg>
        </span>
      </div>
      {children}
    </div>
  )
}

/** A real product screenshot with its dark and light versions. */
export function Shot({ name, alt, width, height, sizes }: { name: string; alt: string; width: number; height: number; sizes: string }) {
  const image = (theme: 'dark' | 'light') => (
    <Image src={`/shots/${name}-${theme}.webp`} alt={alt} width={width} height={height} sizes={sizes} className={`h-auto w-full only-${theme}`} />
  )
  return (
    <>
      {image('dark')}
      {image('light')}
    </>
  )
}
