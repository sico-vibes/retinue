import type { ReactNode } from 'react'
import { downloadHref } from '@/content/site'

const base = 'inline-flex items-center justify-center gap-2.5 rounded-[14px] font-medium tracking-[-0.01em] transition-[transform,opacity,background-color] duration-200 ease-haze hover:-translate-y-px active:translate-y-0'
const looks = {
  primary: 'bg-button text-button-ink hover:opacity-90',
  ghost: 'border border-line-strong bg-raised text-ink hover:bg-glass-solid',
}
const sizes = { md: 'px-5.5 py-3.5 text-base', sm: 'px-4 py-2.5 text-[14.5px] rounded-[11px]' }

export function ButtonLink({ href, children, look = 'primary', size = 'md', className = '' }: { href: string; children: ReactNode; look?: keyof typeof looks; size?: keyof typeof sizes; className?: string }) {
  return <a href={href} className={`${base} ${looks[look]} ${sizes[size]} ${className}`}>{children}</a>
}

export function WindowsIcon({ size = 15 }: { size?: number }) {
  return <svg viewBox="0 0 16 16" width={size} height={size} aria-hidden="true"><path fill="currentColor" d="M0 2.3 6.5 1.4v6.3H0Zm7.3-1 8.7-1.3v7.7H7.3ZM0 8.3h6.5v6.3L0 13.7Zm7.3 0H16V16l-8.7-1.2Z" /></svg>
}

export function DownloadButton({ size = 'md', label = 'Download for Windows', className = '' }: { size?: keyof typeof sizes; label?: string; className?: string }) {
  return <ButtonLink href={downloadHref} size={size} className={className}><WindowsIcon />{label}</ButtonLink>
}
