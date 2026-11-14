import { Container } from '@/components/ui'
import { nav, site } from '@/content/site'

export function Footer() {
  const columns = [
    { title: 'Retinue', links: [{ label: 'Download', href: site.download }, { label: 'Release notes', href: site.download }, { label: 'GitHub', href: site.github }] },
    { title: 'On this page', links: nav },
    { title: 'Help', links: [{ label: 'Report a problem', href: site.issues }] },
  ]
  return <footer className="border-t border-line bg-raise">
    <Container className="grid gap-12 py-16 md:grid-cols-[1.4fr_repeat(3,1fr)]">
      <div>
        <p className="display text-[44px] text-ink">Retinue</p>
        <p className="mt-3 max-w-[300px] text-[14px] leading-relaxed text-ink-3">A team of AI engineers on your desktop. Version {site.version}.</p>
      </div>
      {columns.map((column) => <nav key={column.title} aria-label={column.title}>
        <p className="eyebrow mb-4">{column.title}</p>
        <ul className="space-y-2.5">
          {column.links.map((link) => <li key={link.label}><a href={link.href} className="text-[14px] text-ink-2 transition hover:text-ink">{link.label}</a></li>)}
        </ul>
      </nav>)}
    </Container>
    <Container className="flex flex-col justify-between gap-3 border-t border-line py-6 font-mono text-[12px] text-ink-3 sm:flex-row">
      <span>© {new Date().getFullYear()} Retinue</span>
      <span>Made with a team of employees, most of them AI.</span>
    </Container>
  </footer>
}
