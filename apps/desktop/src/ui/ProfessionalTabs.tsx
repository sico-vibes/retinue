// The tab strip of Professional mode: the conversations you have open, one tab each, with a plus at the end. Closing a tab
// only closes the tab; the conversation keeps its place in the list and nothing running stops. See workMode.ts.
import { useEffect, useState } from 'react'
import { Plus, X } from 'lucide-react'
import type { Agent, Runtime, Session } from '../types'
import { RuntimeMark } from './providerBrand'
import './professional.css'

const KEY = 'retinue.pro.tabs'
const MAX_TABS = 12
const load = (): string[] => { try { const raw = JSON.parse(window.localStorage.getItem(KEY) ?? '[]'); return Array.isArray(raw) ? raw.filter((id): id is string => typeof id === 'string') : [] } catch { return [] } }

export function ProfessionalTabs({ sessions, agents, runtimes, selectedSessionId, onOpen, onNew }: {
  sessions: readonly Session[]; agents: readonly Agent[]; runtimes: readonly Runtime[]
  selectedSessionId: string | null
  onOpen: (agent: Agent, session: Session) => void
  onNew: () => void
}) {
  const [open, setOpen] = useState<string[]>(load)
  // The conversation you are in always has a tab.
  useEffect(() => {
    if (!selectedSessionId) return
    setOpen((current) => {
      if (current.includes(selectedSessionId)) return current
      const next = [...current, selectedSessionId].slice(-MAX_TABS)
      try { window.localStorage.setItem(KEY, JSON.stringify(next)) } catch { /* kept until the window closes */ }
      return next
    })
  }, [selectedSessionId])
  const byId = new Map(sessions.map((session) => [session.id, session]))
  const tabs = open.map((id) => byId.get(id)).filter((session): session is Session => !!session && !session.archived)
  const close = (id: string) => {
    const index = tabs.findIndex((session) => session.id === id)
    const next = open.filter((item) => item !== id)
    setOpen(next)
    try { window.localStorage.setItem(KEY, JSON.stringify(next)) } catch { /* kept until the window closes */ }
    if (id === selectedSessionId) {
      const neighbour = tabs[index + 1] ?? tabs[index - 1]
      const agent = neighbour && agents.find((item) => item.id === neighbour.agentId)
      if (neighbour && agent) onOpen(agent, neighbour)
    }
  }
  return <div className="pro-tabs" role="tablist" aria-label="Open chats">
    {tabs.map((session) => {
      const agent = agents.find((item) => item.id === session.agentId)
      const runtime = runtimes.find((item) => item.id === (session.runtimeId ?? agent?.runtimeId))
      const title = session.title?.trim() || 'New chat'
      return <div key={session.id} role="tab" tabIndex={0} aria-selected={session.id === selectedSessionId} className="pro-tab" title={title}
        onClick={() => { if (agent) onOpen(agent, session) }} onKeyDown={(event) => { if ((event.key === 'Enter' || event.key === ' ') && agent) { event.preventDefault(); onOpen(agent, session) } }}>
        <RuntimeMark provider={runtime?.provider} size={14} />
        <span className="pro-tab-title">{title}</span>
        <button type="button" className="pro-tab-close" aria-label={`Close ${title}`} tabIndex={-1} onClick={(event) => { event.stopPropagation(); close(session.id) }}><X size={12} aria-hidden="true" /></button>
      </div>
    })}
    <button type="button" className="pro-tab-add" aria-label="New chat" title="New chat" onClick={onNew}><Plus size={14} aria-hidden="true" /></button>
  </div>
}
