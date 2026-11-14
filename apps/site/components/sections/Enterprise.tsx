'use client'
// For teams: every employee's ID badge, clipped to two cords that run across the page and slide past
// in opposite directions (hover stops a row; a badge opens the team page). Then what is coming next.
import Link from 'next/link'
import { Button, Container, Reveal, SectionHead } from '@/components/ui'
import { people, type Person } from '@/content/roster'
import { next, site } from '@/content/site'

function Row({ list, reverse, time }: { list: Person[]; reverse?: boolean; time: string }) {
  // Two copies side by side, so moving by half the width loops without a seam.
  const doubled = [...list, ...list]
  return <div className="relative">
    {/* The cord the badges are clipped to. */}
    <div className="absolute inset-x-0 top-[13px] h-[3px] bg-line-strong" aria-hidden="true" />
    <div className="marquee flex w-max gap-7 px-4" style={{ '--marquee-time': time, animationDirection: reverse ? 'reverse' : 'normal' } as React.CSSProperties}>
      {doubled.map((person, index) => <Link key={`${person.id}-${index}`} href="/team/" aria-hidden={index >= list.length || undefined} tabIndex={index >= list.length ? -1 : undefined}
        className="group relative flex w-[150px] shrink-0 flex-col items-center" style={{ transform: `rotate(${((index * 7) % 5) - 2}deg)`, transformOrigin: 'top center' }}>
        {/* Clip and strap in the employee's colour. */}
        <span className="relative z-10 h-[26px] w-[14px] rounded-[4px] border border-white/20" style={{ background: person.color }} aria-hidden="true" />
        <span className="-mt-1 h-3 w-7 rounded-b-[6px] bg-[#9aa0a6]" aria-hidden="true" />
        <img src={`/badges/${person.id}.png`} alt={`${person.name}, ${person.role}`} loading="lazy"
          className="mt-1 w-full rounded-[12px] shadow-[0_18px_30px_-12px_rgb(0_0_0/0.55)] transition-transform duration-300 group-hover:-translate-y-1.5 group-hover:rotate-1" />
      </Link>)}
    </div>
  </div>
}

export function Enterprise() {
  const half = Math.ceil(people.length / 2)
  return <section className="relative overflow-hidden border-t border-line py-[clamp(96px,14vh,160px)]">
    <Container>
      <SectionHead id="enterprise" eyebrow="For teams" title={<>A whole department<br />in one go</>}
        lead="Hire the roles you are missing, all at once, each with its own skills and personality."
        action={<div className="flex gap-3"><Button href="/team/">Meet the team</Button><Button href={site.issues} variant="ghost">Ask about teams</Button></div>} />
    </Container>
    <Reveal className="mt-14 grid gap-12 [mask-image:linear-gradient(to_right,transparent,black_8%,black_92%,transparent)]">
      <Row list={people.slice(0, half)} time="90s" />
      <Row list={people.slice(half)} reverse time="100s" />
    </Reveal>
    <Container>
      <div className="mt-20 grid items-start gap-8 rounded-[18px] border border-line bg-raise p-8 lg:grid-cols-[1fr_2fr]">
        <div><p className="eyebrow">Coming next</p><h3 className="display mt-2 text-[40px] text-ink">More autonomy</h3></div>
        <ul className="grid gap-3 sm:grid-cols-2">{next.map((item) => <li key={item} className="flex items-start gap-3 text-[15px] text-ink-2"><i className="mt-2 size-1.5 shrink-0 rounded-full bg-accent" />{item}</li>)}</ul>
      </div>
    </Container>
  </section>
}
