// The employee's ID badge on its lanyard, as on the website (owner, 15 November 2026): the left of the Look
// editor shows it, and the badge redraws as the photo, name, role or colour changes. The face is drawn here
// with a 2D canvas to the same design as apps/site/scripts/badges.mjs; the lanyard is React Bits' (three.js),
// loaded only when this panel opens.
import { lazy, Suspense, useEffect, useState } from 'react'
import { canPaintMetal } from './MetallicMark'
import markWhite from '../assets/brand/retinue-mark-white.png'

const Lanyard = lazy(() => import('./reactbits/Lanyard'))

const W = 640
const H = 900
const FONT = '"Segoe UI", Inter, Arial, sans-serif'
const MONO = 'Consolas, "Cascadia Mono", monospace'

function loadImage(src: string) {
  return new Promise<HTMLImageElement | null>((resolve) => {
    const image = new Image()
    image.onload = () => resolve(image)
    image.onerror = () => resolve(null)
    image.src = src
  })
}

/** Draws the badge face and returns it as a PNG data URL (exported for the test). */
export async function drawBadgeFace({ photoUrl, name, role, colorHex }: { photoUrl: string | null; name: string; role: string; colorHex: string }): Promise<string | null> {
  const canvas = document.createElement('canvas')
  canvas.width = W
  canvas.height = H
  const ctx = canvas.getContext('2d')
  if (!ctx) return null
  const bg = ctx.createLinearGradient(0, 0, W, H)
  bg.addColorStop(0, '#16161a')
  bg.addColorStop(1, '#0b0b0c')
  ctx.fillStyle = bg
  ctx.fillRect(0, 0, W, H)
  const tint = ctx.createLinearGradient(0, 0, 0, 520)
  tint.addColorStop(0, hexAlpha(colorHex, .55))
  tint.addColorStop(1, hexAlpha(colorHex, 0))
  ctx.fillStyle = tint
  ctx.fillRect(0, 0, W, 520)
  // The slot the clip goes through.
  ctx.fillStyle = 'rgba(0,0,0,.55)'
  roundRect(ctx, W / 2 - 46, 34, 92, 20, 10)
  ctx.fill()
  ctx.fillStyle = '#f4f4f5'
  ctx.font = `700 26px ${FONT}`
  ctx.textBaseline = 'alphabetic'
  ctx.textAlign = 'left'
  drawSpaced(ctx, 'RETINUE', 40, 118, 6)
  // The photo in a ring of the employee's colour.
  ctx.beginPath()
  ctx.arc(W / 2, 330, 178, 0, Math.PI * 2)
  ctx.strokeStyle = colorHex
  ctx.lineWidth = 6
  ctx.stroke()
  ctx.save()
  ctx.beginPath()
  ctx.arc(W / 2, 330, 170, 0, Math.PI * 2)
  ctx.clip()
  const photo = photoUrl ? await loadImage(photoUrl) : null
  if (photo) ctx.drawImage(photo, W / 2 - 170, 160, 340, 340)
  else {
    ctx.fillStyle = colorHex
    ctx.fillRect(W / 2 - 170, 160, 340, 340)
    ctx.fillStyle = 'rgba(255,255,255,.92)'
    ctx.font = `800 150px ${FONT}`
    ctx.textAlign = 'center'
    ctx.textBaseline = 'middle'
    ctx.fillText((name.trim()[0] ?? '?').toUpperCase(), W / 2, 338)
  }
  ctx.restore()
  ctx.textAlign = 'center'
  ctx.textBaseline = 'alphabetic'
  ctx.fillStyle = '#f4f4f5'
  ctx.font = `800 ${fitSize(ctx, name.toUpperCase(), 78, W - 80)}px ${FONT}`
  ctx.fillText(name.toUpperCase(), W / 2, 640)
  ctx.fillStyle = '#c8c8cf'
  ctx.font = `400 32px ${FONT}`
  ctx.fillText(role.trim() || 'Employee', W / 2, 700, W - 80)
  ctx.fillStyle = 'rgba(244,244,245,.15)'
  ctx.fillRect(40, 770, W - 80, 1)
  ctx.fillStyle = 'rgba(244,244,245,.6)'
  ctx.font = `400 22px ${MONO}`
  ctx.textAlign = 'left'
  ctx.fillText('RETINUE EMPLOYEE', 40, 830)
  ctx.beginPath()
  ctx.arc(W - 56, 823, 9, 0, Math.PI * 2)
  ctx.fillStyle = colorHex
  ctx.fill()
  return canvas.toDataURL('image/png')
}

/**
 * The back of the badge (owner, 15 November 2026: "a premium lanyard, as if it's their company access"): no
 * photo, just the company. The Retinue mark at the centre, the name set wide beneath it, a hairline in the
 * employee's colour and a quiet access line at the foot.
 */
