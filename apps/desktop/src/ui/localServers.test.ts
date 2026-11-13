import { describe, expect, it } from 'vitest'
import { parseLocalServers } from './localServers'

describe('local servers', () => {
  it('keeps loopback servers and drops anything that points off this computer', () => {
    const servers = parseLocalServers({ servers: [
      { port: 5173, url: 'http://127.0.0.1:5173/', process: 'node.exe', answeredHtml: true },
      { port: 3000, url: 'http://[::1]:3000/', process: '', answeredHtml: false },
      { port: 80, url: 'http://192.168.1.20:80/', process: 'nginx.exe', answeredHtml: true },
      { port: 443, url: 'https://example.com/', answeredHtml: true },
      { port: 0, url: 'http://127.0.0.1:0/' },
      { port: 8080, url: 'not a url' },
    ] })
    expect(servers).toEqual([
      { port: 5173, url: 'http://127.0.0.1:5173/', process: 'node.exe', answeredHtml: true },
      { port: 3000, url: 'http://[::1]:3000/', process: null, answeredHtml: false },
    ])
    expect(parseLocalServers(null)).toEqual([])
    expect(parseLocalServers({ servers: 'nope' })).toEqual([])
  })
})
