'use client'
// For teams: the whole roster as a spotlight grid (hover lights each employee, click opens them),
// and what is coming next.
import ChromaGrid from '@/components/bits/ChromaGrid'
import { useTheme } from '@/components/theme'
import { Button, Container, Reveal, SectionHead } from '@/components/ui'
import { people } from '@/content/roster'
import { next, site } from '@/content/site'

export function Enterprise() {
  const theme = useTheme()
  const items = people.map((person) => ({ image: person.photo, title: person.name, subtitle: person.role, accent: person.color, borderColor: person.color, url: `#employee-${person.id}`, alt: `${person.name}, ${person.role}` }))
  return <section className="relative border-t border-line py-[clamp(96px,14vh,160px)]">
    <Container>
      <SectionHead id="enterprise" eyebrow="For teams" title={<>Staff a whole<br />department</>} lead="Hire the roles you are missing, all at once, each with its own skills and personality." action={<Button href={site.issues} variant="ghost">Ask about teams</Button>} />
      <Reveal className="mt-14">
        <ChromaGrid items={items} theme={theme} columns={7} minCardWidth={120} gap={10} aspectRatio="4 / 5" cardRadius={14} grayscale={0.85} showInfo frame={false} />
      </Reveal>
      <div className="mt-16 grid items-start gap-8 rounded-[18px] border border-line bg-raise p-8 lg:grid-cols-[1fr_2fr]">
        <div><p className="eyebrow">Coming next</p><h3 className="display mt-2 text-[40px] text-ink">More autonomy</h3></div>
        <ul className="grid gap-3 sm:grid-cols-2">{next.map((item) => <li key={item} className="flex items-start gap-3 text-[15px] text-ink-2"><i className="mt-2 size-1.5 shrink-0 rounded-full bg-accent" />{item}</li>)}</ul>
      </div>
    </Container>
  </section>
}
