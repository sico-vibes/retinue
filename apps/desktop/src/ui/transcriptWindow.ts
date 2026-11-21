// The transcript renders its newest rows only (21 November 2026, the owner's decision after
// docs/research/performance-2026-11.md, item 3): a long conversation mounts a page, not every row,
// and older rows load as the owner scrolls up. Anything that jumps to a row asks for it here, which
// mounts the whole conversation first when the row is outside the page.

/** Rows mounted when a conversation opens, and how many more each step up loads. */
export const TRANSCRIPT_PAGE = 80
/** How close to the top (px) the transcript loads the previous page. */
export const LOAD_EARLIER_WITHIN = 400

const SHOW_ALL = 'retinue:transcript-show-all'

/** The element for a transcript anchor, mounting every row first when it is outside the page. */
export function transcriptElement(anchor: string): HTMLElement | null {
  const element = document.getElementById(anchor)
  if (element || !anchor.startsWith('conversation-message-')) return element
  window.dispatchEvent(new Event(SHOW_ALL))
  return document.getElementById(anchor)
}

/** The transcript's listener for `transcriptElement`; it must mount the rows synchronously. */
export function onTranscriptShowAll(handler: () => void): () => void {
  window.addEventListener(SHOW_ALL, handler)
  return () => window.removeEventListener(SHOW_ALL, handler)
}

/**
 * The first index to render, given the page size: never in the middle of a run of rows that render
 * together (a strip of generated images), so the row that starts the run is always mounted with it.
 */
export function transcriptWindowStart(total: number, shown: number, joinsPrevious: (index: number) => boolean): number {
  let start = Math.max(0, total - shown)
  while (start > 0 && joinsPrevious(start)) start -= 1
  return start
}
