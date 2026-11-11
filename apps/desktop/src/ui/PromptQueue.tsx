// The composer's prompt queue: the rows of waiting messages inside the composer
// card, the card's one status line, and the "Send later" popover. State lives in
// the daemon; this file only renders it.
import { useEffect, useMemo, useRef, useState } from 'react'
import { ArrowUp, Clock, GripVertical, Pencil, Square, X, CircleAlert } from 'lucide-react'
import type { QueuedPrompt } from '../types'
import { moveMenuFocus, usePopoverDismiss } from './ComposerControls'
import { SEND_LATER_MAX_MS, SEND_LATER_MIN_MS, describeDue, queueRowText, sendLaterPresets, toLocalInputValue } from './queueFormat'

const VISIBLE_ROWS = 3

export interface QueueBannerProps {
  items: readonly QueuedPrompt[]
  /** Whether the conversation is mid-turn. */
  sessionBusy: boolean
  /** Whether a plain message can go into the running turn right now. */
  steer?: boolean
  /** Why queued messages wait for the turn to end, when they do. Shown once under the rows. */
  waitNote?: string | null
  /** The employee's name, for the button descriptions. */
  botName?: string
  /** The row whose text is open in the message box. */
  editingId?: string | null
  nowMs: number
  disabled?: boolean
  /** Idle: sends the message. Busy, with no other way offered: moves it to the front. */
  onSendNow: (id: string, confirmResend?: boolean) => void
  /** Puts the message into the running turn and takes it off the queue. */
  onSteer?: (id: string) => void
  /** Stops the running turn, waits for it to end, then sends this message. */
  onStopAndSend?: (id: string) => void
  /** Opens the message in the message box. Absent where there is no box to open it in. */
  onEdit?: (item: QueuedPrompt) => void
  onDelete: (id: string) => void
  onReorder: (ids: string[]) => void
  /** Cancels the pending automatic retry (a queue item of kind `retry`). */
  onCancelRetry?: () => void
}

type SendKind = 'now' | 'steer' | 'stop' | 'next' | 'again'
const SEND_LABEL: Record<SendKind, string> = { now: 'Send now', steer: 'Send now', stop: 'Stop and send', next: 'Send next', again: 'Try again' }

