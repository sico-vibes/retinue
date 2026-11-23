import { describe, expect, it } from 'vitest'
import { skyField } from './skyPrint'

const brightness = (field: Float32Array, columns: number, column: number, row: number) => {
  const at = (row * columns + column) * 3
  return (field[at]! + field[at + 1]! + field[at + 2]!) / 3
}

/** Mean brightness of a rectangle given in fractions of the sky. */
function region(field: Float32Array, columns: number, rows: number, u0: number, u1: number, v0: number, v1: number) {
  let sum = 0, count = 0
  for (let row = Math.floor(v0 * rows); row < Math.floor(v1 * rows); row += 1) {
    for (let column = Math.floor(u0 * columns); column < Math.floor(u1 * columns); column += 1) { sum += brightness(field, columns, column, row); count += 1 }
  }
  return sum / count
}

describe('Retinue sky', () => {
  it('is the same sky every time for the same size and seed', () => {
    expect(skyField(60, 40, 'night')).toEqual(skyField(60, 40, 'night'))
    expect(skyField(60, 40, 'night', 8)).not.toEqual(skyField(60, 40, 'night'))
  })

  it('keeps open sky behind the middle and cloud banks low and at the sides', () => {
    const columns = 240, rows = 150
    const field = skyField(columns, rows, 'night')
    const openMiddle = region(field, columns, rows, 0.4, 0.6, 0.2, 0.4)
    const lowBank = region(field, columns, rows, 0, 1, 0.72, 0.9)
    const sides = region(field, columns, rows, 0, 0.12, 0.45, 0.7)
    expect(lowBank).toBeGreaterThan(openMiddle + 20)
    expect(sides).toBeGreaterThan(openMiddle + 20)
  })

  it('paints a lighter day than night', () => {
    const night = skyField(80, 50, 'night'), day = skyField(80, 50, 'day')
    expect(region(day, 80, 50, 0, 1, 0, 1)).toBeGreaterThan(region(night, 80, 50, 0, 1, 0, 1) + 60)
  })
})
