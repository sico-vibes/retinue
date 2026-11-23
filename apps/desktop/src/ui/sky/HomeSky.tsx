// The sky behind an empty conversation in Haze, as Zephium's new tab paints its sky (owner, 23 November 2026).
// It prints once per size (and again on a theme change), fades up once printed, and steps aside for the owner's own
// chat background picture, as Zephium's sky steps aside for a custom one.
import { useEffect, useRef, useState, useSyncExternalStore, type CSSProperties } from 'react'
import { createPortal } from 'react-dom'
import { printSky, SKY_CELL, skyEdgeBands, type SkyScene } from './skyPrint'
import { useLookStyle } from '../lookStyle'

function readScene(): SkyScene {
  return document.documentElement.dataset.theme === 'light' ? 'day' : 'night'
}

function subscribeScene(listener: () => void) {
  const observer = new MutationObserver(listener)
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
  return () => observer.disconnect()
}

export const useSkyScene = () => useSyncExternalStore(subscribeScene, readScene, () => 'night' as SkyScene)

export function HomeSky() {
  const scene = useSkyScene()
  const host = useRef<HTMLDivElement>(null)
  const canvas = useRef<HTMLCanvasElement>(null)
  const [printed, setPrinted] = useState<string | null>(null)
  const [light, setLight] = useState<{ shell: HTMLElement; style: CSSProperties } | null>(null)

  useEffect(() => {
    const element = host.current
    if (!element) return
    let timer: number | undefined
    let last = ''
    const print = () => {
      const { width, height } = element.getBoundingClientRect()
      // Print a little larger than the pane, in 160px steps, so small resizes keep the same print.
      const w = Math.ceil((width + 1) / 160) * 160, h = Math.ceil((height + 1) / 160) * 160
      const key = `${w}x${h}:${scene}`
      if (width < 2 || height < 2 || !canvas.current) return
      if (key !== last) {
        if (!printSky(canvas.current, w, h, scene)) return
        last = key
        setPrinted(key)
      }
      place(width)
    }
    // The sky lights the columns beside it (Zephium's sky light): each band of the picture's left edge, laid across
    // the columns from the pane's edge, strongest beside the page and gone by the window's far side.
    const place = (width: number) => {
      const shell = element.closest<HTMLElement>('.app-shell')
      if (!shell || !canvas.current) { setLight(null); return }
      const sky = element.getBoundingClientRect(), frame = shell.getBoundingClientRect()
      const offset = (canvas.current.width * SKY_CELL - width) / 2
      const bands = skyEdgeBands(canvas.current, offset / SKY_CELL)
      const stops = bands.map(([r, g, b], band) => `rgb(${r} ${g} ${b}) ${(((band + 0.5) * sky.height) / bands.length).toFixed(1)}px`)
      setLight({ shell, style: { left: 0, top: sky.top - frame.top, width: Math.max(0, sky.left - frame.left), height: sky.height, backgroundImage: `linear-gradient(${stops.join(', ')})` } })
    }
    // The first print waits a frame, so the pane is on screen (as its paper) before the sky is worked out.
    const first = window.requestAnimationFrame(() => { timer = window.setTimeout(print, 0) })
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => { window.clearTimeout(timer); timer = window.setTimeout(print, 140) })
    observer?.observe(element)
    return () => { observer?.disconnect(); window.cancelAnimationFrame(first); window.clearTimeout(timer) }
  }, [scene])

  return <div ref={host} className="home-sky" data-sky={scene} data-ready={printed ? 'true' : undefined} aria-hidden="true">
    <canvas ref={canvas} className="home-sky-print" />
    {light && createPortal(<div className="sky-light" data-sky={scene} data-ready={printed ? 'true' : undefined} style={light.style} />, light.shell)}
  </div>
}

/** The time and the day, as Zephium's new tab shows them under its name. */
export function SkyClock() {
  const [now, setNow] = useState(() => new Date())
  useEffect(() => {
    const tick = () => setNow(new Date())
    const timer = window.setInterval(tick, 15_000)
    return () => window.clearInterval(timer)
  }, [])
  const time = now.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
  const [clock, period] = time.split(/\s(?=[AP]M$)/i)
  const day = now.toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' })
  return <p className="sky-clock"><time dateTime={now.toISOString()}><b>{clock}</b>{period && <small> {period}</small>}</time><span aria-hidden="true"> · </span>{day}</p>
}

/** The clock in an empty conversation, in Haze only. */
export function HazeSkyClock() {
  return useLookStyle() === 'haze' ? <SkyClock /> : null
}
