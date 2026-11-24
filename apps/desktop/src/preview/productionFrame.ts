// The preview draws the main window the way the installed app is drawn (owner, 24 November 2026: "1:1 UI preview of the
// main app production"). The shell makes a transparent 1360 x 860 window (minimum 1000 x 680) with the operating system's
// title bar and a native acrylic blur behind the page; Haze then paints a translucent tint over that blur instead of an
// opaque composition (haze.css `[data-backdrop]`). A bare browser tab has none of that, so Haze looked different here.
// This adds a desktop, the acrylic over it, the title bar and the minimum size, and reports the backdrop as active so the
// page takes the same code path. `?frame=0` turns it off for the old bare view. Development preview only (demoMenu.ts).
import markWhite from '../assets/brand/retinue-mark-white.png'
import markBlack from '../assets/brand/retinue-mark-black.png'
import { productionFrameWanted } from './frameState'
import './productionFrame.css'

const GLYPHS = {
  min: '<svg viewBox="0 0 10 10" width="10" height="10"><path d="M0 5.5h10" stroke="currentColor" stroke-width="1" fill="none"/></svg>',
  max: '<svg viewBox="0 0 10 10" width="10" height="10"><rect x=".5" y=".5" width="9" height="9" stroke="currentColor" stroke-width="1" fill="none"/></svg>',
  close: '<svg viewBox="0 0 10 10" width="10" height="10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1" fill="none"/></svg>',
}

export function installProductionFrame() {
  if (!productionFrameWanted() || document.documentElement.dataset.pvFrame) return
  const root = document.documentElement
  root.dataset.pvFrame = 'on'

  const desktop = document.createElement('div')
  desktop.className = 'pv-desktop'
  desktop.setAttribute('aria-hidden', 'true')
  desktop.innerHTML = '<div class="pv-wall"></div><div class="pv-acrylic"></div>'
  root.prepend(desktop)

  const caption = document.createElement('div')
  caption.className = 'pv-caption'
  caption.setAttribute('aria-hidden', 'true')
  const icon = Object.assign(document.createElement('img'), { alt: '', draggable: false })
  icon.className = 'pv-caption-icon'
  const syncIcon = () => { icon.src = root.dataset.theme === 'light' ? markBlack : markWhite }
  syncIcon()
  new MutationObserver(syncIcon).observe(root, { attributes: true, attributeFilter: ['data-theme'] })
  const title = Object.assign(document.createElement('span'), { textContent: 'Retinue' })
  title.className = 'pv-caption-title'
  const buttons = document.createElement('span')
  buttons.className = 'pv-caption-buttons'
  buttons.innerHTML = `<i>${GLYPHS.min}</i><i>${GLYPHS.max}</i><i class="close">${GLYPHS.close}</i>`
  caption.append(icon, title, buttons)
  root.append(caption)
}

installProductionFrame()
