import type { Metadata, Viewport } from 'next'
import localFont from 'next/font/local'
import type { ReactNode } from 'react'
import { Footer } from '@/components/ui/Footer'
import { RevealObserver } from '@/components/ui/Reveal'
import { Nav } from '@/components/ui/Nav'
import { site } from '@/content/site'
import './globals.css'

const inter = localFont({ src: './inter-var.woff2', weight: '100 900', variable: '--font-inter', display: 'swap' })

export const metadata: Metadata = {
  title: { default: `${site.name}: your company of AI employees`, template: `%s · ${site.name}` },
  description: site.description,
  openGraph: { title: `${site.name}: your company of AI employees`, description: site.description, type: 'website' },
}

export const viewport: Viewport = {
  themeColor: [
    { media: '(prefers-color-scheme: dark)', color: '#1a1b1d' },
    { media: '(prefers-color-scheme: light)', color: '#f1efeb' },
  ],
}

// Runs before paint so the page never flashes the wrong theme. A saved choice wins, then the system; dark otherwise.
const themeScript = `try{var t=localStorage.getItem('retinue-theme');if(t!=='dark'&&t!=='light'){t=matchMedia('(prefers-color-scheme: light)').matches?'light':'dark'}document.documentElement.dataset.theme=t}catch(e){}`

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" data-theme="dark" className={inter.variable} suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
        <noscript><style>{'[data-reveal]{opacity:1!important;transform:none!important}'}</style></noscript>
      </head>
      <body>
        <a href="#main" className="fixed top-[-80px] left-4 z-[60] rounded-lg bg-button px-4 py-2.5 font-medium text-button-ink focus:top-3">Skip to content</a>
        <div className="haze" aria-hidden="true" />
        <Nav />
        <main id="main">{children}</main>
        <Footer />
        <RevealObserver />
      </body>
    </html>
  )
}
