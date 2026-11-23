import { rpc } from '../tauri'
import { usePlatform } from '../transport/platform'
import { useEffect, useRef, useState, type CSSProperties, type FormEvent, type ReactNode, type RefObject } from 'react'
import { ArchiveRestore, CalendarClock, ChevronsUpDown, Compass, Gauge, LogOut, Pencil, Settings2, Keyboard, Smartphone, Newspaper } from 'lucide-react'
import { startTour } from './tour/startTour'
import { useRemoteStatus } from '../remote/useRemote'
import type { ConnectionState } from '../types'

const NAME_KEY = 'retinue.profile.name'

export function readProfileName() {
  try { return localStorage.getItem(NAME_KEY)?.trim() ?? '' } catch { return '' }
}

function writeProfileName(name: string) {
  try {
    if (name) localStorage.setItem(NAME_KEY, name)
    else localStorage.removeItem(NAME_KEY)
  } catch { /* A blocked Storage API keeps the name for this run only. */ }
  syncProfileName(name)
}

/** Blobs address the user by this name; the daemon builds their instructions. */
export function syncProfileName(name: string) {
  void rpc('settings.set', { key: 'profile.name', value: name.trim() }).catch(() => undefined)
}
export { writeProfileName as saveProfileName }


export function profileInitials(name: string) {
  const words = name.trim().split(/\s+/).filter(Boolean)
  if (words.length === 0) return ''
  const letters = words.length === 1 ? Array.from(words[0]).slice(0, 2) : [Array.from(words[0])[0], Array.from(words[words.length - 1])[0]]
  return letters.join('').toLocaleUpperCase()
}

/**
 * Bottom-left account button. Everything that used to sit in the sidebar foot
 * (usage, settings, quit) lives in its menu, like a chat app's
 * profile menu. The name is local to this device.
 */
