// Draws an ID badge for every employee in content/roster.json (photo, name, role, department, colour)
// into public/badges/<id>.png, for the Lanyard component. Run after roster.mjs: node scripts/badges.mjs
import { readFileSync, mkdirSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import sharp from 'sharp'

const site = join(dirname(fileURLToPath(import.meta.url)), '..')
const { departments, people } = JSON.parse(readFileSync(join(site, 'content', 'roster.json'), 'utf8'))
const out = join(site, 'public', 'badges')
mkdirSync(out, { recursive: true })

const W = 640, H = 900
const escape = (text) => String(text).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')

for (const [index, person] of people.entries()) {
  const photo = await sharp(join(site, 'public', person.photo.slice(1))).resize(360, 360).png().toBuffer()
  const department = departments.find((item) => item.id === person.department)?.name ?? ''
  const code = `RTN-${String(index + 1).padStart(3, '0')}`
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#16161a"/><stop offset="1" stop-color="#0b0b0c"/></linearGradient>
    <linearGradient id="tint" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${person.color}" stop-opacity="0.55"/><stop offset="1" stop-color="${person.color}" stop-opacity="0"/></linearGradient>
    <clipPath id="round"><circle cx="${W / 2}" cy="330" r="170"/></clipPath>
  </defs>
  <rect width="${W}" height="${H}" fill="url(#bg)"/>
  <rect width="${W}" height="520" fill="url(#tint)"/>
  <rect x="${W / 2 - 46}" y="34" width="92" height="20" rx="10" fill="#000" fill-opacity="0.55"/>
  <text x="40" y="118" font-family="Segoe UI, Arial, sans-serif" font-size="26" font-weight="700" letter-spacing="6" fill="#f4f4f5">RETINUE</text>
  <text x="${W - 40}" y="118" text-anchor="end" font-family="Consolas, monospace" font-size="22" fill="#f4f4f5" fill-opacity="0.7">${code}</text>
  <circle cx="${W / 2}" cy="330" r="178" fill="none" stroke="${person.color}" stroke-width="6"/>
  <image x="${W / 2 - 170}" y="160" width="340" height="340" clip-path="url(#round)" xlink:href="data:image/png;base64,${photo.toString('base64')}"/>
  <text x="${W / 2}" y="640" text-anchor="middle" font-family="Segoe UI, Arial, sans-serif" font-size="78" font-weight="800" fill="#f4f4f5">${escape(person.name.toUpperCase())}</text>
  <text x="${W / 2}" y="700" text-anchor="middle" font-family="Segoe UI, Arial, sans-serif" font-size="32" fill="#c8c8cf">${escape(person.role)}</text>
  <rect x="40" y="770" width="${W - 80}" height="1" fill="#f4f4f5" fill-opacity="0.15"/>
  <text x="40" y="830" font-family="Consolas, monospace" font-size="22" fill="#f4f4f5" fill-opacity="0.6">${escape(department.toUpperCase())}</text>
  <circle cx="${W - 56}" cy="823" r="9" fill="${person.color}"/>
</svg>`
  await sharp(Buffer.from(svg)).png().toFile(join(out, `${person.id}.png`))

  // The back of the badge: what they do, their default personality and their skills.
  const wrap = (text, width) => text.split(' ').reduce((lines, word) => {
    const last = lines[lines.length - 1]
    if (last && (last + ' ' + word).length <= width) lines[lines.length - 1] = last + ' ' + word
    else lines.push(word)
    return lines
  }, [])
  const label = (id) => (id.charAt(0).toUpperCase() + id.slice(1)).replace(/-/g, ' ').replace(/\btdd\b/i, 'TDD').replace(/\bapi\b/i, 'API')
  const about = wrap(person.description, 30).slice(0, 4)
  const voice = person.personality ? wrap(person.personality.summary, 34).slice(0, 3) : []
  const chips = person.skills.slice(0, 6).map(label)
  let y = 0
  const lines = (rows, x, startY, size, color, weight = 400, gap = 1.35) => rows.map((row, index) => `<text x="${x}" y="${startY + index * size * gap}" font-family="Segoe UI, Arial, sans-serif" font-size="${size}" font-weight="${weight}" fill="${color}">${escape(row)}</text>`).join('')
  y = 250
  const aboutSvg = lines(about, 40, y, 30, '#e7e7ea')
  y += about.length * 30 * 1.35 + 40
  const voiceTitle = person.personality ? `<text x="40" y="${y}" font-family="Consolas, monospace" font-size="20" fill="${person.color}">PERSONALITY · ${escape(person.personality.name.toUpperCase())}</text>` : ''
  const voiceSvg = lines(voice, 40, y + 40, 25, '#b9b9c2')
  y += 40 + voice.length * 25 * 1.35 + 36
  const skillTitle = `<text x="40" y="${y}" font-family="Consolas, monospace" font-size="20" fill="${person.color}">SKILLS</text>`
  let chipX = 40, chipY = y + 22
  const chipSvg = chips.map((chip) => {
    const w = Math.round(chip.length * 12.5 + 28)
    if (chipX + w > W - 40) { chipX = 40; chipY += 52 }
    const part = `<rect x="${chipX}" y="${chipY}" width="${w}" height="40" rx="20" fill="#ffffff" fill-opacity="0.07" stroke="#ffffff" stroke-opacity="0.16"/><text x="${chipX + w / 2}" y="${chipY + 27}" text-anchor="middle" font-family="Segoe UI, Arial, sans-serif" font-size="21" fill="#e7e7ea">${escape(chip)}</text>`
    chipX += w + 10
    return part
  }).join('')
  const back = `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">
  <defs><linearGradient id="bb" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${person.color}" stop-opacity="0.35"/><stop offset="0.35" stop-color="#121216"/><stop offset="1" stop-color="#0b0b0c"/></linearGradient></defs>
  <rect width="${W}" height="${H}" fill="url(#bb)"/>
  <rect x="${W / 2 - 46}" y="34" width="92" height="20" rx="10" fill="#000" fill-opacity="0.55"/>
  <text x="40" y="132" font-family="Segoe UI, Arial, sans-serif" font-size="54" font-weight="800" fill="#f4f4f5">${escape(person.name.toUpperCase())}</text>
  <text x="40" y="178" font-family="Segoe UI, Arial, sans-serif" font-size="28" fill="#c8c8cf">${escape(person.role)}</text>
  <rect x="40" y="206" width="${W - 80}" height="1" fill="#f4f4f5" fill-opacity="0.15"/>
  ${aboutSvg}${voiceTitle}${voiceSvg}${skillTitle}${chipSvg}
  <text x="40" y="${H - 44}" font-family="Consolas, monospace" font-size="20" fill="#f4f4f5" fill-opacity="0.5">TAP TO TURN</text>
  <circle cx="${W - 56}" cy="${H - 51}" r="9" fill="${person.color}"/>
</svg>`
  await sharp(Buffer.from(back)).png().toFile(join(out, `${person.id}-back.png`))
}
console.log(`${people.length} badges`)
