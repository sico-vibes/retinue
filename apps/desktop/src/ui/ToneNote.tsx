import type { ReactNode } from 'react'
import { AlertTriangle, CircleX, Info, type LucideIcon } from 'lucide-react'
import './tone.css'

export type Tone = 'danger' | 'warning' | 'info'

/** The default icon for each tone; a caller may pass a more specific one (a key for sign-in, a shield for bypass). */
export const TONE_ICON: Record<Tone, LucideIcon> = { danger: CircleX, warning: AlertTriangle, info: Info }

/**
 * A notice with a status (owner, 15 November 2026): a filled surface with the
 * status colour washing in from the right edge, and the icon on the left beside
 * the text. Never an outline in the status colour ("neon").
 */
export function ToneNote({ tone, icon, role = 'status', className = '', children, actions }: { tone: Tone; icon?: LucideIcon; role?: 'status' | 'alert' | 'note'; className?: string; children: ReactNode; actions?: ReactNode }) {
  const Icon = icon ?? TONE_ICON[tone]
  return <div className={`tone-note ${className}`.trim()} data-tone={tone} role={role}>
    <span className="tone-icon" aria-hidden="true"><Icon size={14} /></span>
    <div className="tone-note-body">{children}</div>
    {actions && <div className="tone-note-actions">{actions}</div>}
  </div>
}
