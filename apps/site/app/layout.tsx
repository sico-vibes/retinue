import type { Metadata, Viewport } from 'next'
import { Archivo, Geist, Geist_Mono } from 'next/font/google'
import './globals.css'

const display = Archivo({ subsets: ['latin'], axes: ['wdth'], variable: '--font-display-face', display: 'swap' })
const sans = Geist({ subsets: ['latin'], variable: '--font-sans-face', display: 'swap' })
const mono = Geist_Mono({ subsets: ['latin'], variable: '--font-mono-face', display: 'swap' })

export const metadata: Metadata = {
  title: 'Retinue: a team of AI engineers on your desktop',
  description: 'Hire coding agents as employees. Give them projects, queue the work, approve what matters, and review what they change, on this PC, your other machines and a computer of their own.',
}

export const viewport: Viewport = {
  themeColor: [
    { media: '(prefers-color-scheme: light)', color: '#f6f5f2' },
    { media: '(prefers-color-scheme: dark)', color: '#0b0b0c' },
  ],
}

// Picks the theme before the first paint: the visitor's choice, else their system's.
const themeScript = `(function(){try{var t=localStorage.getItem('retinue-theme');if(t!=='light'&&t!=='dark'){t=matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light'}document.documentElement.dataset.theme=t}catch(e){document.documentElement.dataset.theme='dark'}})()`

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return <html lang="en" data-theme="dark" suppressHydrationWarning className={`${display.variable} ${sans.variable} ${mono.variable}`}>
    <head><script dangerouslySetInnerHTML={{ __html: themeScript }} /></head>
    <body>{children}</body>
  </html>
}
