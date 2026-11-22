// The bar an imported, read-only conversation shows in place of its message box
// (conversation import, 22 November 2026; design docs/research/conversation-import.md,
// sections 4.2 and 4.9). The daemon decides what "continue" means: the same coding
// agent with the source file still there resumes natively; anything else starts a
// new conversation for that employee from a bounded summary (`import.continue`).
import { useState } from 'react'
import { History } from 'lucide-react'
import type { Agent, ImportContinueResult, Session } from '../types'
import { rpc } from '../tauri'
import { providerBrand } from './providerBrand'
import { HandoffDialog } from './TeamMessages'
import './importDialog.css'

/** The employee who can pick the conversation up in its own coding agent: the conversation's owner first, then anyone else on that agent. */
export function continuingAgent(agents: readonly Agent[], session: Pick<Session, 'agentId'> & { imported?: Session['imported'] }, providerOf: (agent: Agent) => string | null): Agent | null {
  const source = session.imported?.source
  if (!source) return null
  const able = agents.filter((agent) => !agent.archived && !agent.hidden && providerOf(agent) === source)
  return able.find((agent) => agent.id === session.agentId) ?? able[0] ?? null
}

const message = (error: unknown) => error instanceof Error && error.message ? error.message : 'The conversation could not be handed over.'

export function ImportedBar({ session, agents, providerOf, onContinued }: {
  session: Session
  agents: readonly Agent[]
  providerOf: (agent: Agent) => string | null
  /** The daemon's answer: `continue` changed this conversation; `carry_over` made a new one with a draft to put in its composer. */
  onContinued: (result: ImportContinueResult, agent: Agent) => void
}) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [picking, setPicking] = useState(false)
  const source = session.imported?.source ?? ''
  const sourceName = providerBrand(source).name
  const able = continuingAgent(agents, session, providerOf)
  const others = agents.filter((agent) => !agent.archived && !agent.hidden)
  const go = async (agent: Agent) => {
    setBusy(true); setError(null)
    try {
      const result = await rpc<ImportContinueResult>('import.continue', { sessionId: session.id, agentId: agent.id })
      setPicking(false)
      onContinued(result, agent)
    } catch (reason) { setError(message(reason)) } finally { setBusy(false) }
  }
  return <div className="imported-bar" role="status">
    <History size={15} aria-hidden="true" />
    <span><strong>Imported from {sourceName}.</strong> This is read-only history.{error ? <em role="alert"> {error}</em> : null}</span>
    <span className="imported-bar-actions">{others.length > 0 && <button type="button" className="secondary-button small" disabled={busy} onClick={() => { setError(null); setPicking(true) }}>Give this to another employee…</button>}
    {able && <button type="button" className="primary-button small" disabled={busy} aria-busy={busy || undefined} onClick={() => void go(able)}>Continue with {able.name}</button>}</span>
    {picking && <HandoffDialog agents={agents} sourceAgentId={able?.id ?? null} busy={busy} error={error} providerOf={providerOf} onClose={() => setPicking(false)} onPick={(agent) => void go(agent)} />}
  </div>
}
