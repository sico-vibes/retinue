import Image from 'next/image'
import { footerColumns, site } from '@/content/site'
import { Container } from './layout'
import { Logo } from './Nav'

/** Closes the page: the team around the table, dissolving into the mist, then the links. */
export function Footer() {
  return (
    <footer className="relative">
      <div className="footer-art relative -mt-10 h-[clamp(240px,38vw,640px)] overflow-hidden sm:-mt-16">
        <Image src="/art/footer.webp" alt="The Retinue team gathered around a table, working through plans together." fill sizes="100vw" className="object-cover object-center" />
      </div>
      <Container className="relative pb-8">
        <div className="grid grid-cols-2 gap-x-6 gap-y-12 lg:grid-cols-[5fr_2fr_2fr_2fr]">
          <div className="col-span-2 lg:col-span-1">
            <div className="flex items-center gap-2.5 text-xl font-semibold tracking-[-0.015em]"><Logo size={28} /><span>Retinue</span></div>
            <p className="mt-4 max-w-[34ch] leading-relaxed text-ink-2">{site.tagline} Local to your computer, and answerable to you.</p>
          </div>
          {footerColumns.map((column) => (
            <nav key={column.title} aria-label={column.title}>
              <h2 className="mb-4 text-[13px] font-medium text-ink-3">{column.title}</h2>
              <ul className="grid gap-3">
                {column.links.map((link) => (
                  <li key={link.label}><a href={link.href} className="text-[15px] text-ink-2 transition-colors duration-150 hover:text-ink">{link.label}</a></li>
                ))}
              </ul>
            </nav>
          ))}
        </div>
        <p className="mt-16 border-t border-line pt-6 text-[13px] text-ink-3">© 2026 Retinue. Inter is used under the <a href="/LICENSE-Inter.txt" className="underline underline-offset-2">SIL Open Font License</a>.</p>
      </Container>
    </footer>
  )
}