export function ProfileMenu({ connection, usageActive, schedulesActive = false, triggerRef, onUsage, onSchedules, onRemote, onBackup, onArchivedBots, onWhatsNew, onSettings, onQuit, onShortcuts, trailing }: {
  connection: ConnectionState
  /** Shown at the row's right, outside the menu button (the update arrow). */
  trailing?: ReactNode
  usageActive: boolean
  schedulesActive?: boolean
  onSchedules?: () => void
  triggerRef?: RefObject<HTMLButtonElement | null>
  onUsage: () => void
  onRemote?: () => void
  onBackup?: () => void
  onArchivedBots?: () => void
  onWhatsNew?: () => void
  onShortcuts?: () => void
  onSettings: () => void
  onQuit: () => void
}) {
  const [name, setName] = useState(readProfileName)
  const { available: remoteAvailable } = useRemoteStatus()
  const [open, setOpen] = useState(false)
  // 22 November 2026 (Zephium-style Haze): the menu stays drawn for the 120 ms it takes to leave. While it
  // leaves it is no longer a menu (no role, hidden from assistive tech), so nothing can act on it.
  const [closing, setClosing] = useState(false)
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(name)
  const platform = usePlatform()
  const wrapRef = useRef<HTMLDivElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const ownTrigger = useRef<HTMLButtonElement>(null)
  const trigger = triggerRef ?? ownTrigger
  const connected = connection === 'connected'

  useEffect(() => {
    if (!open) return
    menuRef.current?.querySelector<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')?.focus()
    const close = (event: PointerEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(event.target as Node)) { setClosing(true); setOpen(false) }
    }
    document.addEventListener('pointerdown', close)
    return () => document.removeEventListener('pointerdown', close)
  }, [open])

  useEffect(() => {
    if (!closing) return
    const timer = window.setTimeout(() => setClosing(false), 140)
    return () => window.clearTimeout(timer)
  }, [closing])
  useEffect(() => {
    if (open) { setClosing(false); return }
  }, [open])
  const closeMenu = (refocus = true) => {
    if (open) setClosing(true)
    setOpen(false)
    if (refocus) window.setTimeout(() => trigger.current?.focus(), 0)
  }
  const run = (action: () => void) => { closeMenu(false); action() }
  const onMenuKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const items = Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])
    const index = items.indexOf(document.activeElement as HTMLButtonElement)
    if (event.key === 'Escape') { event.preventDefault(); closeMenu(); return }
    let next = -1
    if (event.key === 'ArrowDown') next = (index + 1) % items.length
    else if (event.key === 'ArrowUp') next = (index - 1 + items.length) % items.length
    else if (event.key === 'Home') next = 0
    else if (event.key === 'End') next = items.length - 1
    if (next >= 0 && items.length) { event.preventDefault(); items[next]?.focus() }
  }
  const saveName = (event: FormEvent) => {
    event.preventDefault()
    const next = draft.trim().slice(0, 40)
    writeProfileName(next)
    setName(next)
    setEditing(false)
  }
  const dismissEditor = () => {
    setDraft(name)
    setEditing(false)
  }

  const initials = profileInitials(name)
  type Row = { kind: 'item'; key: string; label: string; icon: ReactNode; action: () => void; disabled?: boolean; current?: boolean; danger?: boolean; hint?: string } | { kind: 'separator'; key: string } | { kind: 'head'; key: string }
  const item = (key: string, label: string, icon: ReactNode, action: () => void, extra: Partial<Extract<Row, { kind: 'item' }>> = {}): Row => ({ kind: 'item', key, label, icon, action, ...extra })
  const menuRows: Row[] = [
    { kind: 'head', key: 'head' },
    { kind: 'separator', key: 'sep-head' },
    item('settings', 'Settings', <Settings2 size={15} />, onSettings),
    ...(onShortcuts ? [item('shortcuts', 'Keyboard shortcuts', <Keyboard size={15} />, onShortcuts)] : []),
    { kind: 'separator', key: 'sep-1' },
    item('usage', 'Usage', <Gauge size={15} />, onUsage, { disabled: !connected }),
    ...(onSchedules ? [item('schedules', 'Schedules', <CalendarClock size={15} />, onSchedules, { disabled: !connected, current: schedulesActive })] : []),
    ...(remoteAvailable === true && onRemote ? [item('remote', 'Remote', <Smartphone size={15} />, onRemote)] : []),
    ...(onBackup ? [item('backup', 'Backup', <ArchiveRestore size={15} />, onBackup, { disabled: !connected })] : []),
    ...(onArchivedBots ? [item('archived', 'Archived employees', <ArchiveRestore size={15} />, onArchivedBots, { disabled: !connected })] : []),
    { kind: 'separator', key: 'sep-2' },
    ...(onWhatsNew ? [item('whats-new', 'What’s new', <Newspaper size={15} />, onWhatsNew)] : []),
    item('tour', 'Take the tour', <Compass size={15} />, startTour),
    ...(platform.tray ? [{ kind: 'separator', key: 'sep-3' } as Row, item('quit', 'Quit Retinue', <LogOut size={15} />, onQuit, { danger: true })] : []),
  ]
  const hasName = name.trim().length > 0
  const statusText = hasName ? (connected ? 'On this device' : connection === 'connecting' ? 'Connecting…' : 'Offline') : 'Set your name'

  return <div className="sidebar-foot" ref={wrapRef}>
    {editing && <form className="profile-onboarding" aria-label="Your name" onSubmit={saveName}>
      <strong>{name ? 'Edit your name' : 'Welcome to Retinue'}</strong>
      <p>{name ? 'This name is shown in the sidebar and stays on this device.' : 'What should Retinue call you? It stays on this device.'}</p>
      <input className="text-input" aria-label="Your name" value={draft} maxLength={40} placeholder="Your name" onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => { if (event.key === 'Escape') dismissEditor() }} />
      <div className="profile-onboarding-actions">
        <button type="button" className="ghost-button small" onClick={dismissEditor}>{name ? 'Cancel' : 'Not now'}</button>
        <button type="submit" className="primary-button small" disabled={!draft.trim()}>Save name</button>
      </div>
    </form>}
    <div className="profile-anchor">
    <button
      ref={trigger}
      type="button"
      className={`profile-button ${open ? 'open' : ''}`}
      aria-label={hasName ? 'Account and settings' : 'Set your name'}
      aria-haspopup={hasName ? 'menu' : undefined}
      aria-expanded={hasName ? open : undefined}
      aria-current={usageActive ? 'page' : undefined}
      title={hasName ? name : 'Set your name'}
      onClick={() => { if (!hasName) { setDraft(name); setEditing(true); return } if (open) closeMenu(false); else setOpen(true) }}
    >
      <span className={`profile-avatar ${initials ? '' : 'empty'}`} aria-hidden="true">{initials || '?'}<i className={`presence ${connection}`} /></span>
      <span className="profile-copy"><strong>{name || 'Your profile'}</strong><small>{statusText}</small></span>
      {!trailing && <ChevronsUpDown size={14} aria-hidden="true" />}
    </button>
    {trailing}
    {(open || closing) && <div ref={open ? menuRef : undefined} className={`menu-surface profile-menu ${open ? 'is-open' : 'is-closing'}`} role={open ? 'menu' : undefined} aria-hidden={open ? undefined : true} aria-label={open ? 'Account' : undefined} onKeyDown={open ? onMenuKeyDown : undefined}>
      {menuRows.map((row, index) => {
        // Rows rise nearest-first: the bottom row moves first, the name at the top arrives last.
        const style = { '--step': menuRows.length - 1 - index } as CSSProperties
        if (row.kind === 'separator') return <span key={row.key} className="menu-separator" role="separator" style={style} />
        if (row.kind === 'head') return <button key={row.key} type="button" role="menuitem" className="menu-item profile-menu-head" aria-label={`Edit name (${name})`} style={style} onClick={() => run(() => { setDraft(name); setEditing(true) })}>
          <span className="profile-menu-head-avatar" aria-hidden="true">{initials || '?'}</span><strong title={name}>{name}</strong><Pencil size={13} aria-hidden="true" className="profile-menu-head-edit" />
        </button>
        return <button key={row.key} type="button" role="menuitem" className={`menu-item${row.danger ? ' danger' : ''}`} style={style} disabled={row.disabled} aria-current={row.current ? 'page' : undefined} onClick={() => run(row.action)}>{row.icon}{row.label}{row.hint && <kbd className="menu-hint">{row.hint}</kbd>}</button>
      })}
    </div>}
    </div>
  </div>
}
