import type { Metadata } from 'next'
import { PageShell } from '@/components/shell'
import { Container, DownloadButton } from '@/components/ui'
import { releases } from '@/lib/markdown'
import { site } from '@/content/site'

export const metadata: Metadata = { title: 'Changelog · Retinue', description: 'Every Retinue release and what changed in it.' }

export default function ChangelogPage() {
  const list = releases()
  return <PageShell>
    <Container className="pb-12 pt-[clamp(72px,12vh,120px)]">
      <p className="eyebrow">{list.length} releases</p>
      <h1 className="display mt-3 text-[clamp(56px,8vw,120px)] text-ink">Changelog</h1>
      <div className="mt-8 flex flex-wrap gap-3"><DownloadButton label="Download the latest" /><a href={site.download} className="inline-flex h-11 items-center rounded-full border border-line-strong px-5 text-[15px] font-medium text-ink transition hover:border-ink">All downloads on GitHub</a></div>
    </Container>
    <Container className="pb-24">
      <ol className="relative">
        {list.map((release, index) => <li key={release.version} id={release.version} className="grid scroll-mt-24 gap-4 border-t border-line py-12 md:grid-cols-[220px_1fr]">
          <div className="md:sticky md:top-24 md:self-start">
            <p className="display text-[34px] text-ink">{release.version}</p>
            {release.date && <p className="mt-1 font-mono text-[12px] text-ink-3">{release.date}</p>}
            {index === 0 && <span className="mt-3 inline-block rounded-full bg-accent-soft px-2.5 py-1 font-mono text-[11px] text-accent">Latest</span>}
          </div>
          <article className="prose-retinue" dangerouslySetInnerHTML={{ __html: release.html }} />
        </li>)}
      </ol>
    </Container>
  </PageShell>
}
