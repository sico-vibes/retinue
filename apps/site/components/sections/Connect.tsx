'use client'
// Bring your own tools: the Retinue MCP, dictation and the coding agents, in three plain cards.
import { useState } from 'react'
import { Check, Copy, Mic, Plug, Bot } from 'lucide-react'
import { Art, Container, Reveal, SectionHead } from '@/components/ui'
import { connect } from '@/content/site'

const icons = [Plug, Mic, Bot]

function CopyCode({ code }: { code: string }) {
  const [copied, setCopied] = useState(false)
  return <button type="button" onClick={() => { void navigator.clipboard?.writeText(code); setCopied(true); window.setTimeout(() => setCopied(false), 1500) }}
    className="mt-5 flex w-full items-center justify-between gap-3 rounded-[10px] border border-line bg-panel px-3 py-2.5 text-left font-mono text-[12px] text-ink-2 transition hover:border-line-strong" aria-label="Copy the command">
    <span className="truncate">{code}</span>{copied ? <Check size={14} className="shrink-0 text-ok" /> : <Copy size={14} className="shrink-0" />}
  </button>
}

export function Connect() {
  return <section className="relative border-t border-line py-[clamp(96px,14vh,160px)]">
    <Container>
      <SectionHead object="/art/props/plug.webp" eyebrow="Plugs into your tools" title={<>Bring your own<br />everything</>} lead="Your coding agents, your terminal, your voice." />
      <div className="mt-16 grid gap-5 lg:grid-cols-3">
        {connect.map((item, index) => { const Icon = icons[index]; return <Reveal key={item.title} delay={index * 0.06}>
          <article className="relative h-full rounded-[18px] border border-line bg-raise p-7">
            <span className="grid size-11 place-items-center rounded-[12px] bg-accent-soft text-accent"><Icon size={20} aria-hidden="true" /></span>
            <h3 className="display mt-8 text-[36px] text-ink">{item.title}</h3>
            <p className="mt-2 text-[15px] leading-relaxed text-ink-3">{item.body}</p>
            {item.code && <CopyCode code={item.code} />}
          </article>
        </Reveal> })}
      </div>
      <Art src="/art/props/notebook.webp" className="pointer-events-none mx-auto mt-10 hidden w-[120px] opacity-90 lg:block" />
    </Container>
  </section>
}
