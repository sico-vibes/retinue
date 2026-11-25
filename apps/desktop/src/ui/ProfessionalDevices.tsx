// Other devices in Professional mode (owner, 25 November 2026): where a chat lives is shown by an icon (this device, a remote PC,
// a VPS, a phone), a device can be renamed, given a colour and dragged into the order wanted. Renames go to the daemon (the
// same call as the Machines page); the colour and the order belong to this window and live in its storage.
import { useCallback, useEffect, useRef, useState, type CSSProperties } from 'react'
import { House, Monitor, Server, Smartphone } from 'lucide-react'
import { rpc } from '../tauri'
import { usePopoverDismiss } from './ComposerControls'
import { PROJECT_ICON_COLOURS } from './ProjectIcon'
import { normalizeMachineSessions, useMachineSessionChanges, type Machine, type MachineSessionRow } from './machines'

export type WhereKind = 'local' | 'pc' | 'vps' | 'phone'
export type Where = { kind: WhereKind; name: string; color?: string | null }
export const LOCAL_WHERE: Where = { kind: 'local', name: 'This computer' }

/** What a far machine is, for its icon: a Linux box is a VPS, a phone is a phone, anything else a PC. */
export function whereKindOf(machine: Pick<Machine, 'kind'>): WhereKind {
  const kind = String(machine.kind ?? '').toLowerCase()
  if (kind === 'linux') return 'vps'
  if (kind === 'ios' || kind === 'android' || kind === 'phone') return 'phone'
  return 'pc'
}
const WORD: Record<WhereKind, string> = { local: 'This computer', pc: 'Remote PC', vps: 'VPS', phone: 'Phone' }

/** One glyph, one box: every place a device or a chat location is drawn uses this so the size and position never differ. */
export function WhereIcon({ where, size = 14 }: { where: Where; size?: number }) {
  const Icon = where.kind === 'local' ? House : where.kind === 'vps' ? Server : where.kind === 'phone' ? Smartphone : Monitor
  const style: CSSProperties = { width: size + 4, height: size + 4, color: where.color ?? undefined }
  return <span className="pro-where" style={style} role="img" aria-label={`${WORD[where.kind]}: ${where.name}`} title={`${where.name} · ${WORD[where.kind]}`}><Icon size={size} aria-hidden="true" /></span>
}

type Prefs = { order: string[]; colors: Record<string, string> }
const PREFS_KEY = 'retinue.pro.devices'
const readPrefs = (): Prefs => { try { const raw = JSON.parse(window.localStorage.getItem(PREFS_KEY) ?? '{}'); return { order: Array.isArray(raw.order) ? raw.order.filter((id: unknown): id is string => typeof id === 'string') : [], colors: raw.colors && typeof raw.colors === 'object' ? raw.colors : {} } } catch { return { order: [], colors: {} } } }

/** The owner's order and colours for devices; the machines themselves come from the daemon. */
export function useDevicePrefs(machines: readonly Machine[]) {
  const [prefs, setPrefs] = useState(readPrefs)
  const store = (next: Prefs) => { setPrefs(next); try { window.localStorage.setItem(PREFS_KEY, JSON.stringify(next)) } catch { /* kept until the window closes */ } }
  const ordered = [...machines].sort((a, b) => {
    const left = prefs.order.indexOf(a.id), right = prefs.order.indexOf(b.id)
    return (left < 0 ? 1e6 : left) - (right < 0 ? 1e6 : right)
  })
  return {
    ordered,
    colorOf: (id: string) => prefs.colors[id] ?? null,
    setColor: (id: string, color: string | null) => { const colors = { ...prefs.colors }; if (color) colors[id] = color; else delete colors[id]; store({ ...prefs, colors }) },
    move: (id: string, beforeId: string) => {
      if (id === beforeId) return
      const ids = ordered.map((machine) => machine.id).filter((item) => item !== id)
      ids.splice(Math.max(0, ids.indexOf(beforeId)), 0, id)
      store({ ...prefs, order: ids })
    },
  }
}

/** Every device's conversations, read from the daemon and refreshed as they change. */
export function useDeviceRows(machines: readonly Machine[]): Record<string, MachineSessionRow[]> {
  const [rows, setRows] = useState<Record<string, MachineSessionRow[]>>({})
  const ids = machines.map((machine) => machine.id).join('|')
  const refresh = useCallback(() => {
    for (const id of ids ? ids.split('|') : []) void rpc('machine.sessions', { machineId: id }).then(normalizeMachineSessions).then((list) => setRows((current) => ({ ...current, [id]: list.filter((row) => !row.archived) }))).catch(() => undefined)
  }, [ids])
  useEffect(() => { refresh(); const timer = window.setInterval(refresh, 20_000); return () => window.clearInterval(timer) }, [refresh])
  useMachineSessionChanges(machines.length > 0, refresh)
  return rows
}

/** Rename and colour, in a small popover under the device's name. The name is saved on the device's own record. */
export function DeviceMenu({ machine, color, position, onColor, onRenamed, onClose }: { machine: Machine; color: string | null; position: { x: number; y: number }; onColor: (color: string | null) => void; onRenamed: () => void; onClose: () => void }) {
  const [name, setName] = useState(machine.name)
  const [problem, setProblem] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  usePopoverDismiss(true, rootRef, onClose)
  useEffect(() => { inputRef.current?.focus({ focusVisible: false } as FocusOptions); inputRef.current?.select() }, [])
  const save = async () => {
    const next = name.trim()
    if (!next || next === machine.name) { onClose(); return }
    setSaving(true); setProblem(null)
    try { await rpc('machine.rename', { id: machine.id, name: next }); onRenamed(); onClose() } catch (reason) { setProblem(reason instanceof Error ? reason.message : 'That name could not be saved.'); setSaving(false) }
  }
  return <div ref={rootRef} className="menu-surface pro-device-menu" role="dialog" aria-label={`${machine.name} settings`} style={{ left: Math.max(8, Math.min(position.x, window.innerWidth - 260)), top: Math.max(8, Math.min(position.y, window.innerHeight - 190)) }}>
    <label className="pro-device-field"><span>Name</span><input ref={inputRef} value={name} maxLength={60} spellCheck={false} disabled={saving} onChange={(event) => setName(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); void save() } }} /></label>
    <div className="pro-device-field"><span>Icon colour</span>
      <div className="pro-device-colours" role="radiogroup" aria-label="Icon colour">{PROJECT_ICON_COLOURS.map((item) => <button key={item.label} type="button" role="radio" aria-checked={color === item.value} aria-label={item.label} title={item.label} className={color === item.value ? 'on' : undefined} style={{ background: item.swatch }} onClick={() => onColor(item.value)} />)}</div>
    </div>
    {problem && <p className="pro-device-problem" role="alert">{problem}</p>}
    <div className="pro-device-actions"><button type="button" className="ghost-button small" onClick={onClose}>Cancel</button><button type="button" className="primary-button small" disabled={saving || !name.trim()} onClick={() => void save()}>{saving ? 'Saving…' : 'Save name'}</button></div>
  </div>
}
