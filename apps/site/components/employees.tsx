'use client'
// Employee cards in the app's hire-card style (tinted with the employee's colour, a large faint role
// icon behind), and the dialog that opens for any link to #employee-<id>, from any page.
import { createElement, useEffect, useState } from 'react'
import { AnimatePresence, motion } from 'motion/react'
import { X } from 'lucide-react'
import { roleIcon } from '@/components/roleIcon'
import { byId, departmentName, skillLabel, type Person } from '@/content/roster'
import { site } from '@/content/site'

/** With onSelect, the card is a button that puts this employee on the badge; without it, a link to their dialog. */
export function EmployeeCard({ person, compact = false, onSelect, selected = false }: { person: Person; compact?: boolean; onSelect?: () => void; selected?: boolean }) {
  const className = `group relative isolate flex h-full w-full flex-col overflow-hidden rounded-[16px] border p-5 text-left transition-[transform,border-color,box-shadow] duration-300 hover:-translate-y-0.5 ${selected ? 'border-transparent' : 'border-line hover:border-line-strong'}`
  const style = { background: `color-mix(in srgb, ${person.color} ${selected ? 16 : 8}%, var(--bg-raise))`, boxShadow: selected ? `0 0 0 2px ${person.color}` : undefined }
  const Tag = onSelect ? 'button' : 'a'
  const props = onSelect ? { type: 'button' as const, onClick: onSelect, 'aria-pressed': selected, 'aria-label': `${person.name}, ${person.role}. Show their badge` } : { href: `#employee-${person.id}`, 'aria-label': `${person.name}, ${person.role}. Read more` }
  return <Tag {...props} className={className} style={style}>
    {createElement(roleIcon(person.role), { 'aria-hidden': true, strokeWidth: 1.1, className: 'pointer-events-none absolute -right-[12%] -top-[4%] -z-10 size-[62%] -rotate-[10deg] opacity-20 transition-transform duration-500 [mask-image:linear-gradient(to_bottom,black,transparent)] group-hover:-rotate-[6deg] group-hover:scale-105', style: { color: person.color } })}
    <img src={person.photo} alt="" className="size-[68px] rounded-full object-cover" style={{ boxShadow: `0 0 0 2px color-mix(in srgb, ${person.color} 55%, transparent)` }} loading="lazy" />
    <p className="mt-4 text-[19px] font-semibold text-ink">{person.name}</p>
    <p className="text-[14px] font-medium text-ink-2">{person.role}</p>
    {!compact && <p className="mt-2 line-clamp-2 text-[14px] leading-relaxed text-ink-3">{person.description}</p>}
    {!compact && <div className="mt-auto flex flex-wrap gap-1.5 pt-4">{person.tags.slice(0, 3).map((tag) => <span key={tag} className="rounded-full border border-line px-2 py-0.5 font-mono text-[11px] text-ink-3">{tag}</span>)}</div>}
  </Tag>
}

/** Opens for #employee-<id>; closing clears the hash without jumping the page. */
export function EmployeeDialog() {
  const [id, setId] = useState<string | null>(null)
  useEffect(() => {
    const read = () => { const match = /^#employee-([a-z0-9-]+)$/.exec(location.hash); setId(match ? match[1] : null) }
    read()
    window.addEventListener('hashchange', read)
    return () => window.removeEventListener('hashchange', read)
  }, [])
  const person = id ? byId(id) : null
  const close = () => { history.replaceState(null, '', location.pathname + location.search); setId(null) }
  useEffect(() => {
    if (!person) return
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') close() }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [person])
  return <AnimatePresence>
    {person && <motion.div key="backdrop" className="fixed inset-0 z-[100] grid place-items-center bg-black/55 p-4 backdrop-blur-sm" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} onClick={close} data-lenis-prevent>
      <motion.section role="dialog" aria-modal="true" aria-labelledby="employee-name" onClick={(event) => event.stopPropagation()}
        className="relative max-h-[88vh] w-full max-w-[640px] overflow-y-auto rounded-[22px] border border-line-strong bg-raise shadow-card"
        initial={{ opacity: 0, y: 24, scale: 0.96 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, y: 12, scale: 0.98 }} transition={{ type: 'spring', stiffness: 320, damping: 30 }}>
        <div className="relative h-[150px] overflow-hidden" style={{ background: `linear-gradient(160deg, color-mix(in srgb, ${person.color} 45%, transparent), transparent 80%)` }}>
          {createElement(roleIcon(person.role), { 'aria-hidden': true, strokeWidth: 1, className: 'absolute -right-6 -top-6 size-56 -rotate-12 opacity-25', style: { color: person.color } })}
          <button type="button" onClick={close} aria-label="Close" className="absolute right-4 top-4 grid size-9 place-items-center rounded-full bg-bg/60 text-ink backdrop-blur transition hover:bg-bg"><X size={16} /></button>
        </div>
        <div className="-mt-14 px-7 pb-7">
          <img src={person.photo} alt="" className="size-28 rounded-full border-4 border-raise object-cover" />
          <p className="eyebrow mt-4">{departmentName(person.department)}</p>
          <h2 id="employee-name" className="display mt-1 text-[56px] text-ink">{person.name}</h2>
          <p className="text-[17px] font-medium text-ink-2">{person.role}</p>
          <p className="mt-4 text-[16px] leading-relaxed text-ink-2">{person.description}</p>
          {person.personality && <div className="mt-6 rounded-[14px] border border-line bg-panel p-5">
            <p className="eyebrow">Default personality · {person.personality.name}</p>
            <p className="mt-2 text-[15px] text-ink-2">{person.personality.summary}</p>
            <p className="mt-3 border-l-2 pl-3 text-[15px] italic leading-relaxed text-ink-3" style={{ borderColor: person.color }}>“{person.personality.example}”</p>
            <p className="mt-3 text-[13px] text-ink-3">Change it any time from the employee&apos;s page.</p>
          </div>}
          <p className="eyebrow mt-6">Comes with these skills</p>
          <div className="mt-3 flex flex-wrap gap-1.5">{person.skills.map((skill) => <span key={skill} className="rounded-full border border-line bg-panel px-2.5 py-1 text-[13px] text-ink-2">{skillLabel(skill)}</span>)}</div>
          <div className="mt-7 flex flex-wrap gap-3">
            <a href={site.download} className="inline-flex h-11 items-center rounded-full bg-ink px-5 text-[15px] font-medium text-bg transition hover:bg-ink-2">Hire {person.name} in Retinue</a>
            <a href="/team/" className="inline-flex h-11 items-center rounded-full border border-line-strong px-5 text-[15px] font-medium text-ink transition hover:border-ink">See the whole team</a>
          </div>
        </div>
      </motion.section>
    </motion.div>}
  </AnimatePresence>
}