export async function drawBadgeBack({ name, colorHex }: { name: string; colorHex: string }): Promise<string | null> {
  const canvas = document.createElement('canvas')
  canvas.width = W
  canvas.height = H
  const ctx = canvas.getContext('2d')
  if (!ctx) return null
  const bg = ctx.createLinearGradient(0, 0, 0, H)
  bg.addColorStop(0, '#141417')
  bg.addColorStop(1, '#09090a')
  ctx.fillStyle = bg
  ctx.fillRect(0, 0, W, H)
  // A faint fine weave, like a printed security ground.
  ctx.strokeStyle = 'rgba(255,255,255,.025)'
  ctx.lineWidth = 1
  for (let x = -H; x < W; x += 14) { ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x + H, H); ctx.stroke() }
  // A soft pool of the employee's colour behind the mark.
  const glow = ctx.createRadialGradient(W / 2, 370, 10, W / 2, 370, 300)
  glow.addColorStop(0, hexAlpha(colorHex, .22))
  glow.addColorStop(1, hexAlpha(colorHex, 0))
  ctx.fillStyle = glow
  ctx.fillRect(0, 0, W, H)
  ctx.fillStyle = 'rgba(0,0,0,.55)'
  roundRect(ctx, W / 2 - 46, 34, 92, 20, 10)
  ctx.fill()
  const mark = await loadImage(markWhite)
  if (mark) {
    const size = 190
    const ratio = mark.naturalWidth && mark.naturalHeight ? mark.naturalWidth / mark.naturalHeight : 1
    const w = ratio >= 1 ? size : size * ratio
    const h = ratio >= 1 ? size / ratio : size
    ctx.drawImage(mark, W / 2 - w / 2, 370 - h / 2, w, h)
  }
  ctx.fillStyle = '#f4f4f5'
  ctx.font = `600 46px ${FONT}`
  ctx.textBaseline = 'alphabetic'
  const word = 'RETINUE'
  const spacing = 18
  const width = [...word].reduce((sum, letter) => sum + ctx.measureText(letter).width, 0) + spacing * (word.length - 1)
  ctx.textAlign = 'left'
  drawSpaced(ctx, word, W / 2 - width / 2, 560, spacing)
  ctx.fillStyle = colorHex
  ctx.fillRect(W / 2 - 28, 596, 56, 3)
  ctx.textAlign = 'center'
  ctx.fillStyle = 'rgba(244,244,245,.55)'
  ctx.font = `400 22px ${MONO}`
  drawCentredSpaced(ctx, 'AUTHORISED ACCESS', W / 2, 650, 4)
  ctx.fillStyle = 'rgba(244,244,245,.15)'
  ctx.fillRect(40, 770, W - 80, 1)
  ctx.fillStyle = 'rgba(244,244,245,.6)'
  ctx.font = `400 22px ${MONO}`
  ctx.textAlign = 'left'
  ctx.fillText(name.trim().toUpperCase().slice(0, 24) || 'EMPLOYEE', 40, 830)
  ctx.textAlign = 'right'
  ctx.fillText('IF FOUND, RETURN TO RETINUE', W - 40, 830, 300)
  return canvas.toDataURL('image/png')
}

function drawCentredSpaced(ctx: CanvasRenderingContext2D, text: string, centre: number, y: number, spacing: number) {
  const align = ctx.textAlign
  ctx.textAlign = 'left'
  const width = [...text].reduce((sum, letter) => sum + ctx.measureText(letter).width, 0) + spacing * (text.length - 1)
  drawSpaced(ctx, text, centre - width / 2, y, spacing)
  ctx.textAlign = align
}

export function EmployeeBadge({ photoUrl, name, role, colorHex }: { photoUrl: string | null; name: string; role: string; colorHex: string }) {
  const [face, setFace] = useState<string | null>(null)
  const [back, setBack] = useState<string | null>(null)
  useEffect(() => {
    let live = true
    void drawBadgeFace({ photoUrl, name, role, colorHex }).then((url) => { if (live) setFace(url) })
    return () => { live = false }
  }, [photoUrl, name, role, colorHex])
  useEffect(() => {
    let live = true
    void drawBadgeBack({ name, colorHex }).then((url) => { if (live) setBack(url) })
    return () => { live = false }
  }, [name, colorHex])
  // Without WebGL 2, or with reduced motion, the badge hangs still as a picture.
  const able = canPaintMetal()
  return <div className="employee-badge" aria-label={`${name}'s badge`} role="img">
    {face && (able
      ? <Suspense fallback={<img className="employee-badge-still" src={face} alt="" />}><Lanyard frontImage={face} backImage={back ?? undefined} cardColor="#0b0b0c" strapColor={colorHex} finish="glossy" strapLength={0.5} size={0.56} intro={false} /></Suspense>
      : <img className="employee-badge-still" src={face} alt="" />)}
  </div>
}

function hexAlpha(hex: string, alpha: number) {
  const value = /^#?([0-9a-f]{6})$/i.exec(hex.trim())?.[1]
  if (!value) return `rgba(139,92,246,${alpha})`
  const n = parseInt(value, 16)
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha})`
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath()
  ctx.moveTo(x + r, y)
  ctx.arcTo(x + w, y, x + w, y + h, r)
  ctx.arcTo(x + w, y + h, x, y + h, r)
  ctx.arcTo(x, y + h, x, y, r)
  ctx.arcTo(x, y, x + w, y, r)
  ctx.closePath()
}

function drawSpaced(ctx: CanvasRenderingContext2D, text: string, x: number, y: number, spacing: number) {
  let left = x
  for (const letter of text) { ctx.fillText(letter, left, y); left += ctx.measureText(letter).width + spacing }
}

function fitSize(ctx: CanvasRenderingContext2D, text: string, start: number, width: number) {
  let size = start
  for (; size > 32; size -= 4) { ctx.font = `800 ${size}px ${FONT}`; if (ctx.measureText(text).width <= width) break }
  return size
}
