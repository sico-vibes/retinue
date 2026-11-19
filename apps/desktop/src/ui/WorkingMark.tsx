// The working state in a conversation (owner, 14 November 2026): React Bits'
// Lattice Loader with the arrow pattern. Haze uses square cells (rounded
// rectangles like the rest of Haze); Stone uses round ones.
//
// Owner, 15 November 2026: thinking and doing look different. While a tool runs,
// the lattice gives way to that step's icon with its own small motion, and the
// words say the step ("Running npm test") with a shimmer. The timer keeps
// counting the whole turn, because the loader stays mounted.
import { FilePen, FileText, Globe, Search, SquareTerminal, Wrench, type LucideIcon } from 'lucide-react'
import LatticeLoader from './reactbits/LatticeLoader'
import './reactbits/LatticeLoader.css'
import { useThemeStyle } from '../blob/retinue'
import type { DoingKind } from './doingNow'
import './workingMark.css'

const STEP_ICON: Record<DoingKind, LucideIcon> = { command: SquareTerminal, read: FileText, edit: FilePen, search: Search, web: Globe, tool: Wrench }

export function WorkingMark({ label, timer = true, compact = false, step = null }: { label: string; timer?: boolean; compact?: boolean; step?: { kind: DoingKind; label: string } | null }) {
  const style = useThemeStyle()
  const Icon = step ? STEP_ICON[step.kind] : null
  return <span className={`working-step${step ? ' is-doing' : ''}`} data-kind={step?.kind}>
    {Icon && <span className="working-step-icon" aria-hidden="true"><Icon size={compact ? 12 : 14} strokeWidth={2} />{step!.kind === 'command' && <i className="working-step-caret" />}</span>}
    <LatticeLoader className="working-mark" label={step ? step.label : label} pattern="arrow" grid={3} shape={style === 'stone' ? 'round' : 'square'} cellSize={compact ? 3 : 4} gap={compact ? 1 : 1.5} fontSize={compact ? 12 : 13} showTimer={timer} />
  </span>
}
