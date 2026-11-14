'use client'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import Lenis from 'lenis'
import { gsap } from 'gsap'
import { ScrollTrigger } from 'gsap/ScrollTrigger'

/** Smooth scrolling for the whole page, and smooth jumps to #sections. Off for reduced motion. */
export function SmoothScroll() {
  useEffect(() => {
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return
    const lenis = new Lenis({ duration: 1.1, smoothWheel: true })
    // The React Bits reveals run on GSAP ScrollTrigger: drive Lenis from GSAP's ticker and tell
    // ScrollTrigger every time Lenis moves the page, so reveals fire where the page really is.
    gsap.registerPlugin(ScrollTrigger)
    lenis.on('scroll', ScrollTrigger.update)
    const tick = (seconds: number) => lenis.raf(seconds * 1000)
    gsap.ticker.add(tick)
    gsap.ticker.lagSmoothing(0)
    const jump = (event: MouseEvent) => {
      const link = (event.target as HTMLElement).closest('a[href^="#"]') as HTMLAnchorElement | null
      if (!link || link.getAttribute('href') === '#') return
      const target = document.querySelector(link.getAttribute('href')!)
      if (!target) return
      event.preventDefault()
      lenis.scrollTo(target as HTMLElement, { offset: -72 })
    }
    document.addEventListener('click', jump)
    return () => { gsap.ticker.remove(tick); document.removeEventListener('click', jump); lenis.destroy() }
  }, [])
  return null
}

/**
 * Mounts its children only while they are near the screen. Used for WebGL backgrounds: a page can
 * hold only a few WebGL contexts, so each one exists only while it can be seen.
 */
export function WhenVisible({ children, margin = '200px', className = '' }: { children: ReactNode; margin?: string; className?: string }) {
  const ref = useRef<HTMLDivElement>(null)
  const [shown, setShown] = useState(false)
  useEffect(() => {
    const element = ref.current
    if (!element) return
    const observer = new IntersectionObserver(([entry]) => setShown(entry.isIntersecting), { rootMargin: margin })
    observer.observe(element)
    return () => observer.disconnect()
  }, [margin])
  return <div ref={ref} className={className}>{shown ? children : null}</div>
}
