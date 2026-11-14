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
}
console.log(`${people.length} badges`)
