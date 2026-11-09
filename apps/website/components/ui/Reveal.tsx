'use client'

import { useEffect, type CSSProperties, type ReactNode } from 'react'

/**
 * One observer for the whole page: elements with `.reveal` get `.in` as they enter the viewport.
 * Page-level reveals are plain CSS, so a section that only fades in needs no JavaScript of its own.
 */
export function RevealObserver() {
  useEffect(() => {
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue
        entry.target.classList.add('in')
        observer.unobserve(entry.target)
      }
    }, { rootMargin: '0px 0px -8% 0px', threshold: 0.03 })
    const watch = () => document.querySelectorAll('.reveal:not(.in)').forEach((element) => observer.observe(element))
    watch()
    // Sections hydrate as they near the viewport, which adds new elements.
    const added = new MutationObserver(watch)
    added.observe(document.body, { childList: true, subtree: true })
    return () => { observer.disconnect(); added.disconnect() }
  }, [])
  return null
}

/** Soft rise as an element enters the viewport. */
export function Reveal({ children, className = '', delay = 0 }: { children: ReactNode; className?: string; delay?: number }) {
  return <div data-reveal className={`reveal ${className}`} style={delay ? ({ '--delay': `${delay * 1000}ms` } as CSSProperties) : undefined}>{children}</div>
}

/** Text that arrives word by word, like a streaming reply. Opacity only, so it never reflows. */
export function StreamText({ text, active, delay = 0, step = 0.035, className = '' }: { text: string; active: boolean; delay?: number; step?: number; className?: string }) {
  const words = text.split(' ')
  return (
    <span className={className}>
      <span className="sr-only">{text}</span>
      <span aria-hidden="true" data-on={active} className="stream">
        {words.map((word, index) => (
          <span key={`${word}-${index}`} style={{ transitionDelay: active ? `${delay + index * step}s` : '0s' }}>
            {word}{index < words.length - 1 ? ' ' : ''}
          </span>
        ))}
      </span>
    </span>
  )
}
