import type { Metadata } from 'next'
import { notFound } from 'next/navigation'
import { DocsLayout } from '@/components/docsLayout'
import { docs, render } from '@/lib/markdown'

export function generateStaticParams() {
  return docs().filter((page) => page.slug !== 'index').map((page) => ({ slug: page.slug }))
}

export async function generateMetadata({ params }: { params: Promise<{ slug: string }> }): Promise<Metadata> {
  const { slug } = await params
  const page = docs().find((item) => item.slug === slug)
  return { title: page ? `${page.title} · Retinue docs` : 'Retinue docs', description: page?.description }
}

export default async function DocPage({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params
  const all = docs()
  const index = all.findIndex((item) => item.slug === slug)
  if (index < 0) notFound()
  const page = all[index]
  const previous = all[index - 1]
  const next = all[index + 1]
  return <DocsLayout current={slug}>
    <p className="eyebrow">{page.section}</p>
    <h1 className="display mt-2 text-[clamp(44px,5.4vw,76px)] text-ink">{page.title}</h1>
    {page.description && <p className="mt-4 max-w-[640px] text-[18px] leading-relaxed text-ink-2">{page.description}</p>}
    <article className="prose-retinue mt-8" dangerouslySetInnerHTML={{ __html: render(page.body) }} />
    <nav aria-label="More pages" className="mt-16 grid gap-3 border-t border-line pt-8 sm:grid-cols-2">
      {previous ? <a href={previous.slug === 'index' ? '/docs/' : `/docs/${previous.slug}/`} className="rounded-[14px] border border-line p-4 transition hover:border-line-strong"><span className="eyebrow">Previous</span><p className="mt-1 font-medium text-ink">{previous.title}</p></a> : <span />}
      {next && <a href={`/docs/${next.slug}/`} className="rounded-[14px] border border-line p-4 text-right transition hover:border-line-strong"><span className="eyebrow">Next</span><p className="mt-1 font-medium text-ink">{next.title}</p></a>}
    </nav>
  </DocsLayout>
}
