// Checks the art in public/art: size, file weight, and whether a PNG's background really is transparent.
// No dependencies (Node's own zlib). Run from apps/site: node scripts/check-art.mjs [--write]
// --write fills width, height and transparent in public/art/manifest.json from the files themselves.
import { readFileSync, readdirSync, statSync, writeFileSync, existsSync } from 'node:fs'
import { join, relative, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { inflateSync } from 'node:zlib'

const art = join(dirname(fileURLToPath(import.meta.url)), '..', 'art-source')

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name)
    return statSync(path).isDirectory() ? walk(path) : [path]
  })
}

/** Width, height, and the share of fully transparent pixels and transparent corners, for an 8-bit RGBA PNG. */
function inspectPng(buffer) {
  const width = buffer.readUInt32BE(16), height = buffer.readUInt32BE(20), depth = buffer[24], type = buffer[25], interlace = buffer[28]
  const result = { width, height, alpha: type === 6 || type === 4, clearShare: null, clearCorners: null }
  if (type !== 6 || depth !== 8 || interlace !== 0) return result
  const chunks = []
  for (let at = 8; at < buffer.length;) {
    const length = buffer.readUInt32BE(at), kind = buffer.toString('ascii', at + 4, at + 8)
    if (kind === 'IDAT') chunks.push(buffer.subarray(at + 8, at + 8 + length))
    at += 12 + length
  }
  const raw = inflateSync(Buffer.concat(chunks))
  const stride = width * 4
  const pixels = Buffer.alloc(stride * height)
  for (let y = 0; y < height; y += 1) {
    const filter = raw[y * (stride + 1)]
    for (let x = 0; x < stride; x += 1) {
      const value = raw[y * (stride + 1) + 1 + x]
      const left = x >= 4 ? pixels[y * stride + x - 4] : 0
      const up = y > 0 ? pixels[(y - 1) * stride + x] : 0
      const corner = x >= 4 && y > 0 ? pixels[(y - 1) * stride + x - 4] : 0
      let add = 0
      if (filter === 1) add = left
      else if (filter === 2) add = up
      else if (filter === 3) add = (left + up) >> 1
      else if (filter === 4) { const p = left + up - corner, pa = Math.abs(p - left), pb = Math.abs(p - up), pc = Math.abs(p - corner); add = pa <= pb && pa <= pc ? left : pb <= pc ? up : corner }
      pixels[y * stride + x] = (value + add) & 255
    }
  }
  let clear = 0
  for (let i = 3; i < pixels.length; i += 4) if (pixels[i] < 8) clear += 1
  const alphaAt = (x, y) => pixels[(y * width + x) * 4 + 3]
  const corners = [alphaAt(0, 0), alphaAt(width - 1, 0), alphaAt(0, height - 1), alphaAt(width - 1, height - 1)]
  return { ...result, clearShare: clear / (width * height), clearCorners: corners.filter((a) => a < 8).length }
}

const files = existsSync(art) ? walk(art).filter((path) => /\.(png|webp|jpe?g)$/i.test(path)) : []
const facts = {}
for (const path of files) {
  const name = relative(art, path).replaceAll('\\', '/')
  const bytes = statSync(path).size
  const buffer = readFileSync(path)
  const isPng = buffer.toString('ascii', 1, 4) === 'PNG'
  const info = isPng ? inspectPng(buffer) : { width: null, height: null, alpha: false, clearShare: null, clearCorners: null }
  const transparent = info.clearCorners === 4 && (info.clearShare ?? 0) > 0.05
  facts[name] = { width: info.width, height: info.height, transparent }
  const weight = `${(bytes / 1024 / 1024).toFixed(2)} MB${bytes > 1.5 * 1024 * 1024 ? ' (over 1.5 MB)' : ''}`
  const clear = info.clearShare === null ? 'n/a' : `${Math.round(info.clearShare * 100)}% clear, ${info.clearCorners}/4 corners clear`
  console.log(`${transparent ? 'TRANSPARENT' : isPng ? 'OPAQUE     ' : 'IMAGE      '}  ${name}  ${info.width ?? '?'}x${info.height ?? '?'}  ${weight}  ${clear}`)
}

if (process.argv.includes('--write')) {
  const manifestPath = join(art, 'manifest.json')
  const text = existsSync(manifestPath) ? readFileSync(manifestPath, 'utf8').replace(/^﻿/, '') : '[]'
  let rows = JSON.parse(text)
  if (!Array.isArray(rows)) rows = [rows]
  for (const row of rows) Object.assign(row, facts[row.file] ?? {})
  for (const name of Object.keys(facts)) if (!rows.some((row) => row.file === name)) rows.push({ file: name, ...facts[name], who: null, description: '' })
  writeFileSync(manifestPath, JSON.stringify(rows, null, 2) + '\n')
  console.log(`manifest.json: ${rows.length} entries`)
}