/** Messages waiting to be sent: scheduled and retry items first, then plain ones in order. */
export function QueueBanner({ items, sessionBusy, steer = false, waitNote, botName = 'The employee', editingId, nowMs, disabled, onSendNow, onSteer, onStopAndSend, onEdit, onDelete, onReorder, onCancelRetry }: QueueBannerProps) {
  const [expanded, setExpanded] = useState(false)
  /** A message that may already have reached the bot is sent again only after the owner says so. */
  const [confirming, setConfirming] = useState<string | null>(null)
  const [dragging, setDragging] = useState<string | null>(null)
  const [over, setOver] = useState<string | null>(null)
  const visible = expanded ? items : items.slice(0, VISIBLE_ROWS)
  const hidden = items.length - visible.length
  const plainIds = useMemo(() => items.filter((item) => item.kind === 'user' && item.status === 'queued').map((item) => item.id), [items])
  useEffect(() => { if (confirming && !items.some((item) => item.id === confirming && item.ambiguous)) setConfirming(null) }, [items, confirming])
  if (!items.length) return null

  const moveTo = (id: string, target: number) => {
    const index = plainIds.indexOf(id)
    if (index < 0 || target < 0 || target >= plainIds.length || target === index) return
    const next = plainIds.filter((entry) => entry !== id)
    next.splice(target, 0, id)
    onReorder(next)
  }
  /** What the row's send button does, decided from what the daemon reports. A retry row has none while a turn runs. */
  const sendKind = (item: QueuedPrompt): SendKind | null => {
    if (item.status === 'sending') return null
    if (item.status === 'failed') return 'again'
    if (!sessionBusy) return 'now'
    if (item.kind === 'retry') return null
    if (steer && onSteer && !item.attachments?.length && item.text.trim()) return 'steer'
    return onStopAndSend ? 'stop' : 'next'
  }
  const send = (item: QueuedPrompt, kind: SendKind) => {
    if (item.ambiguous) { setConfirming(item.id); return }
    if (kind === 'steer') onSteer?.(item.id)
    else if (kind === 'stop') onStopAndSend?.(item.id)
    else onSendNow(item.id)
  }
  const sendTitle = (kind: SendKind) => kind === 'steer' ? `${botName} reads this at its next step`
    : kind === 'stop' ? 'Stops this turn, then sends this message. Work already done is kept.'
    : kind === 'next' ? 'Send next, when the current turn ends' : SEND_LABEL[kind]
  const firstSendable = visible.find((item) => sendKind(item))?.id

  return <section className="queue-banner" aria-label={`Queued messages: ${items.length}`}>
    <ol className="queue-list">
      {visible.map((item) => {
        const scheduled = item.kind === 'send_later'
        const retry = item.kind === 'retry'
        const failed = item.status === 'failed'
        const plainIndex = plainIds.indexOf(item.id)
        const movable = plainIndex >= 0 && plainIds.length > 1
        const text = retry ? 'Continue automatically' : queueRowText(item.text, item.attachments?.length ?? 0)
        const kind = sendKind(item)
        return <li key={item.id} className={`queue-item${failed ? ' failed' : ''}${item.status === 'sending' ? ' sending' : ''}${editingId === item.id ? ' editing' : ''}${over === item.id && dragging && dragging !== item.id ? ' drop-target' : ''}`}
          onDragOver={(event) => { if (dragging && plainIndex >= 0) { event.preventDefault(); setOver(item.id) } }}
          onDrop={(event) => { if (!dragging || plainIndex < 0) return; event.preventDefault(); moveTo(dragging, plainIndex); setDragging(null); setOver(null) }}>
          {movable
            ? <button type="button" className="queue-grip" draggable={!disabled} disabled={disabled} aria-label={`Reorder: ${text}`} title="Drag to reorder, or use the arrow keys"
                onDragStart={(event) => { event.dataTransfer.effectAllowed = 'move'; event.dataTransfer.setData('text/plain', item.id); setDragging(item.id) }}
                onDragEnd={() => { setDragging(null); setOver(null) }}
                onKeyDown={(event) => {
                  if (event.key === 'ArrowUp') { event.preventDefault(); moveTo(item.id, plainIndex - 1) }
                  if (event.key === 'ArrowDown') { event.preventDefault(); moveTo(item.id, plainIndex + 1) }
                }}><GripVertical size={13} aria-hidden="true" /></button>
            : <span className="queue-kind" aria-hidden="true">{failed ? <CircleAlert size={13} /> : scheduled || retry ? <Clock size={13} /> : <span className="queue-dot" />}</span>}
          <span className="queue-body">
            <span className="queue-text" title={item.text || text}>{text}</span>
            {confirming === item.id
              ? <span className="queue-confirm" role="alertdialog" aria-label="Send this message again?">
                  <span>It may already have reached the employee. Send it again?</span>
                  <button type="button" className="queue-link" disabled={disabled} onClick={() => { setConfirming(null); onSendNow(item.id, true) }}>Send again</button>
                  <button type="button" className="queue-link" onClick={() => setConfirming(null)}>Keep it here</button>
                </span>
              : failed
              ? <span className="queue-meta danger">{item.ambiguous ? 'May already have been sent' : item.error ?? 'Could not be sent'}</span>
              : editingId === item.id
              ? <span className="queue-meta">Open in the message box</span>
              : (scheduled || retry) && <span className="queue-meta">{retry ? 'retrying ' : ''}{item.whenLimitResets ? 'when the limit resets' : item.lateBySecs && item.lateBySecs > 0 ? 'late' : describeDue(item.dueAt, nowMs)}</span>}
          </span>
          <span className="queue-actions">
            {kind && <button type="button" className={`queue-send${item.id === firstSendable ? ' primary' : ''}`} disabled={disabled || editingId === item.id} title={sendTitle(kind)} onClick={() => send(item, kind)}>
              {kind === 'stop' ? <Square size={11} aria-hidden="true" /> : kind === 'again' ? null : <ArrowUp size={12} aria-hidden="true" />}<span>{SEND_LABEL[kind]}</span>
            </button>}
            {onEdit && !failed && !retry && item.status === 'queued' && item.text && <button type="button" className="queue-icon" disabled={disabled || editingId === item.id} aria-label="Edit queued message" title="Edit in the message box" onClick={() => onEdit(item)}><Pencil size={12} /></button>}
            {item.status !== 'sending' && <button type="button" className="queue-icon" disabled={disabled} aria-label={retry ? 'Cancel automatic retry' : 'Remove from queue'} title={retry ? 'Cancel automatic retry' : 'Remove'} onClick={() => retry && onCancelRetry ? onCancelRetry() : onDelete(item.id)}><X size={13} /></button>}
          </span>
        </li>
      })}
    </ol>
    {(hidden > 0 || expanded) && items.length > VISIBLE_ROWS && <button type="button" className="queue-link queue-more" onClick={() => setExpanded((value) => !value)}>{expanded ? 'Show fewer' : `Show ${hidden} more`}</button>}
    {sessionBusy && waitNote && <p className="queue-note">{waitNote}</p>}
  </section>
}

