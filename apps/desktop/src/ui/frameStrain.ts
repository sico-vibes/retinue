// Haze's frosted sheets stand down while the window is struggling (owner, 21 November 2026, after
// docs/research/performance-2026-11.md item 8): a backdrop blur is re-drawn whenever what is behind it
// moves, so while frames are long the sheets and dialogs use their solid glass colour instead, and the
// blur comes back on its own once the window is quiet again. The main backdrop, bot art and motion
// never change, and Stone has no blur to drop.
//
// It listens for the browser's own long-task reports (no animation-frame loop, so an idle window does
// no work for it) and sets `data-frame-strain` on the root, which the looks' CSS reads.

/** Long tasks (over 50 ms each) within the window that count as strain. */
const STRAIN_TASKS = 3
const STRAIN_WINDOW_MS = 2_000
/** Quiet time after the last long task before the blur comes back. */
const RECOVER_MS = 3_000

export function watchFrameStrain(root: HTMLElement = document.documentElement): () => void {
  if (typeof PerformanceObserver === 'undefined' || !PerformanceObserver.supportedEntryTypes?.includes('longtask')) return () => {}
  let recent: number[] = []
  let recoverTimer: number | null = null
  let ours = false
  const recover = () => {
    recoverTimer = null
    recent = []
    if (ours) { delete root.dataset.frameStrain; ours = false }
  }
  const observer = new PerformanceObserver((list) => {
    const now = performance.now()
    for (const entry of list.getEntries()) recent.push(entry.startTime)
    recent = recent.filter((time) => now - time <= STRAIN_WINDOW_MS)
    if (recent.length >= STRAIN_TASKS && root.dataset.frameStrain === undefined) { root.dataset.frameStrain = 'true'; ours = true }
    if (recoverTimer !== null) window.clearTimeout(recoverTimer)
    recoverTimer = window.setTimeout(recover, RECOVER_MS)
  })
  observer.observe({ type: 'longtask', buffered: false })
  return () => {
    observer.disconnect()
    if (recoverTimer !== null) window.clearTimeout(recoverTimer)
    if (ours) delete root.dataset.frameStrain
  }
}
