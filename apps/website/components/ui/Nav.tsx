'use client'

import Image from 'next/image'
import { useState } from 'react'
import { downloadHref, navLinks } from '@/content/site'
import { DownloadButton } from './Button'
import { ThemeToggle } from './ThemeToggle'

export function Logo({ size = 26 }: { size?: number }) {
  return (
    <>
      <Image unoptimized src="/brand/retinue-mark-white.webp" alt="" width={size} height={size} className="only-dark" />
      <Image unoptimized src="/brand/retinue-mark-black.webp" alt="" width={size} height={size} className="only-light" />
    </>
  )
}

/** The floating frosted bar, fixed to the top of every page. */
export function Nav() {
  const [open, setOpen] = useState(false)
  return (
    <header className="fixed inset-x-0 top-2.5 z-50 px-2.5 sm:top-4 sm:px-6">
      <div className="glass frost mx-auto max-w-[1240px] rounded-[14px] shadow-soft sm:rounded-[18px]">
        <div className="flex h-[58px] items-center gap-6 pr-2.5 pl-4 sm:h-[64px] sm:pr-3 sm:pl-5">
          <a href="/" className="flex items-center gap-2.5 text-lg font-semibold tracking-[-0.015em]" aria-label="Retinue home" onClick={() => setOpen(false)}>
            <Logo />
            <span aria-hidden="true">Retinue</span>
          </a>
          <nav aria-label="Main" className="ml-3 hidden gap-1 lg:flex">
            {navLinks.map((link) => (
              <a key={link.href} href={link.href} className="rounded-lg px-3 py-2 text-[14.5px] text-ink-2 transition-colors duration-150 hover:text-ink">{link.label}</a>
            ))}
          </nav>
          <div className="ml-auto flex items-center gap-1.5 sm:gap-2">
            <ThemeToggle />
            <DownloadButton size="sm" label="Download" className="max-[429px]:hidden" />
            <button type="button" aria-expanded={open} aria-controls="nav-menu" aria-label={open ? 'Close menu' : 'Open menu'} onClick={() => setOpen((value) => !value)} className="grid h-11 w-11 place-items-center rounded-lg text-ink lg:hidden">
              <svg viewBox="0 0 18 18" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
                {open ? <path d="M4 4l10 10M14 4 4 14" /> : <path d="M2.5 6h13M2.5 12h13" />}
              </svg>
            </button>
          </div>
        </div>
        {open && (
          <nav id="nav-menu" aria-label="Mobile" className="grid border-t border-line p-2 lg:hidden">
            {navLinks.map((link) => (
              <a key={link.href} href={link.href} onClick={() => setOpen(false)} className="rounded-lg px-3 py-3 text-base text-ink-2 hover:text-ink">{link.label}</a>
            ))}
            <a href={downloadHref} onClick={() => setOpen(false)} className="rounded-lg px-3 py-3 text-base font-medium text-ink">Download for Windows</a>
          </nav>
        )}
      </div>
    </header>
  )
}