/** A conversation waiting for its usage limit. The daemon owns the timer; the actions exist only where it offers them. */
export interface LimitLine {
  /** The reset time as the owner reads it ("2:00 AM"), or null when the provider gave none. */
  at: string | null
  /** Whether the employee continues on its own at the reset. */
  resumes: boolean
  snoozed?: boolean
  onResumeAtReset?: (on: boolean) => void
  onSnooze?: (on: boolean) => void
}

/**
 * The composer's own status: one line inside the composer card, then the
 * waiting messages as rows above the box. It renders nothing at all while there
 * is nothing to say (no queue, no usage limit, no pause, retry or notice), so an
 * idle composer has no blank strip above the input.
 */
export function ComposerStatus({ items, paused, sessionBusy, notice, limit, editing, nowMs, disabled, onResume, onCancelRetry, onInterrupt, ...queue }: Omit<QueueBannerProps, 'editingId'> & {
  /** Stop the running turn, wait for it to end, then send what is in the box as a fresh message. Absent when there is nothing to send. */
  onInterrupt?: () => void
  paused: boolean
  /** Why the last message went to the queue instead of reaching the turn. */
  notice?: string | null
  limit?: LimitLine | null
  /** A queued message is open in the message box; sending the box saves it back. */
  editing?: { id: string; onCancel: () => void } | null
  onResume: () => void
}) {
  const failed = items.filter((item) => item.status === 'failed').length
  const retrying = items.some((item) => item.kind === 'retry')
  const interrupt = sessionBusy && onInterrupt && !editing
  const line = !!(limit || editing || interrupt || paused || retrying || notice)
  if (!items.length && !line) return null
  const armed = !!limit && limit.resumes && !limit.snoozed
  return <div className="composer-status" role="group" aria-label="Delivery status">
    {line && <div className="composer-status-line">
      {limit && <span className="composer-status-limit" role="status"><b>Usage limit</b> · {limit.at ? `${armed ? 'resumes' : 'resets'} ${limit.at}` : 'reset time unknown'}{limit.snoozed ? ' · snoozed' : ''}</span>}
      {editing && <span className="composer-status-editing" role="status">Editing a queued message<button type="button" className="queue-link" onClick={editing.onCancel}>Cancel</button></span>}
      {paused && <span className="composer-status-paused" role="status">{failed ? 'Paused: a message could not be sent' : 'Paused until you resume'}<button type="button" className="queue-link" disabled={disabled} onClick={onResume}>Resume</button></span>}
      {retrying && <span className="composer-status-retry" role="status">Retrying automatically{onCancelRetry && <button type="button" className="queue-link" disabled={disabled} onClick={onCancelRetry}>Cancel</button>}</span>}
      {notice && <span className="composer-status-note" title={notice}>{notice}</span>}
      <span className="composer-status-end">
        {limit?.onResumeAtReset && limit.at && <button type="button" className="queue-link" disabled={disabled} aria-label={limit.resumes ? 'Cancel automatic resume' : undefined} onClick={() => limit.onResumeAtReset?.(!limit.resumes)}>{limit.resumes ? 'Cancel' : 'Resume at reset'}</button>}
        {limit?.onSnooze && limit.resumes && limit.at && <button type="button" className="queue-link" disabled={disabled} onClick={() => limit.onSnooze?.(!limit.snoozed)}>{limit.snoozed ? 'Wake now' : 'Snooze'}</button>}
        {interrupt && <button type="button" className="queue-link composer-status-interrupt" disabled={disabled} onClick={onInterrupt} title="Stops what the employee is doing now, then sends this message as a new one. Work already done is kept.">Stop and send instead</button>}
      </span>
    </div>}
    {items.length > 0 && <div className="composer-queue" id="composer-queue-rows">
      <QueueBanner {...queue} items={items} sessionBusy={sessionBusy} editingId={editing?.id ?? null} nowMs={nowMs} disabled={disabled} onCancelRetry={onCancelRetry} />
    </div>}
  </div>
}

