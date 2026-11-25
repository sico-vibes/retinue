// The tab strip of Professional mode: the conversations you have open, one tab each, with a plus at the end. Closing a tab
// only closes the tab; the conversation keeps its place in the list and nothing running stops. A chat that lives on another
// machine is a tab like any other (owner, 25 November 2026: it must not feel different). See workMode.ts.
import { useEffect, useState } from 'react'
import { Plus, X } from 'lucide-react'
import type { Agent, Runtime, Session } from '../types'
import { RuntimeMark } from './providerBrand'
import type { Machine, MachineBot } from './machines'
import './professional.css'

const KEY = 'retinue.pro.tabs'
const META_KEY = 'retinue.pro.remoteTabs'
const MAX_TABS = 12
const load = (): string[] => { try { const raw = JSON.parse(window.localStorage.getItem(KEY) ?? '[]'); return Array.isArray(raw) ? raw.filter((id): id is string => typeof id === 'string') : [] } catch { return [] } }
const save = (tabs: string[]) => { try { window.localStorage.setItem(KEY, JSON.stringify(tabs)) } catch { /* kept until the window closes */ } }

/** A tab key for a chat on another machine. Local chats use the plain session id. */
export const remoteTabKey = (machineId: string, botId: string, sessionId: string) => `m|${machineId}|${botId}|${sessionId}`
type RemoteMeta = { title: string; provider: string | null; machineName: string }
const loadMeta = (): Record<string, RemoteMeta> => { try { const raw = JSON.parse(window.localStorage.getItem(META_KEY) ?? '{}'); return raw && typeof raw === 'object' ? raw : {} } catch { return {} } }
/** What a remote tab shows, kept when its chat is opened so the tab can be drawn before the machine answers. */
export function rememberRemoteTab(machineId: string, botId: string, sessionId: string, meta: RemoteMeta) {
  try { window.localStorage.setItem(META_KEY, JSON.stringify({ ...loadMeta(), [remoteTabKey(machineId, botId, sessionId)]: meta })) } catch { /* the tab falls back to a plain title */ }
}

type RemoteView = { machineId: string; botId: string; sessionId?: string }

export function ProfessionalTabs({ sessions, agents, runtimes, selectedSessionId, onOpen, onNew, machines = [], machineView = null, onOpenRemote, onCloseRemote }: {
  sessions: readonly Session[]; agents: readonly Agent[]; runtimes: readonly Runtime[]
  selectedSessionId: string | null
  onOpen: (agent: Agent, session: Session) => void
  onNew: () => void
  machines?: readonly Machine[]
  machineView?: RemoteView | null
  onOpenRemote?: (machine: Machine, bot: MachineBot, sessionId: string) => void
  onCloseRemote?: () => void
}) {
  const [open, setOpen] = useState<string[]>(load)
  const remoteActive = machineView?.sessionId ? remoteTabKey(machineView.machineId, machineView.botId, machineView.sessionId) : null
  // The conversation you are in always has a tab, and only that one is selected.
  const activeKey = remoteActive ?? selectedSessionId
  useEffect(() => {
    if (!activeKey) return
    setOpen((current) => {
      if (current.includes(activeKey)) return current
      const next = [...current, activeKey].slice(-MAX_TABS)
      save(next)
      return next
    })
  }, [activeKey])
  const byId = new Map(sessions.map((session) => [session.id, session]))
  const meta = loadMeta()
  type Tab = { key: string; title: string; provider: string | null | undefined; hint: string; activate: (() => void) | null }
  const tabs: Tab[] = open.flatMap((key): Tab[] => {
    if (key.startsWith('m|')) {
      const [, machineId, botId, sessionId] = key.split('|')
      const machine = machines.find((item) => item.id === machineId)
      const bot = machine?.bots.find((item) => item.id === botId)
      const info = meta[key]
      if (!machine || !bot || !sessionId) return []
      const title = info?.title?.trim() || 'Chat'
      return [{ key, title, provider: info?.provider ?? bot.provider, hint: `${title} (on ${machine.name})`, activate: onOpenRemote ? () => onOpenRemote(machine, bot, sessionId) : null }]
    }
    const session = byId.get(key)
    if (!session || session.archived) return []
    const agent = agents.find((item) => item.id === session.agentId)
    const runtime = runtimes.find((item) => item.id === (session.runtimeId ?? agent?.runtimeId))
    const title = session.title?.trim() || 'New chat'
    return [{ key, title, provider: runtime?.provider, hint: title, activate: agent ? () => onOpen(agent, session) : null }]
  })
  const close = (key: string) => {
    const index = tabs.findIndex((tab) => tab.key === key)
    const next = open.filter((item) => item !== key)
    setOpen(next)
    save(next)
    if (key === activeKey) {
      const neighbour = tabs[index + 1] ?? tabs[index - 1]
      if (neighbour?.activate) neighbour.activate()
      else if (remoteActive) onCloseRemote?.()
    }
  }
  return <div className="pro-tabs" role="tablist" aria-label="Open chats">
    {tabs.map((tab) => <div key={tab.key} role="tab" tabIndex={0} aria-selected={tab.key === activeKey} className="pro-tab" title={tab.hint}
      onClick={() => tab.activate?.()} onKeyDown={(event) => { if ((event.key === 'Enter' || event.key === ' ') && tab.activate) { event.preventDefault(); tab.activate() } }}>
      <RuntimeMark provider={tab.provider} size={14} />
      <span className="pro-tab-title">{tab.title}</span>
      <button type="button" className="pro-tab-close" aria-label={`Close ${tab.title}`} tabIndex={-1} onClick={(event) => { event.stopPropagation(); close(tab.key) }}><X size={12} aria-hidden="true" /></button>
    </div>)}
    <button type="button" className="pro-tab-add" aria-label="New chat" title="New chat" onClick={onNew}><Plus size={14} aria-hidden="true" /></button>
  </div>
}
