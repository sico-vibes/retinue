// The status badge of Professional mode (owner, 24 November 2026, after T3 Code and Synara): a small rounded tile with a
// round icon and a word, in place of the plain coloured text. The icon says the state at a glance: a dashed circle is a
// draft, a half circle is work in progress, a pie is waiting on you, a tick is done. See workMode.ts.
import type { Session } from '../types'
import { conversationStatus } from './ProjectsSidebar'

export type ProTone = 'draft' | 'working' | 'needs' | 'done' | 'bad' | 'limited'

/** Retinue's own words for what a conversation is doing, as badge tones: idle with something said is Completed. */
export function proBadgeFor(session: Pick<Session, 'state'> & Partial<Session>, needsYou: boolean): { tone: ProTone; label: string } {
  const status = conversationStatus(session as Session, needsYou)
  if (status.tone === 'needs') return { tone: 'needs', label: 'Needs you' }
  if (status.tone === 'working') return { tone: 'working', label: 'Working' }
  if (status.tone === 'limited') return { tone: 'limited', label: 'Limited' }
  if (status.tone === 'bad') return { tone: 'bad', label: 'Failed' }
  return { tone: 'done', label: 'Completed' }
}

function Icon({ tone }: { tone: ProTone }) {
  const common = { width: 13, height: 13, viewBox: '0 0 16 16', 'aria-hidden': true as const }
  if (tone === 'draft') return <svg {...common}><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.5" strokeDasharray="2.6 2.4" /></svg>
  if (tone === 'working') return <svg {...common}><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.5" /><path d="M8 4.2a3.8 3.8 0 0 1 0 7.6z" fill="currentColor" /></svg>
  if (tone === 'needs') return <svg {...common}><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.5" /><path d="M8 8V3.8A4.2 4.2 0 1 1 3.8 8z" fill="currentColor" /></svg>
  if (tone === 'limited') return <svg {...common}><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.5" /><path d="M5.4 8h5.2" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" /></svg>
  if (tone === 'bad') return <svg {...common}><circle cx="8" cy="8" r="7" fill="currentColor" /><path d="M5.6 5.6l4.8 4.8M10.4 5.6l-4.8 4.8" stroke="var(--pro-badge-ink)" strokeWidth="1.6" strokeLinecap="round" /></svg>
  return <svg {...common}><circle cx="8" cy="8" r="7" fill="currentColor" /><path d="M5 8.2l2.3 2.3L11.2 6" fill="none" stroke="var(--pro-badge-ink)" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" /></svg>
}

export function ProBadge({ tone, label }: { tone: ProTone; label: string }) {
  return <span className={`pro-badge ${tone}`}><Icon tone={tone} />{label}</span>
}
