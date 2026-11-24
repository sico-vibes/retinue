// Other devices in Professional mode (owner, 24 November 2026: "pro mode should also impact connected machines and devices
// the same way"): each machine is a group, and its conversations are the same rows as local chats (provider logo, title,
// status badge, last-message time) instead of its employees. Opening a row opens that conversation read as if native.
import { useCallback, useEffect, useMemo, useState } from 'react'
import { CheckCheck, ChevronRight, Monitor, Server } from 'lucide-react'
import { rpc } from '../tauri'
import { Collapse } from './Collapse'
import { ProRow } from './ProfessionalSidebar'
import { proBadgeFor } from './ProBadge'
import { isSettled, useSettledChoices } from './settledStore'
import { normalizeMachineSessions, useMachineSessionChanges, type Machine, type MachineBot, type MachineSessionRow } from './machines'
import type { Session } from '../types'

const FOLDED_KEY = 'retinue.pro.machines.folded'
const readFolded = () => { try { return new Set<string>(JSON.parse(window.localStorage.getItem(FOLDED_KEY) ?? '[]')) } catch { return new Set<string>() } }

/** The few fields the settle rule and the badge read, from a row another machine reported. */
const asSession = (row: MachineSessionRow): Session => ({ id: `${row.machineId}:${row.id}`, state: row.state ?? 'idle', createdAt: row.updatedAt ?? '', updatedAt: row.updatedAt ?? '', lastMessageAt: row.updatedAt ?? undefined, messages: [] }) as unknown as Session

export function ProfessionalMachines({ machines, selected, onOpen }: {
  machines: readonly Machine[]
  selected: { machineId: string; botId: string; sessionId?: string } | null
  onOpen: (machine: Machine, bot: MachineBot, sessionId: string) => void
}) {
  const [rows, setRows] = useState<Record<string, MachineSessionRow[]>>({})
  const [folded, setFolded] = useState(readFolded)
  const [settledOpen, setSettledOpen] = useState<ReadonlySet<string>>(() => new Set())
  const choices = useSettledChoices()
  const ids = machines.map((machine) => machine.id).join('|')
  const refresh = useCallback(() => {
    for (const id of ids ? ids.split('|') : []) void rpc('machine.sessions', { machineId: id }).then(normalizeMachineSessions).then((list) => setRows((current) => ({ ...current, [id]: list.filter((row) => !row.archived) }))).catch(() => undefined)
  }, [ids])
  useEffect(() => { refresh(); const timer = window.setInterval(refresh, 20_000); return () => window.clearInterval(timer) }, [refresh])
  useMachineSessionChanges(machines.length > 0, refresh)
  const now = Date.now()
  const botsOf = useMemo(() => new Map(machines.map((machine) => [machine.id, new Map(machine.bots.map((bot) => [bot.id, bot]))])), [machines])
  if (!machines.length) return null
  const toggle = (id: string) => setFolded((current) => { const next = new Set(current); if (next.has(id)) next.delete(id); else next.add(id); try { window.localStorage.setItem(FOLDED_KEY, JSON.stringify([...next])) } catch { /* kept until the window closes */ } return next })

  return <div className="pro-machines">
    <div className="pro-machines-label"><Monitor size={11} aria-hidden="true" />Other devices</div>
    {machines.map((machine) => {
      const list = rows[machine.id] ?? []
      const isFolded = folded.has(machine.id)
      const offline = machine.state !== 'online'
      const live = list.filter((row) => !isSettled(asSession(row), now, choices))
      const settled = list.filter((row) => isSettled(asSession(row), now, choices))
      const renderRow = (row: MachineSessionRow) => {
        const bot = row.agentId ? botsOf.get(machine.id)?.get(row.agentId) : undefined
        return <ProRow key={row.id} title={row.title?.trim() || 'New chat'} provider={bot?.provider} badge={proBadgeFor({ state: row.state ?? 'idle' }, row.state === 'waiting_permission')} stamp={row.updatedAt ?? ''} selected={selected?.machineId === machine.id && selected.sessionId === row.id} dimmed={offline} onOpen={() => { if (bot) onOpen(machine, bot, row.id) }} />
      }
      const settledShown = settledOpen.has(machine.id)
      return <section key={machine.id} className="pro-group" aria-label={machine.name}>
        <div className="pro-group-head">
          <button type="button" className="pro-group-fold" aria-expanded={!isFolded} onClick={() => toggle(machine.id)} title={offline ? 'Shown as it was when last reached.' : machine.live ? 'Live.' : 'Checked every 20 seconds.'}>
            <ChevronRight size={12} aria-hidden="true" className={`fold-chevron${isFolded ? '' : ' open'}`} />
            {machine.kind === 'linux' ? <Server size={14} aria-hidden="true" /> : <Monitor size={14} aria-hidden="true" />}
            <span className="pro-group-name">{machine.name}</span><i className={`pro-dot ${offline ? 'off' : 'on'}`} aria-hidden="true" />
            <span className="pro-count">{list.length}</span>
          </button>
        </div>
        <Collapse open={!isFolded}>
          <ul role="tree" className="pro-list">{live.map(renderRow)}</ul>
          {live.length === 0 && settled.length === 0 && <p className="pro-empty">{offline ? 'Not reachable right now.' : 'No chats yet.'}</p>}
          {settled.length > 0 && <div className="pro-settled">
            <button type="button" className="pro-settled-head" aria-expanded={settledShown} onClick={() => setSettledOpen((current) => { const next = new Set(current); if (settledShown) next.delete(machine.id); else next.add(machine.id); return next })}>
              <ChevronRight size={12} aria-hidden="true" className={`fold-chevron${settledShown ? ' open' : ''}`} /><CheckCheck size={13} aria-hidden="true" /><span>Settled</span><span className="pro-count">{settled.length}</span>
            </button>
            <Collapse open={settledShown}><ul role="tree" className="pro-list pro-list-settled">{settled.map(renderRow)}</ul></Collapse>
          </div>}
        </Collapse>
      </section>
    })}
  </div>
}
