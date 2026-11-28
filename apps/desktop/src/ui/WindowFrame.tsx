// The drawn title bar (owner, 28 November 2026): on Linux the main window has no system frame
// (`decorations: false` in `tauri.linux.conf.json`), so Retinue draws one that matches the
// Windows title bar it has there: the name on the left, minimise, maximise and close on the
// right, drag to move, double-click to maximise. Windows keeps its system title bar and never
// renders this. The preview shows it with `?demo-os=linux`, with buttons that do nothing.
import { useEffect, useState } from 'react'
import { currentWindow } from '../transport/nativeBridge'
import { usePlatform } from '../transport/platform'
import './windowFrame.css'

export function drawsOwnFrame(platform: { os: string; kind: string }): boolean {
  return platform.os === 'linux' && platform.kind !== 'browser'
}

export function WindowFrame() {
  const platform = usePlatform()
  const drawn = drawsOwnFrame(platform)
  const [maximized, setMaximized] = useState(false)

  useEffect(() => {
    if (!drawn) return
    const root = document.documentElement
    root.dataset.frame = 'drawn'
    let unlisten: (() => void) | undefined
    let live = true
    const read = () => { void currentWindow().isMaximized().then((value) => { if (live) setMaximized(value) }).catch(() => undefined) }
    read()
    void currentWindow().onResized(read).then((stop) => { if (live) unlisten = stop; else stop() }).catch(() => undefined)
    return () => { live = false; unlisten?.(); delete root.dataset.frame }
  }, [drawn])

  if (!drawn) return null
  const act = (action: 'minimize' | 'toggleMaximize' | 'close') => () => { void currentWindow()[action]().catch(() => undefined) }
  return <header className="window-frame" data-tauri-drag-region>
    <span className="window-frame-title" data-tauri-drag-region>Retinue</span>
    <div className="window-frame-controls">
      <button type="button" className="window-frame-button" aria-label="Minimise" title="Minimise" onClick={act('minimize')}>
        <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" /></svg>
      </button>
      <button type="button" className="window-frame-button" aria-label={maximized ? 'Restore' : 'Maximise'} title={maximized ? 'Restore' : 'Maximise'} onClick={act('toggleMaximize')}>
        {maximized
          ? <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2.5 2.5V.5h7v7h-2" /><rect x=".5" y="2.5" width="7" height="7" /></svg>
          : <svg viewBox="0 0 10 10" aria-hidden="true"><rect x=".5" y=".5" width="9" height="9" /></svg>}
      </button>
      <button type="button" className="window-frame-button close" aria-label="Close" title="Close" onClick={act('close')}>
        <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M.5.5l9 9M9.5.5l-9 9" /></svg>
      </button>
    </div>
  </header>
}