/** "Send later": quick presets and a custom time; the daemon owns the timer. */
export function SendLaterControl({ disabled, nowMs, onSchedule }: { disabled: boolean; nowMs: number; onSchedule: (dueAt: string) => void }) {
  const [open, setOpen] = useState(false)
  const [custom, setCustom] = useState('')
  const [problem, setProblem] = useState<string | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const close = (restoreFocus: boolean) => { setOpen(false); setProblem(null); if (restoreFocus) requestAnimationFrame(() => triggerRef.current?.focus()) }
  usePopoverDismiss(open, rootRef, close)
  useEffect(() => { if (open) requestAnimationFrame(() => menuRef.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus()) }, [open])
  const presets = sendLaterPresets(nowMs)
  const min = toLocalInputValue(new Date(nowMs + SEND_LATER_MIN_MS))
  const max = toLocalInputValue(new Date(nowMs + SEND_LATER_MAX_MS))

  const pick = (at: Date) => { close(true); onSchedule(at.toISOString()) }
  const scheduleCustom = () => {
    const at = new Date(custom)
    const delta = at.getTime() - nowMs
    if (!custom || Number.isNaN(at.getTime())) { setProblem('Pick a date and time.'); return }
    if (delta < SEND_LATER_MIN_MS) { setProblem('Pick a time at least a minute from now.'); return }
    if (delta > SEND_LATER_MAX_MS) { setProblem('Pick a time within the next year.'); return }
    pick(at)
  }

  return <div ref={rootRef} className="composer-popover-anchor">
    <button ref={triggerRef} type="button" className="composer-icon-button send-later-trigger" aria-haspopup="menu" aria-expanded={open} aria-label="Send later" title="Send later" disabled={disabled} onClick={() => setOpen((value) => !value)}><Clock size={16} aria-hidden="true" /></button>
    {open && <div ref={menuRef} className="composer-menu send-later-menu align-right" role="menu" aria-label="Send this message later" onKeyDown={(event) => { if (moveMenuFocus(menuRef.current, event.key)) event.preventDefault() }}>
      <div className="composer-menu-heading">Send later</div>
      {presets.map((preset) => <button key={preset.id} type="button" role="menuitem" className="composer-menu-item" onClick={() => pick(preset.at)}>
        <span className="composer-menu-text"><strong>{preset.label}</strong><small>{describeDue(preset.at.toISOString(), nowMs)}</small></span>
      </button>)}
      <div className="send-later-custom">
        <label htmlFor="send-later-time">Pick a time</label>
        <input id="send-later-time" type="datetime-local" value={custom} min={min} max={max} onChange={(event) => { setCustom(event.target.value); setProblem(null) }} />
        {problem && <p className="send-later-problem" role="alert">{problem}</p>}
        <button type="button" className="secondary-button small" onClick={scheduleCustom}>Schedule</button>
      </div>
      <p className="composer-menu-note">Sends only while Retinue is running and the computer is awake.</p>
    </div>}
  </div>
}
