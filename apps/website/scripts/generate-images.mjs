// Pre-sizes every raster image the site uses into public/, as WebP.
//
//   npm run images -- --assets "D:/path/to/repo/assets" [--shots "D:/folder/of/shots"]
//
// --assets holds website/hero.png, website/footer.png, profile/*.png and icon/retinue-*.png.
// --shots (optional) holds full-size product shots, named <name>-dark and <name>-light.
import { readdir, mkdir, copyFile } from 'node:fs/promises'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve, parse } from 'node:path'
import { fileURLToPath } from 'node:url'
import sharp from 'sharp'

const here = dirname(fileURLToPath(import.meta.url))
const site = resolve(here, '..')
const flag = (name) => { const index = process.argv.indexOf(`--${name}`); return index > -1 ? process.argv[index + 1] : undefined }
const assets = resolve(flag('assets') ?? resolve(site, '../../assets'))
const shots = flag('shots') && resolve(flag('shots'))

async function sizes(input, folder, name, widths, options = {}) {
  const out = join(site, 'public', folder)
  await mkdir(out, { recursive: true })
  for (const width of widths) {
    await sharp(input).resize({ width, withoutEnlargement: true }).webp({ quality: options.quality ?? 76, effort: 6 }).toFile(join(out, `${name}-${width}.webp`))
  }
  console.log(`${folder}/${name}: ${widths.join(', ')}`)
}

const ART = [640, 960, 1280, 1920]
await sizes(join(assets, 'website/hero.png'), 'art', 'hero', ART, { quality: 62 })
// Phones get their own 4:3 crop of the hero.
await sharp(join(assets, 'website/hero.png')).extract({ left: 215, top: 0, width: 1092, height: 819 }).resize({ width: 800 }).webp({ quality: 62, effort: 6 }).toFile(join(site, 'public/art/hero-mobile.webp'))
await sizes(join(assets, 'website/footer.png'), 'art', 'footer', ART, { quality: 62 })

// The source renders carry no names. photo-map.json says which render is which employee.
const photoMap = JSON.parse(readFileSync(join(here, 'photo-map.json'), 'utf8'))
for (const file of await readdir(join(assets, 'profile'))) {
  const name = photoMap[parse(file).name.replace(/^exec-/, '').slice(0, 8)]
  if (name) await sizes(join(assets, 'profile', file), 'people', name, [96, 192, 384], { quality: 78 })
  else console.log(`skipped ${file}: not in photo-map.json`)
}

await mkdir(join(site, 'public/brand'), { recursive: true })
for (const name of ['retinue-mark-white', 'retinue-mark-black']) {
  await sharp(join(assets, 'icon', `${name}.png`)).resize(96).webp({ quality: 90 }).toFile(join(site, 'public/brand', `${name}.webp`))
}
for (const name of ['retinue-tile-dark', 'retinue-tile-light']) {
  await sharp(join(assets, 'icon', `${name}.png`)).resize(192).webp({ quality: 90 }).toFile(join(site, 'public/brand', `${name}.webp`))
}
await sharp(join(assets, 'icon/retinue-tile-dark.png')).resize(64).png().toFile(join(site, 'app/icon.png'))
await sharp(join(assets, 'icon/retinue-tile-dark.png')).resize(180).png().toFile(join(site, 'app/apple-icon.png'))

if (shots && existsSync(shots)) {
  for (const file of await readdir(shots)) {
    if (/-(dark|light)\./.test(file)) await sizes(join(shots, file), 'shots', parse(file).name, [640, 960, 1280, 1920], { quality: 78 })
  }
}

// Coding-agent logos are vectors: copied as they are.
const providers = resolve(site, '../desktop/src/assets/providers')
if (existsSync(providers)) {
  await mkdir(join(site, 'public/providers'), { recursive: true })
  for (const file of await readdir(providers)) if (file.endsWith('.svg')) await copyFile(join(providers, file), join(site, 'public/providers', file))
}
