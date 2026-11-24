import { demoEntries, type DemoEntry } from './demoRegistry'
import { subscribeToUnknownRpcs, unknownRpcMethods } from './demoDiagnostics'
import { runSteps } from './inspector/scenarioSteps'
import './demoMenu.css'
import './productionFrame'

const preference = 'retinue.preview.demoArea'
const areas = [...new Set(demoEntries.map((entry) => entry.area))]
const params = new URLSearchParams(location.search)
if (params.get('demo-clipboard') === 'write') {
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: async () => { throw new DOMException('Clipboard permission denied.', 'NotAllowedError') } },
  })
}
const current = demoEntries.find((entry) => entry.id === params.get('demo-entry'))
let area = current?.area ?? areas[0]
try {
  const saved = localStorage.getItem(preference)
  if (saved && areas.includes(saved)) area = saved
} catch { /* The menu also works without browser storage. */ }

function go(entry?: DemoEntry) {
  const url = new URL(entry?.url ?? (params.has('companion') ? '/?companion' : location.pathname), location.origin)
  for (const key of ['style', 'theme']) {
    const value = params.get(key) ?? document.documentElement.dataset[key]
    if (value) url.searchParams.set(key, value)
  }
  if (entry) url.searchParams.set('demo-entry', entry.id)
  location.assign(url)
}

function button(label: string, action: () => void) {
  const element = document.createElement('button')
  element.type = 'button'
  element.textContent = label
  element.addEventListener('click', action)
  return element
}

const host = document.createElement('aside')
host.className = 'retinue-preview-demo'
host.setAttribute('aria-label', 'Preview controls')
const panel = document.createElement('section')
panel.id = 'retinue-preview-demo-panel'
panel.hidden = true
panel.setAttribute('aria-label', 'Demo states')
const badge = button('Demo', () => setOpen(panel.hidden))
badge.setAttribute('aria-controls', panel.id)
badge.setAttribute('aria-expanded', 'false')
const picker = document.createElement('select')
picker.setAttribute('aria-label', 'Demo area')
for (const name of areas) picker.add(new Option(name, name))
picker.value = area
const entries = document.createElement('ul')
const unknown = document.createElement('details')
const summary = document.createElement('summary')
const methods = document.createElement('ul')
unknown.append(summary, methods)

function setOpen(open: boolean) {
  panel.hidden = !open
  badge.setAttribute('aria-expanded', String(open))
  if (open) picker.focus()
  else badge.focus()
}

function renderEntries() {
  const group = demoEntries.filter((entry) => entry.area === area)
  entries.replaceChildren(...group.map((entry) => {
    const item = document.createElement('li')
    const control = button(entry.label, () => go(entry))
    if (entry === current) control.setAttribute('aria-current', 'true')
    item.append(control)
    return item
  }))
}

function renderUnknown() {
  const names = unknownRpcMethods()
  summary.textContent = `Not demoed yet (${names.length})`
  methods.replaceChildren(...names.map((name) => {
    const method = document.createElement('li')
    method.textContent = name
    return method
  }))
}

picker.addEventListener('change', () => {
  area = picker.value
  try { localStorage.setItem(preference, area) } catch { /* Optional browser storage. */ }
  renderEntries()
})
const next = button('Next state', () => {
  const group = demoEntries.filter((entry) => entry.area === area)
  const index = current ? group.indexOf(current) : -1
  go(group[(index + 1) % group.length])
})
const actions = document.createElement('div')
actions.append(next, button('Reset demo', () => go()))
panel.append(picker, actions, entries, unknown)
host.append(badge, panel)
host.addEventListener('keydown', (event) => {
  if (event.key !== 'Escape' || panel.hidden) return
  event.stopPropagation()
  setOpen(false)
})
document.addEventListener('pointerdown', (event) => {
  if (!panel.hidden && event.target instanceof Node && !host.contains(event.target)) {
    panel.hidden = true
    badge.setAttribute('aria-expanded', 'false')
  }
})
renderEntries()
renderUnknown()
subscribeToUnknownRpcs(renderUnknown)
// In the production frame the body is the window's client area; the badge lives beside the title bar, outside it.
;(document.documentElement.dataset.pvFrame ? document.documentElement : document.body).append(host)

if (current?.steps) void runSteps(() => document, current.steps).then((problem) => {
  if (!problem) return
  const status = document.createElement('p')
  status.setAttribute('role', 'status')
  status.textContent = `Could not open this demo: ${problem}`
  panel.prepend(status)
  setOpen(true)
})

