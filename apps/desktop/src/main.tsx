import './legacyNames'
import React from 'react'
import ReactDOM from 'react-dom/client'
import { App } from './ui/App'
import { DictationBubble } from './ui/DictationBubble'
import { startWindowBackdrop } from './ui/windowBackdrop'
import { mainWindowReady } from './tauri'
import './ui/theme.css'
import './ui/styles.css'
import './ui/approval.css'
import './ui/accent.css'
import './ui/shell.css'
import './ui/density.css'
import './ui/skeleton.css'
import './ui/composer.css'
import './ui/usage.css'
import './ui/team.css'
import './ui/blobPage.css'
import './ui/companion.css'
import './ui/settings.css'
import './ui/polish.css'
// Vite collapses CSS-only dynamic imports inside a conditional expression into
// one preload of the last branch (beta.35 loaded Dot CSS for Frost and Pop).
// Keep all looks in the entry stylesheet; their rules are scoped by data-style.
import './ui/themes/haze.css'

const dictation = new URLSearchParams(window.location.search).has('dictation')
// The native frosted backdrop belongs to the main window only (the helper also skips the companion).
if (!dictation) startWindowBackdrop()

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>{dictation ? <DictationBubble /> : <App />}</React.StrictMode>,
)

// After the first render: let the shell show the main window (it starts
// hidden so its saved size is restored before anyone sees a frame; 8 November
// 2026), then let the shell's own transitions run. index.html adds `booting`,
// which holds them still so the columns do not slide in on launch. A timer,
// not animation frames: a hidden WebView2 may not run animation frames.
const companionWindow = new URLSearchParams(window.location.search).has('companion')
window.setTimeout(() => {
  if (!dictation && !companionWindow) void mainWindowReady()
  window.setTimeout(() => document.documentElement.classList.remove('booting'), 450)
}, 30)
