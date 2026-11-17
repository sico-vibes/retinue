import { describe, expect, it } from 'vitest'
import { cardNumberProblem, originProblem, vaultSubtitle, type VaultItem } from './VaultPage'
import { channelStatusLine, type Channel } from './ChannelsPage'

// Owner request, 16 November 2026: a Vault page and chat apps to the CEO (docs/plans/vault-channels-pi.md).
const base: VaultItem = { id: 'v', kind: 'login', title: 'GitHub', origin: null, username: null, holder: null, brand: null, last4: null, expiry: null, service: null, hasTotp: false, createdAt: '', updatedAt: '', lastUsedAt: null, useCount: 0 }

describe('vault helpers', () => {
  it('describes an item without anything secret', () => {
    expect(vaultSubtitle({ ...base, username: 'me@example.com', origin: 'https://github.com' })).toBe('me@example.com · github.com')
    expect(vaultSubtitle({ ...base, kind: 'card', brand: 'Visa', last4: '4242', expiry: '08/28' })).toBe('Visa •••• 4242 · expires 08/28')
    expect(vaultSubtitle({ ...base, kind: 'apiKey', service: 'OpenAI' })).toBe('OpenAI')
    expect(vaultSubtitle({ ...base, kind: 'note' })).toBe('Note')
  })

  it('checks card numbers with Luhn and sites as https origins', () => {
    expect(cardNumberProblem('4242 4242 4242 4242')).toBeNull()
    expect(cardNumberProblem('4242 4242 4242 4241')).toMatch(/doesn’t look right/)
    expect(cardNumberProblem('1234')).toMatch(/12 to 19 digits/)
    expect(originProblem('github.com')).toBeNull()
    expect(originProblem('https://github.com/login')).toBeNull()
    expect(originProblem('http://github.com')).toMatch(/https/)
  })
})

describe('chat app status', () => {
  const off: Channel = { kind: 'telegram', status: 'off', detail: null, account: null, peer: null, approvals: true, sessionId: null, lastMessageAt: null }
  it('says who is paired, who is waiting, and what went wrong', () => {
    expect(channelStatusLine(off)).toBe('Not connected')
    expect(channelStatusLine({ ...off, status: 'connected', peer: { name: 'Owner', handle: '@owner' } })).toBe('Connected to Owner (@owner)')
    expect(channelStatusLine({ ...off, status: 'pairing', candidate: { name: 'Owner', handle: null, at: '' } })).toBe('Owner wants to pair')
    expect(channelStatusLine({ ...off, status: 'error', detail: 'The token was refused.' })).toBe('The token was refused.')
  })
})
