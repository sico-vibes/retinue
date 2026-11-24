// Whether the preview draws the installed window around the page, and what the native backdrop would report (productionFrame.ts).
export function productionFrameWanted(search = location.search, top = window.self === window.top, path = location.pathname) {
  const params = new URLSearchParams(search)
  if (params.get('frame') === '0' || params.has('companion') || params.has('dictation')) return false
  return top && (path === '/' || path === '/index.html')
}

/** What the shell reports for the native backdrop: acrylic while the frame is on and Haze asks for it. */
export function previewBackdrop(enabled: boolean, frame = productionFrameWanted()) {
  return enabled && frame
    ? { effect: 'acrylic' as const, active: true, transparencyOff: false }
    : { effect: 'none' as const, active: false, transparencyOff: false, reason: 'Not running in the desktop app.' }
}
