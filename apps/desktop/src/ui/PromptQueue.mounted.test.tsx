// @vitest-environment happy-dom
import { act, type ComponentProps, type ReactNode } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { QueuedPrompt } from '../types'
import { ComposerStatus, QueueBanner, SendLaterControl } from './PromptQueue'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const NOW = new Date(2026, 9, 12, 10, 0, 0).getTime()
const item = (partial: Partial<QueuedPrompt> & Pick<QueuedPrompt, 'id' | 'text'>): QueuedPrompt => ({
  sessionId: 's1', kind: 'user', dueAt: null, status: 'queued', error: null, position: 0, ...partial,
})

let root: Root
let host: HTMLDivElement
beforeEach(() => { host = document.createElement('div'); document.body.appendChild(host); root = createRoot(host) })
afterEach(() => { act(() => root.unmount()); host.remove() })
const render = (node: ReactNode) => act(() => { root.render(node) })
const q = <T extends Element>(selector: string) => host.querySelector<T>(selector)
const click = (element: Element | null | undefined) => act(() => { (element as HTMLElement).click() })
const key = (element: Element | null | undefined, name: string) => act(() => { element!.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true })) })
const sendButtons = () => [...host.querySelectorAll<HTMLButtonElement>('.queue-send')]

function banner(items: QueuedPrompt[], extra: Partial<ComponentProps<typeof QueueBanner>> = {}) {
  const handlers = { onSendNow: vi.fn(), onSteer: vi.fn(), onStopAndSend: vi.fn(), onEdit: vi.fn(), onDelete: vi.fn(), onReorder: vi.fn() }
  render(<QueueBanner items={items} sessionBusy nowMs={NOW} {...handlers} {...extra} />)
  return handlers
}

