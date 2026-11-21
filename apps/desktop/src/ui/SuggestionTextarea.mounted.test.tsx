// @vitest-environment happy-dom
import { act, useState } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { DaemonEvent } from '../types'
import { SuggestionTextarea } from './SuggestionTextarea'
import { useSessionSuggestion } from './sessionSuggestions'

const rpc = vi.hoisted(() => vi.fn())
vi.mock('../tauri', () => ({ rpc }))
const eventHandler = { current: null as ((event: DaemonEvent) => void) | null }

;(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const text = 'What should we check next?'
let host: HTMLDivElement
let root: Root

beforeEach(() => {
  rpc.mockResolvedValue(null)
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
})
afterEach(() => {
  act(() => root.unmount())
  host.remove()
  rpc.mockReset()
})

async function mount(enabled = true, initialDraft = '') {
  function Composer() {
    const [draft, setDraft] = useState(initialDraft)
    const { suggestion, dismiss } = useSessionSuggestion('sol', enabled, eventHandler)
    return <SuggestionTextarea value={draft} placeholder="Message Sol" suggestion={suggestion}
      dismissSuggestion={dismiss} onAcceptSuggestion={setDraft} onChange={(event) => setDraft(event.currentTarget.value)} />
  }
  await act(async () => root.render(<Composer />))
  return host.querySelector('textarea')!
}

function emit(type: string, sessionId = 'sol') {
  act(() => {
    eventHandler.current?.({ type, payload: { sessionId, turnId: 'turn-1', text } })
  })
}

function press(input: HTMLTextAreaElement, key: string) {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })
  act(() => { input.dispatchEvent(event) })
  return event.defaultPrevented
}

function inputText(input: HTMLTextAreaElement, value: string) {
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

describe('composer follow-up suggestions', () => {
  it('shows the suggestion as a placeholder with an accessible description', async () => {
    const input = await mount()
    emit('session.suggestion')
    expect(input.placeholder).toBe(text)
    expect(input.value).toBe('')
    expect(input.getAttribute('aria-description')).toContain(text)
  })

  it('loads the open conversation suggestion on mount', async () => {
    rpc.mockResolvedValue({ turnId: 'turn-1', text })
    expect((await mount()).placeholder).toBe(text)
    expect(rpc).toHaveBeenCalledWith('session.suggestion.get', { sessionId: 'sol' })
  })

  it('does not restore a stale reload response after a cleared event', async () => {
    let reply!: (suggestion: { turnId: string; text: string }) => void
    rpc.mockReturnValue(new Promise((resolve) => { reply = resolve }))
    const input = await mount()
    emit('session.suggestion.cleared')
    await act(async () => reply({ turnId: 'turn-1', text }))
    expect(input.placeholder).toBe('Message Sol')
  })

  it.each(['Tab', 'ArrowRight'])('inserts an editable draft with %s at the end', async (key) => {
    const input = await mount()
    emit('session.suggestion')
    expect(press(input, key)).toBe(true)
    expect(input.value).toBe(text)
    expect(input.placeholder).toBe('Message Sol')
    expect(rpc).toHaveBeenCalledWith('session.suggestion.dismiss', { sessionId: 'sol' })
    inputText(input, 'What else should we check?')
    expect(input.value).toBe('What else should we check?')
  })

  it('leaves Right Arrow alone in the middle of an existing draft', async () => {
    const input = await mount(true, 'Existing draft')
    input.setSelectionRange(3, 3)
    emit('session.suggestion')
    expect(press(input, 'ArrowRight')).toBe(false)
    expect(input.value).toBe('Existing draft')
  })

  it('dismisses when typing and stays dismissed when the draft is erased', async () => {
    const input = await mount()
    emit('session.suggestion')
    inputText(input, 'a')
    inputText(input, '')
    expect(rpc).toHaveBeenCalledWith('session.suggestion.dismiss', { sessionId: 'sol' })
    expect(input.placeholder).toBe('Message Sol')
  })

  it('dismisses with Escape', async () => {
    const input = await mount()
    emit('session.suggestion')
    expect(press(input, 'Escape')).toBe(true)
    expect(input.value).toBe('')
    expect(input.placeholder).toBe('Message Sol')
    expect(rpc).toHaveBeenCalledWith('session.suggestion.dismiss', { sessionId: 'sol' })
  })

  it('hides suggestions when the setting is off', async () => {
    const input = await mount(false)
    emit('session.suggestion')
    expect(input.placeholder).toBe('Message Sol')
    expect(press(input, 'Tab')).toBe(false)
    expect(rpc).not.toHaveBeenCalled()
  })

  it('ignores another conversation suggestion', async () => {
    const input = await mount()
    emit('session.suggestion', 'ada')
    expect(input.placeholder).toBe('Message Sol')
  })

  it('removes the suggestion and description on the cleared event', async () => {
    const input = await mount()
    emit('session.suggestion')
    emit('session.suggestion.cleared')
    expect(input.placeholder).toBe('Message Sol')
    expect(input.hasAttribute('aria-description')).toBe(false)
  })
})
