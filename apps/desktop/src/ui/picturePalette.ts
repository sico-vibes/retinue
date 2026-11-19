// Haze takes its colours from the chat background picture (owner, 16 November 2026): the four
// mist colours come from the parts of the picture they sit over, and the accent is its most vivid
// colour. Pure functions over RGBA pixels, so the picture never leaves the window.

export type PicturePalette = { d1: string; d2: string; d3: string; d4: string; accent: string; base: string }
type Pixels = { width: number; height: number; data: ArrayLike<number> }
type Region = [x0: number, y0: number, x1: number, y1: number]

const HEX = /^#[0-9a-f]{6}$/

// Where each mist blob sits in haze.css: d1 top left, d2 right, d3 along the bottom, d4 the middle.
const REGIONS: Record<'d1' | 'd2' | 'd3' | 'd4', Region> = { d1: [0, 0, .5, .45], d2: [.55, .35, 1, .9], d3: [.2, .65, .8, 1], d4: [.3, .15, .75, .6] }

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255; g /= 255; b /= 255
  const max = Math.max(r, g, b), min = Math.min(r, g, b)
  const l = (max + min) / 2
  if (max === min) return [0, 0, l]
  const d = max - min
  const s = l > .5 ? d / (2 - max - min) : d / (max + min)
  const h = max === r ? ((g - b) / d + (g < b ? 6 : 0)) : max === g ? (b - r) / d + 2 : (r - g) / d + 4
  return [h * 60, s, l]
}

function hslToHex(h: number, s: number, l: number): string {
  const k = (n: number) => (n + h / 30) % 12
  const a = s * Math.min(l, 1 - l)
  const f = (n: number) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)))
  return '#' + [f(0), f(8), f(4)].map((value) => Math.round(Math.max(0, Math.min(1, value)) * 255).toString(16).padStart(2, '0')).join('')
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value))

/** The region's characteristic colour: the strongest hue family, weighted by colourfulness, averaged. */
function dominant(pixels: Pixels, region: Region, vivid: boolean): [number, number, number] {
  const { width, height, data } = pixels
  const bins = Array.from({ length: 13 }, () => ({ weight: 0, r: 0, g: 0, b: 0, n: 0 }))
  const x0 = Math.floor(region[0] * width), x1 = Math.max(x0 + 1, Math.ceil(region[2] * width))
  const y0 = Math.floor(region[1] * height), y1 = Math.max(y0 + 1, Math.ceil(region[3] * height))
  for (let y = y0; y < Math.min(y1, height); y++) for (let x = x0; x < Math.min(x1, width); x++) {
    const i = (y * width + x) * 4
    const r = data[i]!, g = data[i + 1]!, b = data[i + 2]!
    const [h, s, l] = rgbToHsl(r, g, b)
    // Greys go in their own bin; colourful, mid-light pixels count most.
    const bin = s < .12 ? bins[12]! : bins[Math.floor(h / 30) % 12]!
    const presence = 1 - Math.abs(l - .5) * 1.6
    const weight = Math.max(.02, vivid ? s * s * presence : (.25 + s) * presence)
    bin.weight += weight; bin.r += r * weight; bin.g += g * weight; bin.b += b * weight; bin.n += 1
  }
  const best = bins.reduce((top, bin) => bin.weight > top.weight ? bin : top)
  if (!best.weight) return [0, 0, .5]
  return rgbToHsl(best.r / best.weight, best.g / best.weight, best.b / best.weight)
}

/** The picture's palette, tuned so it reads like Haze's own greys: soft mist, a clear accent, a deep base. */
export function picturePalette(pixels: Pixels): PicturePalette {
  const mist = (region: Region) => { const [h, s, l] = dominant(pixels, region, false); return hslToHex(h, clamp(s, 0, .55), clamp(l, .38, .66)) }
  const [ah, as, al] = dominant(pixels, [0, 0, 1, 1], true)
  const [bh, bs] = dominant(pixels, [0, 0, 1, 1], false)
  return {
    d1: mist(REGIONS.d1), d2: mist(REGIONS.d2), d3: mist(REGIONS.d3), d4: mist(REGIONS.d4),
    accent: hslToHex(ah, as < .12 ? as : clamp(as, .5, .85), clamp(al, .52, .66)),
    base: hslToHex(bh, clamp(bs, 0, .3), .1),
  }
}

/** A stored palette, or null when any part is not a `#rrggbb` colour. */
export function normalizePicturePalette(value: unknown): PicturePalette | null {
  if (!value || typeof value !== 'object') return null
  const raw = value as Record<string, unknown>
  const keys = ['d1', 'd2', 'd3', 'd4', 'accent', 'base'] as const
  return keys.every((key) => typeof raw[key] === 'string' && HEX.test(raw[key] as string)) ? Object.fromEntries(keys.map((key) => [key, raw[key]])) as PicturePalette : null
}

/** Reads a canvas small, since a few thousand pixels are plenty for a palette. */
export function paletteFromCanvas(source: HTMLCanvasElement): PicturePalette | null {
  const canvas = document.createElement('canvas')
  canvas.width = 96; canvas.height = Math.max(1, Math.round(96 * source.height / source.width))
  const context = canvas.getContext('2d', { willReadFrequently: true })
  if (!context) return null
  context.drawImage(source, 0, 0, canvas.width, canvas.height)
  try { return picturePalette(context.getImageData(0, 0, canvas.width, canvas.height)) } catch { return null }
}
