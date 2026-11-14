// Reads the Markdown the site shows: customer docs (content/docs) and the app's release notes
// (docs/releases in the repository). Runs at build time only.
import { readFileSync, readdirSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { marked } from 'marked'

const site = process.cwd()
const docsDir = join(site, 'content', 'docs')
const releasesDir = join(site, '..', '..', 'docs', 'releases')

export type Doc = { slug: string; title: string; description: string; section: string; order: number; body: string }

function frontMatter(text: string): { data: Record<string, string>; body: string } {
  const match = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/.exec(text)
  if (!match) return { data: {}, body: text }
  const data: Record<string, string> = {}
  for (const line of match[1].split(/\r?\n/)) {
    const at = line.indexOf(':')
    if (at > 0) data[line.slice(0, at).trim()] = line.slice(at + 1).trim().replace(/^["']|["']$/g, '')
  }
  return { data, body: text.slice(match[0].length) }
}

/** An image with a -dark name also gets its -light twin, and each shows only in its own theme. */
function themedImages(html: string): string {
  return html.replace(/<img src="([^"]+)-dark\.webp" alt="([^"]*)"[^>]*>/g, (_all, base: string, alt: string) =>
    `<img src="${base}-dark.webp" alt="${alt}" class="theme-dark" loading="lazy"><img src="${base}-light.webp" alt="" aria-hidden="true" class="theme-light" loading="lazy">`)
}

export function render(markdown: string): string {
  return themedImages(marked.parse(markdown, { async: false }) as string)
}

export const SECTION_ORDER = ['Overview', 'Getting started', 'Employees', 'Working with your team', 'Anywhere', 'Connect', 'Privacy and help']

export function docs(): Doc[] {
  if (!existsSync(docsDir)) return []
  return readdirSync(docsDir).filter((name) => name.endsWith('.md')).map((name) => {
    const parsed = frontMatter(readFileSync(join(docsDir, name), 'utf8'))
    const data = parsed.data
    // The page shows its own title, so the Markdown's own leading "# Title" is dropped.
    const body = parsed.body.replace(/^\s*#\s+.*\r?\n/, '')
    return { slug: name.replace(/\.md$/, ''), title: data.title ?? name, description: data.description ?? '', section: data.section ?? 'Overview', order: Number(data.order ?? 99), body }
  }).sort((a, b) => (SECTION_ORDER.indexOf(a.section) - SECTION_ORDER.indexOf(b.section)) || a.order - b.order)
}

export type Release = { version: string; title: string; html: string; date: string | null }

const versionKey = (version: string) => {
  const [core, pre = ''] = version.split('-')
  const [major, minor, patch] = core.split('.').map(Number)
  const preNumber = Number(pre.replace(/\D+/g, '') || 0)
  return major * 1e9 + minor * 1e6 + patch * 1e3 + (pre ? preNumber : 999)
}

export function releases(): Release[] {
  if (!existsSync(releasesDir)) return []
  return readdirSync(releasesDir).filter((name) => /^\d+\.\d+\.\d+.*\.md$/.test(name)).map((name) => {
    const text = readFileSync(join(releasesDir, name), 'utf8')
    const version = name.replace(/\.md$/, '')
    // Drop the note's own title and its installation section: the page has its own heading and one download link.
    const body = text.replace(/^# .*\r?\n/, '').replace(/\r?\n## Installation[\s\S]*$/, '\n')
    const date = /(\d{1,2} (January|February|March|April|May|June|July|August|September|October|November|December) 20\d\d)/.exec(text)?.[1] ?? null
    return { version, title: `Retinue ${version}`, html: render(body), date }
  }).sort((a, b) => versionKey(b.version) - versionKey(a.version))
}
