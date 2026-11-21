import { SCENARIOS } from './inspector/scenarios'

const scenarios = SCENARIOS.filter(({ id }) => id.startsWith('scene-turn-'))
const badge = document.createElement('details')
badge.setAttribute('aria-label', 'Demo menu')
badge.style.cssText = 'position:fixed;right:160px;top:10px;z-index:10001;background:var(--surface-raised);color:var(--text-1);padding:6px 10px;border:1px solid var(--line-strong);border-radius:8px'

const summary = document.createElement('summary')
summary.textContent = 'Demo'
badge.append(summary)

const menu = document.createElement('nav')
menu.setAttribute('aria-label', 'Demo scenarios')
menu.style.cssText = 'display:grid;gap:6px;padding:6px 0'
for (const scenario of [...scenarios, { title: 'All scenarios', url: '/inspector.html' }]) {
  const link = document.createElement('a')
  const params = new URLSearchParams(location.search)
  for (const key of ['scene', 'session', 'agent', 'open']) params.delete(key)
  const target = new URL(scenario.url, location.href)
  target.searchParams.forEach((value, key) => params.set(key, value))
  target.search = params.toString()
  link.href = target.href
  link.textContent = scenario.title
  link.style.color = 'inherit'
  menu.append(link)
}
badge.append(menu)
document.body.append(badge)
