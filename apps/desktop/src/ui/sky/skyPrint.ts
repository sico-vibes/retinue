// Retinue's own sky (owner, 23 November 2026: "Zephium has a background picture, we need the picture back").
// Zephium's empty page is a photograph of a sky printed with a dot screen. Their photographs carry no licence, so
// Retinue paints its own: a seeded value-noise sky, cloud banks that rise toward the sides like a bowl, and an
// ordered (Bayer) dither that gives the printed dots. Everything is computed here; no picture ships.

export type SkyScene = 'night' | 'day'

type Rgb = [number, number, number]

const PALETTES: Record<SkyScene, { top: Rgb; mid: Rgb; horizon: Rgb; foot: Rgb; lit: Rgb; shade: Rgb; wisp: Rgb }> = {
  night: { top: [11, 27, 52], mid: [26, 50, 88], horizon: [58, 84, 124], foot: [14, 32, 62], lit: [214, 224, 240], shade: [52, 74, 112], wisp: [150, 172, 206] },
  day: { top: [62, 118, 196], mid: [104, 156, 224], horizon: [178, 208, 240], foot: [120, 166, 222], lit: [255, 255, 255], shade: [168, 188, 216], wisp: [236, 242, 252] },
}

/** The dot cell, in CSS pixels. */
export const SKY_CELL = 2

const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5].map((value) => (value + 0.5) / 16 - 0.5)

function hash(x: number, y: number, seed: number) {
  let h = (x * 374761393 + y * 668265263 + seed * 2147483647) | 0
  h = Math.imul(h ^ (h >>> 13), 1274126177)
  return ((h ^ (h >>> 16)) >>> 0) / 4294967295
}

function valueNoise(x: number, y: number, seed: number) {
  const xi = Math.floor(x), yi = Math.floor(y)
  const xf = x - xi, yf = y - yi
  const u = xf * xf * (3 - 2 * xf), v = yf * yf * (3 - 2 * yf)
  const a = hash(xi, yi, seed), b = hash(xi + 1, yi, seed), c = hash(xi, yi + 1, seed), d = hash(xi + 1, yi + 1, seed)
  return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

function fbm(x: number, y: number, seed: number, octaves = 5) {
  let sum = 0, amp = 0.5, freq = 1, norm = 0
  for (let i = 0; i < octaves; i += 1) {
    sum += amp * valueNoise(x * freq, y * freq, seed + i * 17)
    norm += amp
    amp *= 0.5
    freq *= 2.03
  }
  return sum / norm
}

const mix = (a: number, b: number, t: number) => a + (b - a) * t
const smooth = (edge0: number, edge1: number, x: number) => {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)))
  return t * t * (3 - 2 * t)
}
const mixRgb = (a: Rgb, b: Rgb, t: number): Rgb => [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)]

function skyGround(palette: (typeof PALETTES)[SkyScene], v: number): Rgb {
  if (v < 0.42) return mixRgb(palette.top, palette.mid, smooth(0, 0.42, v))
  if (v < 0.72) return mixRgb(palette.mid, palette.horizon, smooth(0.42, 0.72, v))
  return mixRgb(palette.horizon, palette.foot, smooth(0.72, 1, v))
}

