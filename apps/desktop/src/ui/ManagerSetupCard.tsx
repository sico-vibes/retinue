// Sidebar setup stays available until a Manager exists.
import { useEffect, useRef, useState } from 'react'
import { BriefcaseBusiness } from 'lucide-react'
import type { Agent, Runtime } from '../types'
import { createDraft, type AgentDraft } from './agentForm'
import { providerBrand } from './providerBrand'
import { Select } from './Select'
import './managerSetup.css'

export function ManagerSetupCard({ runtimes, disabled, onCreate, openSignal = 0 }: {
  runtimes: readonly Runtime[]
  disabled: boolean
  onCreate: (draft: AgentDraft) => Promise<Agent | null>
  /** Bumped by "Hire your CEO" in the empty conversation (owner, 16 November 2026): opens the form and puts focus in it. */
  openSignal?: number
}) {
  const [open, setOpen] = useState(false)
  const card = useRef<HTMLElement>(null)
  const nameInput = useRef<HTMLInputElement>(null)
  useEffect(() => { if (openSignal) setOpen(true) }, [openSignal])
  useEffect(() => {
    if (!openSignal || !open) return
    card.current?.scrollIntoView?.({ block: 'nearest', behavior: 'smooth' })
    nameInput.current?.focus()
    nameInput.current?.select()
  }, [openSignal, open])
  const [name, setName] = useState('CEO')
  const [runtimeId, setRuntimeId] = useState(runtimes[0]?.id ?? '')
  const [busy, setBusy] = useState(false)
  const selectedRuntimeId = runtimes.find((runtime) => runtime.id === runtimeId)?.id ?? runtimes[0]?.id ?? ''
  const create = async () => {
    if (busy || disabled || !selectedRuntimeId) return
    setBusy(true)
    try {
      const draft: AgentDraft = { ...createDraft(runtimes, selectedRuntimeId), name: name.trim() || 'CEO', runtimeId: selectedRuntimeId, color: 'violet' }
      await onCreate(draft)
    } finally { setBusy(false) }
  }
  return <section ref={card} className="manager-setup" aria-label="Set up your CEO">
    <header>
      <span className="manager-setup-icon" aria-hidden="true"><BriefcaseBusiness size={14} /></span>
      <strong>Meet your CEO</strong>
    </header>
    <p>Plans the work with you across every employee. It has Full access on this device, and team changes need your approval.</p>
    {!selectedRuntimeId && <p role="status">No coding agent is ready yet.</p>}
    {!open
      ? <button type="button" className="primary-button small" disabled={disabled || !selectedRuntimeId} onClick={() => setOpen(true)}>Set it up</button>
      : <div className="manager-setup-form">
        <label className="settings-field"><span>Name</span><input ref={nameInput} className="text-input" value={name} maxLength={40} onChange={(event) => setName(event.target.value)} /></label>
        <div className="settings-field"><span>Coding agent</span><Select ariaLabel="Coding agent" variant="field" value={selectedRuntimeId} disabled={disabled || busy || !selectedRuntimeId} onChange={setRuntimeId} options={runtimes.map((runtime) => ({ value: runtime.id, label: providerBrand(runtime.provider).name, provider: runtime.provider }))} /></div>
        <div className="manager-setup-actions">
          <button type="button" className="ghost-button small" onClick={() => setOpen(false)}>Cancel</button>
          <button type="button" className="primary-button small" disabled={busy || disabled || !selectedRuntimeId} onClick={() => void create()}>{busy ? 'Setting up…' : 'Create CEO'}</button>
        </div>
      </div>}
  </section>
}
