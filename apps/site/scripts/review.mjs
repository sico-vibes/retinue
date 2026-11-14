// Whole-page review shots of the site itself, in both themes, cut into screen-sized pieces.
// The dev server must be running (npm run dev). node scripts/review.mjs [path] [width]
import { mkdirSync } from 'node:fs'
import { chromium } from 'playwright-core'
import sharp from 'sharp'

// Pass the page without a leading slash ("team/"): Git Bash turns "/team" into a Windows path.
const path = '/' + (process.argv[2] ?? '').replace(/^[a-zA-Z]:.*Git\//, '').replace(/^\/+/, '')
const width = Number(process.argv[3] ?? 1440)
const out = 'C:/rtmp/review'
mkdirSync(out, { recursive: true })
const browser = await chromium.launch({ channel: 'msedge', headless: true })
try {
  for (const theme of ['dark', 'light']) {
    const page = await browser.newPage({ viewport: { width, height: 900 }, colorScheme: theme })
    await page.addInitScript((value) => localStorage.setItem('retinue-theme', value), theme)
    await page.goto(`http://localhost:5300${path}`, { waitUntil: 'networkidle', timeout: 90000 })
    // Walk down the page so every reveal and lazy image runs, then back to the top.
    const height = await page.evaluate(() => document.documentElement.scrollHeight)
    for (let y = 0; y < height; y += 600) { await page.mouse.wheel(0, 600); await page.waitForTimeout(250) }
    await page.waitForTimeout(1500)
    const png = await page.screenshot({ fullPage: true })
    const meta = await sharp(png).metadata()
    const step = Math.round(width * 1.1)
    let index = 0
    for (let top = 0; top < meta.height; top += step) {
      const slice = Math.min(step, meta.height - top)
      await sharp(png).extract({ left: 0, top, width: meta.width, height: slice }).resize(Math.round(width / 1.8)).jpeg({ quality: 70 }).toFile(`${out}/${theme}-${String(index).padStart(2, '0')}.jpg`)
      index += 1
    }
    console.log(`${theme}: ${meta.height}px, ${index} pieces`)
    await page.close()
  }
} finally { await browser.close() }