/** The sky's colour field, one value per dot cell. Exported for the test. */
export function skyField(columns: number, rows: number, scene: SkyScene, seed = 7): Float32Array {
  const palette = PALETTES[scene]
  const out = new Float32Array(columns * rows * 3)
  const aspect = columns / Math.max(1, rows)
  for (let row = 0; row < rows; row += 1) {
    const v = (row + 0.5) / rows
    for (let column = 0; column < columns; column += 1) {
      const u = (column + 0.5) / columns
      const side = 2 * u - 1
      // The bank's crest rises toward both sides and dips in the middle, so the name has open sky behind it.
      const crest = 0.64 - 0.3 * side * side
      const x = u * aspect * 1.7, y = v * 2.8
      const warp = fbm(x * 0.5 + 3.1, y * 0.5 - 1.7, seed + 101, 3) - 0.5
      const body = fbm(x + warp * 1.2, y * 1.4 + warp * 0.8, seed, 6)
      const bank = smooth(crest - 0.08, crest + 0.14, v) * (1 - 0.55 * smooth(0.9, 1.04, v))
      const coverage = bank * (0.62 + 0.3 * Math.abs(side))
      const detail = fbm(x * 4.2 + warp, y * 4.6, seed + 29, 4) - 0.5
      const density = smooth(1.05 - coverage, 1.13 - coverage, body + 0.35 + detail * 0.16) * smooth(0, 0.25, bank)
      // Light comes from above: the top of a cloud is lit, its underside in shade.
      const above = fbm(x + warp * 1.2, (y - 0.08) * 1.4 + warp * 0.8, seed, 6)
      const lit = smooth(-0.05, 0.08, body - above + 0.015)
      // Thin wisps across the upper sky, stretched along the wind.
      const streak = fbm(x * 0.32 + 9, y * 5.5 + warp * 2, seed + 53, 5)
      const wisp = smooth(0.6, 0.78, streak) * (1 - smooth(crest - 0.3, crest, v)) * smooth(0.16, 0.34, v) * 0.2
      let color = skyGround(palette, v)
      color = mixRgb(color, palette.wisp, wisp)
      const cloud = mixRgb(palette.shade, palette.lit, Math.min(1, Math.max(0, 0.18 + lit * 0.6 + detail * 0.9)))
      color = mixRgb(color, cloud, density * 0.95)
      const offset = (row * columns + column) * 3
      out[offset] = color[0]; out[offset + 1] = color[1]; out[offset + 2] = color[2]
    }
  }
  return out
}

/** A print takes about 200 ms, so the last few are kept: returning to an empty conversation reuses one. */
const PRINTS = new Map<string, ImageData>()

/** Prints the sky onto a canvas the given CSS size, one dither dot per SKY_CELL. */
export function printSky(canvas: HTMLCanvasElement, width: number, height: number, scene: SkyScene, seed = 7) {
  const columns = Math.max(1, Math.ceil(width / SKY_CELL))
  const rows = Math.max(1, Math.ceil(height / SKY_CELL))
  canvas.width = columns
  canvas.height = rows
  canvas.style.width = `${columns * SKY_CELL}px`
  canvas.style.height = `${rows * SKY_CELL}px`
  const context = canvas.getContext('2d')
  if (!context) return false
  const key = `${columns}x${rows}:${scene}:${seed}`
  const cached = PRINTS.get(key)
  if (cached) { context.putImageData(cached, 0, 0); return true }
  const field = skyField(columns, rows, scene, seed)
  const image = context.createImageData(columns, rows)
  const levels = 17
  const step = 255 / (levels - 1)
  for (let row = 0; row < rows; row += 1) {
    for (let column = 0; column < columns; column += 1) {
      const threshold = BAYER[(row & 3) * 4 + (column & 3)]
      const at = (row * columns + column) * 3
      const pixel = (row * columns + column) * 4
      for (let channel = 0; channel < 3; channel += 1) {
        const value = field[at + channel] / step + threshold
        image.data[pixel + channel] = Math.max(0, Math.min(255, Math.round(value) * step))
      }
      image.data[pixel + 3] = 255
    }
  }
  context.putImageData(image, 0, 0)
  PRINTS.set(key, image)
  if (PRINTS.size > 4) PRINTS.delete(PRINTS.keys().next().value!)
  return true
}

/** The printed sky's colour down its edge at `column` (in dots), in `bands` steps: the light it throws on the columns
 * beside it, as Zephium's sky lights the sidebar. */
export function skyEdgeBands(canvas: HTMLCanvasElement, column: number, bands = 16): Array<[number, number, number]> {
  const context = canvas.getContext('2d')
  if (!context || !canvas.width || !canvas.height) return []
  const x = Math.max(0, Math.min(canvas.width - 4, Math.round(column)))
  const data = context.getImageData(x, 0, 4, canvas.height).data
  const out: Array<[number, number, number]> = []
  for (let band = 0; band < bands; band += 1) {
    const from = Math.floor((band * canvas.height) / bands), to = Math.max(from + 1, Math.floor(((band + 1) * canvas.height) / bands))
    let r = 0, g = 0, b = 0, n = 0
    for (let row = from; row < to; row += 1) for (let dx = 0; dx < 4; dx += 1) { const at = (row * 4 + dx) * 4; r += data[at]!; g += data[at + 1]!; b += data[at + 2]!; n += 1 }
    out.push([Math.round(r / n), Math.round(g / n), Math.round(b / n)])
  }
  return out
}
