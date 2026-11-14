// The working state in a conversation (owner, 14 November 2026): React Bits'
// Lattice Loader with the arrow pattern. Haze uses square cells (rounded
// rectangles like the rest of Haze); Stone uses round ones.
import LatticeLoader from './reactbits/LatticeLoader'
import './reactbits/LatticeLoader.css'
import { useThemeStyle } from '../blob/retinue'

export function WorkingMark({ label, timer = true, compact = false }: { label: string; timer?: boolean; compact?: boolean }) {
  const style = useThemeStyle()
  return <LatticeLoader className="working-mark" label={label} pattern="arrow" grid={3} shape={style === 'stone' ? 'round' : 'square'} cellSize={compact ? 3 : 4} gap={compact ? 1 : 1.5} fontSize={compact ? 12 : 13} showTimer={timer} />
}
