'use client'
import PillNav from '@/components/bits/PillNav'
import { ThemeToggle, useTheme } from '@/components/theme'
import { DownloadButton } from '@/components/ui'
import { nav } from '@/content/site'

export function Nav() {
  const theme = useTheme()
  return <header className="fixed inset-x-0 top-0 z-50 border-b border-line bg-bg/70 backdrop-blur-xl">
    <div className="mx-auto flex h-16 w-full max-w-[1240px] items-center justify-between gap-4 px-5 sm:px-8">
      <a href="/" className="flex items-center gap-2.5" aria-label="Retinue home">
        <img src="/brand/retinue-mark-white.png" alt="" className="theme-dark size-6" />
        <img src="/brand/retinue-mark-black.png" alt="" className="theme-light size-6" />
        <span className="display text-[26px] leading-none tracking-wide">Retinue</span>
      </a>
      <div className="hidden md:block">
        <PillNav
          key={theme}
          items={nav}
          theme={theme}
          surface="none"
          itemStyle="plain"
          hoverEffect="glide"
          indicator="none"
          size="sm"
          intro={false}
          collapse="never"
          defaultActiveIndex={-1}
          textColor={theme === 'dark' ? '#c8c8cf' : '#3f3f46'}
          highlightColor={theme === 'dark' ? 'rgba(255,255,255,0.08)' : 'rgba(11,11,12,0.06)'}
          highlightTextColor={theme === 'dark' ? '#f4f4f5' : '#0b0b0c'}
          ariaLabel="Sections"
        />
      </div>
      <div className="flex items-center gap-2">
        <ThemeToggle />
        <div className="hidden sm:block"><DownloadButton label="Download" /></div>
      </div>
    </div>
  </header>
}
