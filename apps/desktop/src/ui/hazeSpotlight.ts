// Feeds the Haze spotlight hover (themes/haze-polish.css): one delegated listener sets --mx and --my on the row under the pointer.
const ROW = '.ps-bot, .ps-convo, .bot-row, .convo-row, .team-card, .plan-card, .activity-card, .schedule-card, .tree-card, .delegate-chip'

export function installHazeSpotlight(root: Document = document) {
  let frame = 0
  const onMove = (event: PointerEvent) => {
    const row = (event.target as Element | null)?.closest?.<HTMLElement>(ROW)
    if (!row || frame) return
    const { clientX, clientY } = event
    frame = requestAnimationFrame(() => {
      frame = 0
      const rect = row.getBoundingClientRect()
      row.style.setProperty('--mx', `${clientX - rect.left}px`)
      row.style.setProperty('--my', `${clientY - rect.top}px`)
    })
  }
  root.addEventListener('pointermove', onMove, { passive: true })
  return () => { root.removeEventListener('pointermove', onMove); cancelAnimationFrame(frame) }
}
