// Captures Retinue's own screens from the desktop app's browser preview, in both looks, at 2x, using
// the Edge already installed on Windows (no browser download). The preview must be running:
// from the repo root, `npx vite --config apps/desktop/vite.preview.config.ts apps/desktop --port 5174`.
//
//   node scripts/capture.mjs                       every shot in SHOTS below, into public/shots/
//   node scripts/capture.mjs --list shots.json     a list of {name, path, width, height, clip?, out?}
//
// Each shot is saved as <out or public/shots>/<name>-dark.webp and <name>-light.webp.
import { readFileSync, mkdirSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright-core'
import sharp from 'sharp'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const BASE = process.env.PREVIEW_URL ?? 'http://127.0.0.1:5174'

/** path is the preview URL's path and query, without the look and theme. */
const SHOTS = [
  { name: 'main', path: '/?session=session-team-leader&agent=agent-codex', width: 1440, height: 900 },
  { name: 'working', path: '/?session=session-codex&agent=agent-codex', width: 1440, height: 900 },
  { name: 'team', path: '/?idle', width: 1440, height: 900, click: ['Team'] },
  { name: 'hire', path: '/?idle', width: 1440, height: 900, click: ['Team', 'Hire'] },
  { name: 'ceo', path: '/?session=session-manager&agent=agent-manager', width: 1440, height: 900 },
  { name: 'approve', path: '/?scene=approval-many', width: 1440, height: 900 },
  { name: 'queue', path: '/?scene=queue', width: 1440, height: 900 },
  { name: 'tasks', path: '/?open=tasks', width: 1440, height: 900 },
  { name: 'schedules', path: '/?open=schedules', width: 1440, height: 900 },
  { name: 'machines', path: '/?machines=1', width: 1440, height: 900 },
  { name: 'computer', path: '/?preview-settings&open=settings:Computer&preset=computer', width: 1440, height: 900 },
  { name: 'mcp', path: '/?preview-settings&open=settings:Connections&preset=mcp', width: 1440, height: 900 },
  { name: 'providers', path: '/?preview-settings&open=settings:Providers', width: 1440, height: 900 },
  { name: 'companion', path: '/?companion&mode=home&view=chat', width: 760, height: 420 },
  { name: 'companion-home', path: '/?companion&mode=home', width: 760, height: 300 },
  { name: 'compare', path: '/?open=compare', width: 1440, height: 900 },
  { name: 'review', path: '/?open=changes&session=session-codex&agent=agent-codex', width: 1440, height: 900 },
]

const listArg = process.argv.indexOf('--list')
const shots = listArg > 0 ? JSON.parse(readFileSync(process.argv[listArg + 1], 'utf8')) : SHOTS

const browser = await chromium.launch({ channel: 'msedge', headless: true })
try {
  for (const shot of shots) {
    const out = shot.out ? join(root, shot.out) : join(root, 'public', 'shots')
    mkdirSync(out, { recursive: true })
    for (const theme of ['dark', 'light']) {
      const style = theme === 'dark' ? 'haze' : 'stone'
      const page = await browser.newPage({ viewport: { width: shot.width ?? 1440, height: shot.height ?? 900 }, deviceScaleFactor: 2, colorScheme: theme })
      const glue = shot.path.includes('?') ? '&' : '?'
      await page.goto(`${BASE}${shot.path}${glue}style=${style}&theme=${theme}&demo&clean`, { waitUntil: 'networkidle', timeout: 60000 })
      await page.waitForTimeout(shot.wait ?? 2500)
      // Close the one-minute tour if it shows.
      const notNow = page.getByRole('button', { name: 'Not now' })
      if (await notNow.count()) { await notNow.first().click().catch(() => {}); await page.waitForTimeout(400) }
      for (const label of shot.click ?? []) {
        const target = page.getByRole('button', { name: new RegExp('^' + label) }).or(page.getByRole('tab', { name: new RegExp('^' + label) }))
        if (await target.count()) { await target.first().click().catch(() => {}); await page.waitForTimeout(1200) }
      }
      const png = await page.screenshot({ clip: shot.clip, type: 'png' })
      const file = join(out, `${shot.name}-${theme}.webp`)
      await sharp(png).webp({ quality: 88 }).toFile(file)
      console.log(file.replace(root, '.'))
      await page.close()
    }
  }
} finally {
  await browser.close()
}
