import { useEffect, useRef, useState, type CSSProperties } from 'react'
import { ProviderLogo } from './providerBrand'
import { usePopoverDismiss } from './ComposerControls'
import { LimitMeter, UsagePanel } from './QuotaBar'
import { mostPressingWindow, quotaRows, quotaTone, resetIn, type QuotaProviderRow, type QuotaWindowRow } from './quotaModel'
import './sidebarUsage.css'

/** Current, accurate readings only: stale readings and failed checks are left out of the glance. */
export function currentReadings(rows: QuotaProviderRow[]): QuotaProviderRow[] {
  return rows.filter((row) => !row.stale && !row.status && row.windows.length > 0)
}

/**
 * One still line at the foot of the sidebar: the limit closest to running out,
 * across every coding agent. Clicking it opens the Usage panel with the rest.
 * With no reading it renders nothing.
 */
/** The two windows worth showing for a provider: the 5-hour session and the weekly limit when it reports them, else its first two. */
export function stackedWindows(row: QuotaProviderRow): QuotaWindowRow[] {
  const session = row.windows.find((item) => item.short === '5h')
  const weekly = row.windows.find((item) => item.short === 'wk')
  const picked = [session, weekly].filter((item): item is QuotaWindowRow => !!item)
  return picked.length ? picked : row.windows.slice(0, 2)
}

export function SidebarUsage({ provider, quotas, refreshing, onRefresh, onOpenDetails, onTurnOn }: { /** Professional mode: the provider of the conversation open now. Its 5-hour and weekly limits are shown instead of the one closest to running out. */ provider?: string | null; quotas: Record<string, unknown>[]; refreshing: boolean; onRefresh: () => void; onOpenDetails: () => void; onTurnOn?: () => void }) {
  const [now, setNow] = useState(() => Date.now())
  const [panelAt, setPanelAt] = useState<CSSProperties | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000)
    return () => window.clearInterval(timer)
  }, [])
  const close = (restoreFocus: boolean) => { setPanelAt(null); if (restoreFocus) requestAnimationFrame(() => triggerRef.current?.focus()) }
  usePopoverDismiss(panelAt !== null, rootRef, close)
  const rows = quotaRows(quotas)
  const readings = currentReadings(rows)
  const own = provider ? readings.find((item) => item.provider === provider) : undefined
  const pressing = mostPressingWindow(own ? [own] : readings)
  if (!pressing) return null
  const { row, window: reading } = pressing
  const stacked = provider !== undefined && own ? stackedWindows(own) : null
  const reset = resetIn(reading.resetsAt, now)
  const summary = `${row.name} ${reading.label}: ${reading.usedPercent}% used${reset ? `, resets in ${reset}` : ''}`
  // The panel is wider than the sidebar, so it is placed against the window rather than inside the column.
  const toggle = () => {
    if (panelAt) { close(false); return }
    const rect = triggerRef.current?.getBoundingClientRect()
    setPanelAt(rect ? { position: 'fixed', left: rect.left, bottom: window.innerHeight - rect.top + 6, transformOrigin: 'bottom left' } : {})
  }
  return <div ref={rootRef} className="sidebar-usage-root">
    {stacked ? <button ref={triggerRef} type="button" className="sidebar-usage foot-row stacked" onClick={toggle} aria-haspopup="dialog" aria-expanded={panelAt !== null} aria-label={`${row.name}: ${stacked.map((item) => `${item.label} ${item.usedPercent}% used`).join(', ')}. Open usage`} title={summary} data-tour="usage">
      <span className="foot-row-icon"><ProviderLogo provider={row.provider} size={16} /></span>
      <span className="usage-lines">{stacked.map((item) => <span key={item.short} className="usage-line"><em>{item.short}</em><LimitMeter percent={item.usedPercent} /><b className={`sidebar-usage-value tone-${quotaTone(item.usedPercent)}`}>{item.usedPercent}%</b></span>)}</span>
    </button> : <button ref={triggerRef} type="button" className="sidebar-usage foot-row" onClick={toggle} aria-haspopup="dialog" aria-expanded={panelAt !== null} aria-label={`${summary}. Open usage`} title={summary} data-tour="usage">
      <span className="foot-row-icon"><ProviderLogo provider={row.provider} size={14} /></span>
      <span className="foot-row-label">{reading.label}</span>
      <LimitMeter percent={reading.usedPercent} />
      <b className={`sidebar-usage-value tone-${quotaTone(reading.usedPercent)}`}>{reading.usedPercent}%</b>
    </button>}
    {panelAt && <UsagePanel rows={rows} refreshing={refreshing} onRefresh={onRefresh} style={panelAt} onOpenDetails={() => { close(false); onOpenDetails() }} onTurnOn={onTurnOn ? () => { close(false); onTurnOn() } : undefined} />}
  </div>
}
