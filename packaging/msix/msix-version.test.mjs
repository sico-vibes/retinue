import assert from 'node:assert/strict'
import test from 'node:test'
import { msixVersion } from './msix-version.mjs'

test('betas count up inside their patch and the final release sorts above them', () => {
  assert.equal(msixVersion('0.1.0-beta.51'), '0.1.51.0')
  assert.equal(msixVersion('0.1.0-beta.998'), '0.1.998.0')
  assert.equal(msixVersion('0.1.0'), '0.1.999.0')
  assert.equal(msixVersion('0.1.1-beta.1'), '0.1.1001.0')
  assert.equal(msixVersion('1.2.3'), '1.2.3999.0')
})

test('versions keep their order', () => {
  const order = ['0.1.0-beta.50', '0.1.0-beta.51', '0.1.0', '0.1.1-beta.1', '0.1.1', '0.2.0-beta.1']
  const mapped = order.map(msixVersion).map((v) => v.split('.').map(Number))
  for (let i = 1; i < mapped.length; i += 1) {
    const [a, b] = [mapped[i - 1], mapped[i]]
    const cmp = a[0] - b[0] || a[1] - b[1] || a[2] - b[2]
    assert.ok(cmp < 0, `${order[i - 1]} should sort below ${order[i]}`)
  }
})

test('rejects what cannot be mapped', () => {
  assert.throws(() => msixVersion('0.1'), /Cannot map/)
  assert.throws(() => msixVersion('0.1.0-beta'), /Cannot map/)
  assert.throws(() => msixVersion('0.1.0-beta.0'), /1 to 998/)
  assert.throws(() => msixVersion('0.1.66-beta.1'), /too large/)
})
