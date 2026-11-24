// Phase 4: another machine's bots in the sidebar, under that machine's name.
import { useEffect, useRef, useState } from 'react'
import { ChevronDown, FolderPlus, Laptop, Monitor, Server, Settings2 } from 'lucide-react'
import type { Agent } from '../types'
import { showMainWindow } from '../tauri'
import { BlobCanvas } from '../blob/BlobCanvas'
import { agentLook } from '../blob/look'
import { agentColorHex } from './agentColor'
import { canChat, machineKindWord, machineStateWord, type Machine, type MachineBot } from './machines'
import { useEntityNoun } from './vocabulary'
import './machines.css'

/** The look and colour a bot row needs, from what another machine sent. */
const asAgent = (bot: MachineBot) => bot as unknown as Agent

/** The icon for another machine: a server for Linux, a screen for Windows, a laptop otherwise. */
export function MachineKindIcon({ kind, size = 12 }: { kind: string | null | undefined; size?: number }) {
  if (kind === 'linux') return <Server size={size} aria-hidden="true" />
  if (kind === 'windows') return <Monitor size={size} aria-hidden="true" />
  return <Laptop size={size} aria-hidden="true" />
}

export function MachineBadge({ name, kind }: { name: string; kind?: string | null }) {
  const os = machineKindWord(kind ?? null)
  return <span className="machine-badge" title={`Running on ${name}${os ? ` (${os})` : ''}`}><MachineKindIcon kind={kind} size={10} />{name}</span>
}

export function MachineBotRow({ machine, bot, selected, onOpen }: { machine: Machine; bot: MachineBot; selected: boolean; onOpen: () => void }) {
  const offline = machine.state !== 'online'
  return <button type="button" className="machine-bot-row" aria-current={selected ? 'true' : undefined} aria-label={`${bot.name} on ${machine.name}${bot.busy ? ', working' : ''}${canChat(machine) ? '' : ', read only'}`} onClick={onOpen}>
    <span className="machine-bot-avatar"><BlobCanvas decorative color={agentColorHex(bot.color ?? 'mint')} size={24} mini mood={offline ? 'offline' : bot.busy ? 'tool_activity' : 'idle'} look={agentLook(asAgent(bot))} label={bot.name} /><span className="machine-bot-device" aria-hidden="true"><MachineKindIcon kind={machine.kind} size={8} /></span></span>
    <span className="machine-bot-copy">
      <strong>{bot.name}</strong>
      <small>{offline ? machineStateWord(machine) : bot.busy ? 'Working' : bot.conversations ? `${bot.conversations} conversation${bot.conversations === 1 ? '' : 's'}` : bot.role || 'Ready'}</small>
    </span>
  </button>
}

const COLLAPSED_KEY = 'retinue.machines.collapsed'
const readCollapsed = (): string[] => { try { const raw = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? '[]'); return Array.isArray(raw) ? raw.filter((id): id is string => typeof id === 'string') : [] } catch { return [] } }
/** Which other machines the owner folded away in the sidebar; remembered on this computer. */
function useCollapsedMachines() {
  const [collapsed, setCollapsed] = useState<string[]>(readCollapsed)
  const toggle = (id: string) => setCollapsed((current) => {
    const next = current.includes(id) ? current.filter((item) => item !== id) : [...current, id]
    try { localStorage.setItem(COLLAPSED_KEY, JSON.stringify(next)) } catch { /* private mode: it just won't be remembered */ }
    return next
  })
  return { collapsed, toggle }
}

export function MachineGroups({ machines, selected, onOpen, onNewProject, onManage }: { machines: readonly Machine[]; selected: { machineId: string; botId: string } | null; onOpen: (machine: Machine, bot: MachineBot) => void; onNewProject?: (machine: Machine) => void; onManage?: (machine: Machine) => void }) {
  const nouns = useEntityNoun({ plural: true })
  const { collapsed, toggle } = useCollapsedMachines()
  if (!machines.length) return null
  return <div className="machine-groups"><div className="machine-groups-label"><Monitor size={11} aria-hidden="true" />Other devices</div>{machines.map((machine) => { const folded = collapsed.includes(machine.id); return <section key={machine.id} className={`team-group machine-group${machine.state === 'online' ? '' : ' offline'}${folded ? ' folded' : ''}`} aria-label={`${machine.name}, another machine`}>
    <div className="team-group-head static">
      <button type="button" className="machine-group-toggle" title={machine.live ? 'Live: changes arrive as they happen.' : machine.state === 'online' ? 'No live link right now; Retinue checks every 20 seconds.' : `Shown as it was when ${machine.name} was last reached.`} aria-expanded={!folded} aria-label={`${machine.name}, ${folded ? 'show' : 'hide'} its ${nouns}`} onClick={() => toggle(machine.id)}>
        <ChevronDown size={13} className="machine-group-chevron" aria-hidden="true" />
        <i className={`machine-group-state ${machine.state}${machine.state === 'online' && machine.live ? ' live' : ''}`} title={machineStateWord(machine)} aria-hidden="true" />
        <span className="team-group-title"><MachineKindIcon kind={machine.kind} /><span>{machine.name}</span></span>
        {machine.bots.length > 0 && <span className="machine-group-count" aria-hidden="true">{machine.bots.length}</span>}
      </button>
      {machine.state === 'online' && (onNewProject || onManage) && <span className="machine-group-actions">
        {onNewProject && <button type="button" className="queue-icon" aria-label={`New project on ${machine.name}`} title={`New project on ${machine.name}`} onClick={() => onNewProject(machine)}><FolderPlus size={13} /></button>}
        {onManage && <button type="button" className="queue-icon" aria-label={`Manage ${machine.name}`} title={`Manage ${machine.name}`} onClick={() => onManage(machine)}><Settings2 size={13} /></button>}
      </span>}
    </div>
    <div className="machine-group-body" inert={folded || undefined}><div className="machine-group-body-inner">
    {machine.state === 'revoked' && <p className="machine-group-note">{machine.name} removed this link. Pair again in Settings, Machines.</p>}
    {machine.state !== 'revoked' && !machine.bots.length && <p className="machine-group-note">{machine.state === 'online' ? `No ${nouns} there yet.` : `${machine.name} can't be reached.`}</p>}
    {machine.state !== 'revoked' && machine.bots.length > 0 && <div className="team-group-rows">
      {machine.bots.map((bot) => <MachineBotRow key={bot.id} machine={machine} bot={bot} selected={selected?.machineId === machine.id && selected.botId === bot.id} onOpen={() => onOpen(machine, bot)} />)}
    </div>}
    </div></div>
  </section> })}</div>
}

const CHANNEL = 'retinue-open-machine-bot'

/** The companion opens another machine's bot in the full app, the way it opens Changes. */
export async function openMachineBotInMainWindow(machineId: string, botId: string): Promise<void> {
  await showMainWindow()
  const channel = new BroadcastChannel(CHANNEL)
  channel.postMessage({ machineId, botId })
  channel.close()
}

export function useOpenMachineBotRequests(enabled: boolean, onOpen: (machineId: string, botId: string) => void): void {
  const handler = useRef(onOpen)
  handler.current = onOpen
  useEffect(() => {
    if (!enabled || typeof BroadcastChannel === 'undefined') return
    const channel = new BroadcastChannel(CHANNEL)
    channel.onmessage = (event: MessageEvent<{ machineId?: unknown; botId?: unknown }>) => {
      if (typeof event.data?.machineId === 'string' && typeof event.data.botId === 'string') handler.current(event.data.machineId, event.data.botId)
    }
    return () => channel.close()
  }, [enabled])
}
