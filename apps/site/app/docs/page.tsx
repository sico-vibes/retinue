import type { Metadata } from 'next'
import { DocsLayout } from '@/components/docsLayout'
import { docs, render } from '@/lib/markdown'

export const metadata: Metadata = { title: 'Docs · Retinue', description: 'How to use Retinue: hire employees, work with your team, and run it anywhere.' }

export default function DocsHome() {
  const page = docs().find((item) => item.slug === 'index')
  return <DocsLayout current="index">
    <p className="eyebrow">Documentation</p>
    <h1 className="display mt-2 text-[clamp(48px,6vw,84px)] text-ink">{page?.title ?? 'Retinue docs'}</h1>
    {page && <article className="prose-retinue mt-8" dangerouslySetInnerHTML={{ __html: render(page.body) }} />}
  </DocsLayout>
}
