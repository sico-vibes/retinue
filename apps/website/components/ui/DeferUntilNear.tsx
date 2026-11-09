'use client'

import { useEffect, useRef, useState, type ReactNode } from 'react'

/**
 * Keeps the server's HTML for a section and doesn't load or hydrate its JavaScript until it is
 * about to scroll into view. Before that the browser shows the static markup, so the page is
 * complete for readers, search engines and anyone without JavaScript.
 */
export function DeferUntilNear({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null)
  const [ready, setReady] = useState(false)

  useEffect(() => {
    const element = ref.current
    if (!element) return
    const observer = new IntersectionObserver((entries) => {
      if (!entries.some((entry) => entry.isIntersecting)) return
      observer.disconnect()
      setReady(true)
    }, { rootMargin: '1000px 0px' })
    observer.observe(element)
    return () => observer.disconnect()
  }, [])

  // The server renders the section. In the browser, an empty dangerouslySetInnerHTML with
  // suppressHydrationWarning tells React to leave that existing markup alone until `ready`.
  if (typeof window === 'undefined' || ready) return <div ref={ref}>{children}</div>
  return <div ref={ref} suppressHydrationWarning dangerouslySetInnerHTML={{ __html: '' }} />
}
