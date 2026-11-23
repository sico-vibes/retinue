// Settings › About (owner, 22 November 2026: the Zephium-style redesign asked for an About page like theirs).
// A hero with Retinue's own tile and name, the running version, a row of link tiles to real places, then the
// software pages this window already has. Nothing here is fetched or guessed: the version comes from the build.
import { ArrowUpRight, Bug, FileText, Github, HeartPulse, RefreshCw, Scale, ArchiveRestore } from 'lucide-react'
import type { ComponentType } from 'react'
import retinueTileDark from '../assets/brand/retinue-tile-dark.png'
import retinueTileLight from '../assets/brand/retinue-tile-light.png'
import { appVersion } from '../appRelease'
import { openSafeLink } from './openSafeLink'
import { SettingsGroup, SettingsRow } from './SettingsParts'

const REPO = 'https://github.com/sico-vibes/retinue'

export const ABOUT_LINKS: Array<{ label: string; url: string; Icon: ComponentType<{ size?: number; 'aria-hidden'?: boolean }> }> = [
  { label: 'GitHub', url: REPO, Icon: Github },
  { label: 'Release notes', url: `${REPO}/releases`, Icon: FileText },
  { label: 'Report a problem', url: `${REPO}/issues/new`, Icon: Bug },
  { label: 'Licence', url: `${REPO}/blob/main/LICENSE`, Icon: Scale },
]

export function AboutPage({ onOpenPage, canUpdate }: { onOpenPage: (page: 'Updates' | 'Health' | 'Backup') => void; canUpdate: boolean }) {
  return <div className="about-page">
    <section className="about-hero" aria-label="Retinue">
      <span className="about-tile" aria-hidden="true">
        <img className="theme-on-dark" src={retinueTileDark} alt="" />
        <img className="theme-on-light" src={retinueTileLight} alt="" />
      </span>
      <h3 className="about-wordmark">Retinue</h3>
      <p className="about-version">{appVersion ? <>Version {appVersion}</> : 'Version unknown'}</p>
      <p className="about-tagline">A team of coding agents on your desktop. Open source under the MIT licence.</p>
      <nav className="about-links" aria-label="Retinue links">
        {ABOUT_LINKS.map(({ label, url, Icon }) => <button key={label} type="button" className="about-link" onClick={() => void openSafeLink(url)}>
          <span className="about-link-tile"><Icon size={18} aria-hidden /></span>
          <span className="about-link-label">{label}</span>
        </button>)}
      </nav>
    </section>
    <SettingsGroup title="Software">
      {canUpdate && <SettingsRow label="Updates" hint="Check for a new version and choose how Retinue updates itself.">
        <button type="button" className="secondary-button small" onClick={() => onOpenPage('Updates')}><RefreshCw size={13} aria-hidden="true" />Open</button>
      </SettingsRow>}
      <SettingsRow label="Health check" hint="Checks the Retinue service, your coding agents and this computer, and builds a diagnostics file for a bug report.">
        <button type="button" className="secondary-button small" onClick={() => onOpenPage('Health')}><HeartPulse size={13} aria-hidden="true" />Open</button>
      </SettingsRow>
      <SettingsRow label="Backup" hint="Save your employees, projects and settings to a file, or restore them from one.">
        <button type="button" className="secondary-button small" onClick={() => onOpenPage('Backup')}><ArchiveRestore size={13} aria-hidden="true" />Open</button>
      </SettingsRow>
      <SettingsRow label="Source code" hint="Read the code, follow the work and suggest changes on GitHub.">
        <button type="button" className="secondary-button small" onClick={() => void openSafeLink(REPO)}>GitHub<ArrowUpRight size={13} aria-hidden="true" /></button>
      </SettingsRow>
    </SettingsGroup>
  </div>
}
