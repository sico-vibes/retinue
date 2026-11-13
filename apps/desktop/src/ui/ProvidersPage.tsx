// Settings › Coding agents › Providers (owner, 11 November 2026): Claude Code,
// Codex and OpenCode are always on; every other coding agent Retinue knows is
// off until it is switched on here. The daemon owns the list and the rule
// (`provider.list`, and `runtimes.disabled` for the switch); this page shows it.
import { useCallback, useEffect, useState } from 'react'
import { rpc } from '../tauri'
import { ProviderLogo } from './providerBrand'
import { SettingsGroup, SettingsRow } from './SettingsParts'
import { SettingsSkeleton } from './SettingsSkeleton'
import './accounts.css'

export interface ProviderRow {
  provider: string
  displayName: string
  main: boolean
  enabled: boolean
  found: boolean
  version?: string | null
  installHint?: string | null
}

const text = (value: unknown) => typeof value === 'string' && value.trim() ? value.trim() : null

export function parseProviders(reply: unknown): ProviderRow[] {
  const list = (reply as { providers?: unknown } | null)?.providers
  if (!Array.isArray(list)) return []
  return list.flatMap((item): ProviderRow[] => {
    const row = item as Record<string, unknown>
    const provider = text(row.provider)
    if (!provider) return []
    return [{ provider, displayName: text(row.displayName) ?? provider, main: row.main === true, enabled: row.enabled === true, found: row.found === true, version: text(row.version), installHint: text(row.installHint) }]
  })
}

/** What one row says under the name. Never a guess: an agent that was not found says so. */
export function providerLine(row: ProviderRow): string {
  if (!row.found) return row.installHint ? `Not installed. ${row.installHint}` : 'Not installed on this computer'
  const found = row.version ? `Installed · ${row.version}` : 'Installed'
  if (row.main) return found
  return row.enabled ? found : `${found} · Off: not offered to employees`
}

export function ProvidersPage({ disabled, saving, onChange }: {
  /** The stored `runtimes.disabled` list. An optional provider's own id in it means off. */
  disabled: readonly string[]
  saving?: boolean
  /** Saves the next `runtimes.disabled` list; resolves to an error message, or nothing when it saved. */
  onChange: (next: string[]) => Promise<string | null | undefined | void>
}) {
  const [rows, setRows] = useState<ProviderRow[] | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const load = useCallback(() => {
    void rpc<unknown>('provider.list', {}).then((reply) => { setRows(parseProviders(reply)); setProblem(null) }).catch(() => { setRows([]); setProblem('The list of coding agents could not be read.') })
  }, [])
  useEffect(load, [load])
  const toggle = async (row: ProviderRow) => {
    const next = row.enabled ? [...new Set([...disabled, row.provider])] : disabled.filter((id) => id !== row.provider)
    // Shown at once, then replaced by what the daemon says is true.
    setRows((current) => current?.map((item) => item.provider === row.provider ? { ...item, enabled: !row.enabled } : item) ?? current)
    await onChange(next)
    load()
  }
  if (rows === null) return <SettingsSkeleton />
  const main = rows.filter((row) => row.main)
  const optional = rows.filter((row) => !row.main)
  const rowOf = (row: ProviderRow) => <SettingsRow key={row.provider} label={<span className="provider-row-name"><ProviderLogo provider={row.provider} size={16} />{row.displayName}</span>} hint={providerLine(row)}>
    {row.main
      ? <span className="provider-row-fixed">Always on</span>
      : <button type="button" className={`toggle ${row.enabled ? 'on' : ''}`} role="switch" aria-label={`Use ${row.displayName}`} aria-checked={row.enabled} disabled={saving} onClick={() => void toggle(row)}><i /></button>}
  </SettingsRow>
  return <>
    {problem && <p className="form-error" role="alert">{problem}</p>}
    <SettingsGroup title="Main">{main.map(rowOf)}</SettingsGroup>
    <p className="provider-note">These three are always available. To turn off one installed copy, use Agents.</p>
    {optional.length > 0 && <>
      <SettingsGroup title="Optional">{optional.map(rowOf)}</SettingsGroup>
      <p className="provider-note">Off until you switch one on. One that an employee or a conversation already uses stays on.</p>
    </>}
  </>
}
