// A ```openui block in a reply: drawn with Retinue's component library (./library.tsx).
// Buttons and forms raise `retinue:openui-reply`; the conversation puts that text in the
// composer, so nothing is sent without the owner pressing Send.
import { useState } from 'react'
import { Renderer, type ActionEvent } from '@openuidev/react-lang'
import { retinueOpenUi } from './library'
import './openui.css'

export const OPENUI_REPLY_EVENT = 'retinue:openui-reply'

/** The text a button or form puts in the composer: the label, then each answer on its own line. */
export function replyText(event: Pick<ActionEvent, 'humanFriendlyMessage' | 'formState' | 'params' | 'formName'>): string {
  const lines = [String(event.humanFriendlyMessage ?? '').trim()].filter(Boolean)
  // Form state arrives keyed by form name, each field as { value }: flatten it to field → value.
  const raw = event.formState && typeof event.formState === 'object' ? event.formState as Record<string, unknown> : {}
  const scoped = event.formName && raw[event.formName] && typeof raw[event.formName] === 'object' ? raw[event.formName] as Record<string, unknown> : raw
  const state = Object.fromEntries(Object.entries(scoped).map(([key, value]) => [key, value && typeof value === 'object' && 'value' in (value as object) ? (value as { value: unknown }).value : value]))
  for (const [key, value] of Object.entries(state)) {
    if (key.startsWith('__') || value === undefined || value === null || value === '') continue
    lines.push(`${key}: ${typeof value === 'boolean' ? (value ? 'yes' : 'no') : typeof value === 'object' ? JSON.stringify(value) : String(value)}`)
  }
  return lines.join('\n')
}

export function OpenUiBlock({ code, streaming = false, renderCode }: { code: string; streaming?: boolean; renderCode: () => React.ReactNode }) {
  const [view, setView] = useState<'ui' | 'code'>('ui')
  const [failed, setFailed] = useState(false)
  return <div className="oui-block">
    <div className="rich-switch" role="group" aria-label="View"><button type="button" aria-pressed={view === 'ui'} onClick={() => setView('ui')}>View</button><button type="button" aria-pressed={view === 'code'} onClick={() => setView('code')}>Source</button></div>
    {view === 'code' ? renderCode() : <div className="oui-surface">
      <Renderer response={code} library={retinueOpenUi} isStreaming={streaming}
        onError={(errors) => setFailed(errors.length > 0 && !code.trim())}
        onAction={(event) => { const text = replyText(event); if (text) window.dispatchEvent(new CustomEvent(OPENUI_REPLY_EVENT, { detail: { text } })) }} />
      {failed && <p className="oui-invalid">This view could not be drawn. Open Source to see what was sent.</p>}
    </div>}
  </div>
}
