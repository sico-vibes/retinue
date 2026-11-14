// Makes a compressed WebP in public/art for every PNG master in art-source (keeping transparency),
// sized for the site: characters and scenes at most 1100 px tall or 2000 px wide, objects 400 px.
// The masters stay out of git and out of the published site. Runs before every build (prebuild).
import { mkdirSync, readdirSync, statSync } from 'node:fs'
import { join, dirname, relative } from 'node:path'
import { fileURLToPath } from 'node:url'
import sharp from 'sharp'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const art = join(root, 'art-source')
const published = join(root, 'public', 'art')

function walk(dir) {
  try {
    return readdirSync(dir).flatMap((name) => { const path = join(dir, name); return statSync(path).isDirectory() ? walk(path) : [path] })
  } catch { return [] }
}

for (const source of walk(art).filter((path) => path.endsWith('.png'))) {
  const target = join(published, relative(art, source)).replace(/\.png$/, '.webp')
  mkdirSync(dirname(target), { recursive: true })
  try { if (statSync(target).mtimeMs >= statSync(source).mtimeMs) continue } catch { /* not made yet */ }
  const folder = relative(art, dirname(source)).replaceAll('\\', '/')
  const resize = folder === 'objects' ? { width: 400, height: 400, fit: 'inside' } : folder === 'scenes' ? { width: 2000, fit: 'inside' } : { height: 1100, fit: 'inside' }
  await sharp(source).resize({ ...resize, withoutEnlargement: true }).webp({ quality: 84, alphaQuality: 90, effort: 5 }).toFile(target)
  console.log(`${relative(art, target)}  ${Math.round(statSync(source).size / 1024)} KB -> ${Math.round(statSync(target).size / 1024)} KB`)
}