describe('QueueBanner', () => {
  it('renders nothing for an empty queue', () => {
    banner([])
    expect(q('.queue-banner')).toBeNull()
  })

  it('lists messages with scheduled times, with remove always there and no count heading', () => {
    const items = [
      item({ id: 'a', text: 'first', position: 0 }),
      item({ id: 'b', text: 'second', position: 1 }),
      item({ id: 'c', text: 'later', kind: 'send_later', dueAt: new Date(NOW + 14 * 60_000).toISOString(), position: 2 }),
    ]
    const handlers = banner(items)
    expect(q('.queue-banner')?.getAttribute('aria-label')).toBe('Queued messages: 3')
    expect(q('.queue-head')).toBeNull()
    expect([...host.querySelectorAll('.queue-text')].map((node) => node.textContent)).toEqual(['first', 'second', 'later'])
    expect(q('.queue-meta')?.textContent).toBe('in 14 min')
    const rows = [...host.querySelectorAll('li')]
    click(rows[1].querySelector('[aria-label="Remove from queue"]'))
    expect(handlers.onDelete).toHaveBeenCalledWith('b')
    // Scheduled items are not part of the manual order.
    expect(rows[2].querySelector('.queue-grip')).toBeNull()
  })

  it('reorders plain messages from the handle with the arrow keys', () => {
    const handlers = banner([item({ id: 'a', text: 'first' }), item({ id: 'b', text: 'second' }), item({ id: 'c', text: 'third' })])
    const grips = [...host.querySelectorAll('.queue-grip')]
    expect(grips).toHaveLength(3)
    expect(grips[1].getAttribute('aria-label')).toBe('Reorder: second')
    key(grips[1], 'ArrowUp')
    expect(handlers.onReorder).toHaveBeenLastCalledWith(['b', 'a', 'c'])
    key(grips[1], 'ArrowDown')
    expect(handlers.onReorder).toHaveBeenLastCalledWith(['a', 'c', 'b'])
    key(grips[0], 'ArrowUp')
    expect(handlers.onReorder).toHaveBeenCalledTimes(2)
  })

  it('has no handle for a single message', () => {
    banner([item({ id: 'a', text: 'only' })])
    expect(q('.queue-grip')).toBeNull()
  })

  it('says "Send now" and steers into the running turn when the turn can take a message', () => {
    const handlers = banner([item({ id: 'a', text: 'first' }), item({ id: 'b', text: 'second' })], { steer: true, botName: 'Vex' })
    const buttons = sendButtons()
    expect(buttons.map((node) => node.textContent)).toEqual(['Send now', 'Send now'])
    expect(buttons[0].title).toBe('Vex reads this at its next step')
    // One obvious action: only the first row's button is filled.
    expect(buttons.map((node) => node.classList.contains('primary'))).toEqual([true, false])
    click(buttons[1])
    expect(handlers.onSteer).toHaveBeenCalledWith('b')
    expect(handlers.onSendNow).not.toHaveBeenCalled()
  })

  it('says "Stop and send" when the turn cannot take the message, and why', () => {
    const handlers = banner([item({ id: 'a', text: 'first' })], { steer: false, waitNote: 'Steering is unavailable for this coding agent. Your message waits for the turn to end.' })
    expect(sendButtons()[0].textContent).toBe('Stop and send')
    expect(q('.queue-note')?.textContent).toContain('Steering is unavailable')
    click(sendButtons()[0])
    expect(handlers.onStopAndSend).toHaveBeenCalledWith('a')
    expect(handlers.onSteer).not.toHaveBeenCalled()
  })

  it('never steers a message that carries attachments', () => {
    const handlers = banner([item({ id: 'a', text: 'see this', attachments: [{ name: 'a.png', path: 'C:/a.png' }] })], { steer: true })
    expect(sendButtons()[0].textContent).toBe('Stop and send')
    click(sendButtons()[0])
    expect(handlers.onStopAndSend).toHaveBeenCalledWith('a')
  })

  it('keeps "Send next" where stopping is not offered', () => {
    const handlers = banner([item({ id: 'a', text: 'first' })], { onStopAndSend: undefined })
    expect(sendButtons()[0].textContent).toBe('Send next')
    click(sendButtons()[0])
    expect(handlers.onSendNow).toHaveBeenCalledWith('a')
  })

  it('says "Send now" and sends when the conversation is idle, with no waiting note', () => {
    const handlers = banner([item({ id: 'a', text: 'hello' })], { sessionBusy: false, steer: true, waitNote: 'Should not show.' })
    expect(sendButtons()[0].textContent).toBe('Send now')
    expect(q('.queue-note')).toBeNull()
    click(sendButtons()[0])
    expect(handlers.onSendNow).toHaveBeenCalledWith('a')
    expect(handlers.onSteer).not.toHaveBeenCalled()
  })

  it('shows a failed row with its reason and "Try again"', () => {
    const handlers = banner([
      item({ id: 'a', text: 'broken', status: 'failed', error: 'Conversation is archived.' }),
      item({ id: 'b', text: 'waiting' }),
    ], { sessionBusy: false })
    expect(q('.queue-item.failed .queue-meta')?.textContent).toBe('Conversation is archived.')
    expect(q('.queue-item.failed .queue-send')?.textContent).toBe('Try again')
    expect(q('.queue-item.failed [aria-label="Edit queued message"]')).toBeNull()
    click(q('.queue-item.failed .queue-send'))
    expect(handlers.onSendNow).toHaveBeenCalledWith('a')
  })

  it('asks before sending again a message that may already have arrived', () => {
    const handlers = banner([item({ id: 'a', text: 'maybe', status: 'failed', ambiguous: true })], { sessionBusy: false })
    expect(q('.queue-meta')?.textContent).toBe('May already have been sent')
    click(sendButtons()[0])
    expect(handlers.onSendNow).not.toHaveBeenCalled()
    click([...host.querySelectorAll('.queue-link')].find((node) => node.textContent === 'Send again'))
    expect(handlers.onSendNow).toHaveBeenCalledWith('a', true)
  })

  it('opens a queued message in the message box to edit it, and marks the open row', () => {
    const first = item({ id: 'a', text: 'draft one' })
    const handlers = banner([first, item({ id: 'b', text: 'other' })])
    expect(q('input.queue-edit')).toBeNull()
    click(q('[aria-label="Edit queued message"]'))
    expect(handlers.onEdit).toHaveBeenCalledWith(first)
    banner([first, item({ id: 'b', text: 'other' })], { editingId: 'a' })
    expect(q('.queue-item.editing .queue-meta')?.textContent).toBe('Open in the message box')
    expect(q<HTMLButtonElement>('.queue-item.editing .queue-send')?.disabled).toBe(true)
  })

  it('shows an automatic retry as a timed row that cancels through its own action', () => {
    const onCancelRetry = vi.fn()
    const onDelete = vi.fn()
    banner([item({ id: 'r1', text: 'Please continue.', kind: 'retry', dueAt: new Date(NOW + 5 * 60_000).toISOString() })], { onCancelRetry, onDelete })
    expect(q('.queue-text')?.textContent).toBe('Continue automatically')
    expect(q('.queue-meta')?.textContent).toBe('retrying in 5 min')
    expect(q('[aria-label="Edit queued message"]')).toBeNull()
    expect(q('.queue-grip')).toBeNull()
    expect(q('.queue-send')).toBeNull()
    click(q('[aria-label="Cancel automatic retry"]'))
    expect(onCancelRetry).toHaveBeenCalledOnce()
    expect(onDelete).not.toHaveBeenCalled()
  })

  it('shows three rows and expands the rest', () => {
    banner(Array.from({ length: 5 }, (_, index) => item({ id: `m${index}`, text: `msg ${index}`, position: index })))
    expect(host.querySelectorAll('.queue-item')).toHaveLength(3)
    expect(q('.queue-more')?.textContent).toBe('Show 2 more')
    click(q('.queue-more'))
    expect(host.querySelectorAll('.queue-item')).toHaveLength(5)
  })
})

