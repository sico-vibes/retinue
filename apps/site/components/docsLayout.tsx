import type { ReactNode } from 'react'
import { PageShell } from '@/components/shell'
import { Container } from '@/components/ui'
import { docs, SECTION_ORDER } from '@/lib/markdown'

/** The docs: a sidebar grouped by section, the page in the middle. */
export function DocsLayout({ current, children }: { current: string; children: ReactNode }) {
  const all = docs()
  const sections = SECTION_ORDER.map((name) => ({ name, pages: all.filter((page) => page.section === name) })).filter((section) => section.pages.length)
  return <PageShell>
    <Container className="grid gap-10 py-12 lg:grid-cols-[240px_minmax(0,1fr)]">
      <nav aria-label="Documentation" className="lg:sticky lg:top-24 lg:max-h-[calc(100vh-7rem)] lg:self-start lg:overflow-y-auto" data-lenis-prevent>
        {sections.map((section) => <div key={section.name} className="mb-6">
          <p className="eyebrow mb-2">{section.name}</p>
          <ul className="space-y-0.5">{section.pages.map((page) => <li key={page.slug}>
            <a href={page.slug === 'index' ? '/docs/' : `/docs/${page.slug}/`} aria-current={page.slug === current ? 'page' : undefined}
              className={`block rounded-[8px] px-2.5 py-1.5 text-[14px] transition ${page.slug === current ? 'bg-panel font-medium text-ink' : 'text-ink-3 hover:bg-panel hover:text-ink'}`}>{page.title}</a>
          </li>)}</ul>
        </div>)}
      </nav>
      <div className="min-w-0">{children}</div>
    </Container>
  </PageShell>
}
