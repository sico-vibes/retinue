// After a new project (owner, 8 November 2026): staff it with the CEO's help,
// hire by hand, or skip. Either path can be taken later from the project's +.
import { useEffect, useRef } from 'react'
import { MessagesSquare, UserPlus, X } from 'lucide-react'
import type { Project } from '../types'
import './teamPage.css'

export function StaffProjectDialog({ project, ceoName, canAskCeo, onAskCeo, onHire, onSkip }: {
  project: Project
  ceoName: string
  canAskCeo: boolean
  onAskCeo: () => void
  onHire: () => void
  onSkip: () => void
}) {
  const first = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    first.current?.focus()
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') { event.preventDefault(); onSkip() } }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [onSkip])
  return <div className="team-dialog-backdrop" onPointerDown={(event) => { if (event.target === event.currentTarget) onSkip() }}>
    <section className="team-dialog staff-dialog" role="dialog" aria-modal="true" aria-label={`Staff ${project.name}`}>
      <header className="team-dialog-head">
        <div><strong>Who should work on {project.name}?</strong><span>You can change the team any time.</span></div>
        <button type="button" className="icon-button tiny" aria-label="Close" onClick={onSkip}><X size={15} /></button>
      </header>
      <div className="staff-options">
        <button ref={first} type="button" className="staff-option" disabled={!canAskCeo} onClick={onAskCeo}>
          <MessagesSquare size={18} aria-hidden="true" />
          <span><strong>Interview with {ceoName}</strong><small>Explain the project; {ceoName} suggests who to hire.</small></span>
        </button>
        <button type="button" className="staff-option" onClick={onHire}>
          <UserPlus size={18} aria-hidden="true" />
          <span><strong>I'll hire myself</strong><small>Pick employees from the list of specialists.</small></span>
        </button>
      </div>
      <footer className="team-dialog-actions"><button type="button" className="ghost-button small" onClick={onSkip}>Not now</button></footer>
    </section>
  </div>
}
