'use client'

import { m, useInView } from 'motion/react'
import { useRef, useState } from 'react'
import { Avatar } from '@/components/ui/Avatar'
import { easeHaze } from '@/components/ui/Motion'
import { StreamText } from '@/components/ui/Reveal'
import { ProductFrame } from '@/components/ui/ProductFrame'

const sidebar: { id: string; name: string; status: string; tone?: 'amber' }[] = [
  { id: 'ada', name: 'Ada', status: 'Needs you', tone: 'amber' },
  { id: 'fifi', name: 'Fifi', status: 'Working', tone: 'amber' },
  { id: 'nubi', name: 'Nubi', status: 'Free' },
  { id: 'will', name: 'Will', status: 'Free' },
]

type Answer = 'waiting' | 'allowed' | 'denied'

/** A small Haze window with a reply streaming in and an approval you can really answer. */
export function LiveWindow() {
  const ref = useRef<HTMLDivElement>(null)
  const inView = useInView(ref, { once: true, margin: '0px 0px -20% 0px' })
  const [answer, setAnswer] = useState<Answer>('waiting')

  return (
    <div ref={ref}>
      <ProductFrame title="Storefront / Checkout summary">
        <div className="grid min-h-[420px] sm:grid-cols-[210px_minmax(0,1fr)]">
          <aside aria-hidden="true" className="hidden border-r border-line p-2.5 sm:block">
            {sidebar.map((row, index) => (
              <div key={row.id} className={`flex items-center gap-2.5 rounded-[11px] px-2 py-2 ${index === 0 ? 'bg-raised' : ''}`}>
                <Avatar id={row.id} size={32} />
                <span className="text-sm font-medium">{row.name}</span>
                <span className={`ml-auto text-xs ${row.tone === 'amber' ? 'text-amber' : 'text-ink-3'}`}>{index === 0 && answer !== 'waiting' ? 'Working' : row.status}</span>
              </div>
            ))}
          </aside>
          <div className="flex flex-col justify-end gap-4 p-4 sm:p-6">
            <p className="ml-auto w-fit max-w-[85%] rounded-[18px] bg-raised px-4 py-2.5 text-[15px]">Can you make the checkout tests pass?</p>
            <div className="flex gap-3">
              <Avatar id="ada" size={32} className="mt-0.5" />
              <p className="pt-1 text-[15px] leading-relaxed"><StreamText active={inView} delay={0.3} text="Two cases fail because the empty cart isn’t handled. I’ve fixed that. I need to run the tests to confirm." /></p>
            </div>

            <m.div initial={{ opacity: 0, y: 12 }} animate={inView ? { opacity: 1, y: 0 } : undefined} transition={{ delay: 1.5, duration: 0.5, ease: easeHaze }} className="rounded-2xl border border-line-strong bg-raised p-4">
              <p className="text-sm font-medium">Run shell command</p>
              <code className="mt-2.5 block rounded-lg bg-well px-3 py-2 font-mono text-[13px]">npm test -- checkout</code>
              {answer === 'waiting' ? (
                <div className="mt-3 flex flex-wrap gap-2">
                  <button type="button" onClick={() => setAnswer('allowed')} className="min-h-11 rounded-[11px] bg-button px-4 text-sm font-medium text-button-ink transition-opacity hover:opacity-90">Allow once</button>
                  <button type="button" onClick={() => setAnswer('allowed')} className="min-h-11 rounded-[11px] border border-line-strong px-4 text-sm font-medium hover:bg-raised">Allow this session</button>
                  <button type="button" onClick={() => setAnswer('denied')} className="min-h-11 rounded-[11px] border border-line-strong px-4 text-sm font-medium hover:bg-raised">Deny</button>
                </div>
              ) : (
                <p role="status" className={`mt-3 text-sm font-medium ${answer === 'allowed' ? 'text-green' : 'text-ink-3'}`}>{answer === 'allowed' ? 'Allowed. 14 tests passed.' : 'Denied. Nothing was run.'}</p>
              )}
            </m.div>

            {answer !== 'waiting' && (
              <div className="flex gap-3">
                <Avatar id="ada" size={32} className="mt-0.5" />
                <p className="pt-1 text-[15px] leading-relaxed"><StreamText active delay={0.1} text={answer === 'allowed' ? 'All 14 pass. The change is ready for Nubi to review.' : 'No problem. I’ve left the fix in place, and you can run the tests whenever you like.'} /></p>
              </div>
            )}
            {answer !== 'waiting' && <button type="button" onClick={() => setAnswer('waiting')} className="min-h-8 self-start rounded-lg px-1 text-[13px] text-ink-3 underline underline-offset-2 hover:text-ink">Answer again</button>}
          </div>
        </div>
      </ProductFrame>
    </div>
  )
}