describe('SendLaterControl', () => {
  it('offers presets that return an ISO time and validates a custom time', () => {
    const onSchedule = vi.fn()
    render(<SendLaterControl disabled={false} nowMs={NOW} onSchedule={onSchedule} />)
    click(q('[aria-label="Send later"]'))
    expect(q('.send-later-menu')?.textContent).toContain('Sends only while Retinue is running')
    const labels = [...host.querySelectorAll('[role="menuitem"] strong')].map((node) => node.textContent)
    expect(labels).toEqual(['In 15 minutes', 'In 1 hour', 'In 3 hours', 'Tomorrow morning'])
    click(host.querySelectorAll('[role="menuitem"]')[1])
    expect(onSchedule).toHaveBeenCalledWith(new Date(NOW + 60 * 60_000).toISOString())
    expect(q('.send-later-menu')).toBeNull()

    click(q('[aria-label="Send later"]'))
    click(q('.send-later-custom button'))
    expect(q('[role="alert"]')?.textContent).toBe('Pick a date and time.')
    expect(onSchedule).toHaveBeenCalledTimes(1)
  })

  it('is disabled when there is nothing to send', () => {
    render(<SendLaterControl disabled nowMs={NOW} onSchedule={vi.fn()} />)
    expect(q<HTMLButtonElement>('[aria-label="Send later"]')?.disabled).toBe(true)
  })
})

