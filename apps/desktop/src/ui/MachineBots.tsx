// Phase 4: another machine's bots in the sidebar, under that machine's name.
import { useEffect, useRef } from 'react'
import { FolderPlus, Laptop, Monitor, Server, Settings2 } from 'lucide-react'
import type { Agent } from '../types'
import { showMainWindow } from '../tauri'
import { BlobCanvas } from '../blob/BlobCanvas'
import { agentLook } from '../blob/look'
import { agentColorHex } from './agentColor'
import { canChat, machineFreshness, machineKindWord, machineStateWord, type Machine, type MachineBot } from './machines'
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
    <BlobCanvas decorative color={agentColorHex(bot.color ?? 'mint')} size={20} mini mood={offline ? 'offline' : bot.busy ? 'tool_activity' : 'idle'} look={agentLook(asAgent(bot))} label={bot.name} />
    <span className="machine-bot-copy">
      <strong>{bot.name}</strong>
      <small>{offline ? machineStateWord(machine) : bot.busy ? 'Working' : bot.conversations ? `${bot.conversations} conversation${bot.conversations === 1 ? '' : 's'}` : bot.role || 'Ready'}</small>
    </span>
  </button>
}

export function MachineGroups({ machines, selected, onOpen, onNewProject, onManage }: { machines: readonly Machine[]; selected: { machineId: string; botId: string } | null; onOpen: (machine: Machine, bot: MachineBot) => void; onNewProject?: (machine: Machine) => void; onManage?: (machine: Machine) => void }) {
  const nouns = useEntityNoun({ plural: true })
  return <>{machines.map((machine) => <section key={machine.id} className={`team-group machine-group${machine.state === 'online' ? '' : ' offline'}`} aria-label={`${machine.name}, another machine`}>
    <div className="team-group-head static">
      <i className={`machine-group-state ${machine.state}${machine.state === 'online' && machine.live ? ' live' : ''}`} title={machineStateWord(machine)} aria-hidden="true" />
      <span className="team-group-title"><MachineKindIcon kind={machine.kind} /><span>{machine.name}</span></span>
      <small className="machine-group-fresh" title={machine.live ? 'Changes arrive as they happen.' : machine.state === 'online' ? 'No live link right now; Retinue checks every 20 seconds.' : `Shown as it was when ${machine.name} was last reached.`}>{machineFreshness(machine)}</small>
      {machine.state === 'online' && (onNewProject || onManage) && <span className="machine-group-actions">
        {onNewProject && <button type="button" className="queue-icon" aria-label={`New project on ${machine.name}`} title={`New project on ${machine.name}`} onClick={() => onNewProject(machine)}><FolderPlus size={13} /></button>}
        {onManage && <button type="button" className="queue-icon" aria-label={`Manage ${machine.name}`} title={`Manage ${machine.name}`} onClick={() => onManage(machine)}><Settings2 size={13} /></button>}
      </span>}
    </div>
    {machine.state === 'revoked' && <p className="machine-group-note">{machine.name} removed this link. Pair again in Settings, Machines.</p>}
    {machine.state !== 'revoked' && !machine.bots.length && <p className="machine-group-note">{machine.state === 'online' ? `No ${nouns} there yet.` : `${machine.name} can't be reached.`}</p>}
    {machine.state !== 'revoked' && machine.bots.length > 0 && <div className="team-group-rows">
      {machine.bots.map((bot) => <MachineBotRow key={bot.id} machine={machine} bot={bot} selected={selected?.machineId === machine.id && selected.botId === bot.id} onOpen={() => onOpen(machine, bot)} />)}
    </div>}
  </section>)}</>
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
