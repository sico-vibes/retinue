/** Dispatch this on `window` to start (or restart) the tour from anywhere, e.g. the profile menu. */
export const START_TOUR_EVENT = 'retinue:start-tour'

export function startTour() {
  window.dispatchEvent(new Event(START_TOUR_EVENT))
}