describe('ComposerStatus', () => {
  const status = (props: Partial<ComponentProps<typeof ComposerStatus>> = {}) => {
    const handlers = { onResume: vi.fn(), onSendNow: vi.fn(), onDelete: vi.fn(), onReorder: vi.fn() }
    render(<ComposerStatus items={[]} paused={false} sessionBusy nowMs={NOW} {...handlers} {...props} />)
    return handlers
  }
  const endButtons = () => [...host.querySelectorAll<HTMLButtonElement>('.composer-status-end button')]

  it('shows nothing while a turn runs with nothing queued: how a message travels is a setting', () => {
    status()
    expect(q('.composer-status')).toBeNull()
    status({ onInterrupt: vi.fn() })
    expect(q('.composer-status-interrupt')?.textContent).toBe('Stop and send instead')
  })

  it('shows the waiting messages as rows at once, with no chip to open and no status line of its own', () => {
    status({ items: [item({ id: 'a', text: 'first' }), item({ id: 'b', text: 'second' })] })
    expect(q('.composer-status-chip')).toBeNull()
    expect(q('.composer-status-line')).toBeNull()
    expect(q('.queue-banner')?.getAttribute('aria-label')).toBe('Queued messages: 2')
    expect(host.querySelectorAll('.queue-item')).toHaveLength(2)
  })

  it('says when the queue is paused, when a message failed and when a retry is waiting', () => {
    const failed = [item({ id: 'a', text: 'first', status: 'failed', error: 'The provider refused.' })]
    const handlers = status({ items: failed, paused: true })
    expect(q('.composer-status-paused')?.textContent).toContain('Paused: a message could not be sent')
    click(q('.composer-status-paused button'))
    expect(handlers.onResume).toHaveBeenCalled()
    status({ items: [item({ id: 'r', text: 'Please continue.', kind: 'retry' })], onCancelRetry: vi.fn() })
    expect(q('.composer-status-retry')?.textContent).toContain('Retrying automatically')
    expect(q('.composer-status-retry button')?.textContent).toBe('Cancel')
    status({ paused: true })
    expect(q('.composer-status-paused')?.textContent).toContain('Paused until you resume')
  })

  it('shows the reason a message went to the queue', () => {
    status({ notice: 'Queued because this turn could not accept steering. It sends after the turn ends.' })
    expect(q('.composer-status-note')?.textContent).toContain('could not accept steering')
    expect(q('.composer-status-note')?.getAttribute('title')).toContain('could not accept steering')
  })

  it('names a usage limit with "resumes" only when the employee continues on its own', () => {
    status({ sessionBusy: false, limit: { at: '2:00 PM', resumes: true } })
    expect(q('.composer-status-limit')?.textContent).toBe('Usage limit · resumes 2:00 PM')
    status({ sessionBusy: false, limit: { at: '2:00 PM', resumes: false } })
    expect(q('.composer-status-limit')?.textContent).toBe('Usage limit · resets 2:00 PM')
    // No actions unless the daemon offers them for this conversation.
    expect(endButtons()).toHaveLength(0)
  })

  it('never guesses a reset time', () => {
    status({ sessionBusy: false, limit: { at: null, resumes: true, onResumeAtReset: vi.fn(), onSnooze: vi.fn() } })
    expect(q('.composer-status-limit')?.textContent).toBe('Usage limit · reset time unknown')
    expect(endButtons()).toHaveLength(0)
  })

  it('offers cancel, resume at reset, snooze and wake where the daemon keeps them per conversation', () => {
    const onResumeAtReset = vi.fn()
    const onSnooze = vi.fn()
    status({ sessionBusy: false, limit: { at: '2:00 PM', resumes: true, onResumeAtReset, onSnooze } })
    expect(endButtons().map((node) => node.textContent)).toEqual(['Cancel', 'Snooze'])
    expect(endButtons()[0].getAttribute('aria-label')).toBe('Cancel automatic resume')
    click(endButtons()[0])
    expect(onResumeAtReset).toHaveBeenLastCalledWith(false)
    click(endButtons()[1])
    expect(onSnooze).toHaveBeenLastCalledWith(true)
    status({ sessionBusy: false, limit: { at: '2:00 PM', resumes: true, snoozed: true, onResumeAtReset, onSnooze } })
    expect(q('.composer-status-limit')?.textContent).toBe('Usage limit · resets 2:00 PM · snoozed')
    expect(endButtons().map((node) => node.textContent)).toEqual(['Cancel', 'Wake now'])
    click(endButtons()[1])
    expect(onSnooze).toHaveBeenLastCalledWith(false)
    status({ sessionBusy: false, limit: { at: '2:00 PM', resumes: false, onResumeAtReset, onSnooze } })
    expect(endButtons().map((node) => node.textContent)).toEqual(['Resume at reset'])
    click(endButtons()[0])
    expect(onResumeAtReset).toHaveBeenLastCalledWith(true)
  })

  it('says a queued message is open in the box, and cancels from there', () => {
    const onCancel = vi.fn()
    status({ items: [item({ id: 'a', text: 'first' })], editing: { id: 'a', onCancel }, onInterrupt: vi.fn() })
    expect(q('.composer-status-editing')?.textContent).toContain('Editing a queued message')
    // "Stop and send instead" would send the box as a new message, so it steps aside while editing.
    expect(q('.composer-status-interrupt')).toBeNull()
    expect(q('.queue-item.editing')).not.toBeNull()
    click(q('.composer-status-editing button'))
    expect(onCancel).toHaveBeenCalledOnce()
  })
})
