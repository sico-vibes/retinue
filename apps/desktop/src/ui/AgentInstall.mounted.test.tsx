// @vitest-environment happy-dom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const h = vi.hoisted(() => ({ rpc: vi.fn(), listen: vi.fn(), open: vi.fn() }))
vi.mock('../tauri', () => ({ rpc: h.rpc, listenForDaemonEvents: h.listen, inDesktop: true }))
vi.mock('./openSafeLink', () => ({ openSafeLink: h.open }))

import { AgentInstallDialog, jobLine, megabytes, planSentences, type InstallJob, type InstallPlan } from './AgentInstall'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const MANAGE: InstallPlan = {
  provider: 'pi', displayName: 'Pi', mode: 'manage', version: '1.6.0', sourceHost: 'github.com', bytes: 62_400_000, digestPrefix: '9f2c41ab07de', digestAlgorithm: 'sha256',
  target: 'C:\\Users\\you\\AppData\\Local\\Retinue\\agents\\pi', freeBytes: 182_000_000_000, runsTurn: false, supported: true, reason: null, page: 'https://example.com/pi',
  owner: 'none', installed: { version: null, digestPrefix: null, damaged: false }, previousVersion: null, autoUpdate: false,
}
const job = (phase: InstallJob['phase'], extra: Partial<InstallJob> = {}): InstallJob => ({ jobId: 'job-1', provider: 'pi', mode: 'manage', phase, version: '1.6.0', bytesDone: 0, bytesTotal: 62_400_000, line: null, error: null, ...extra })

let root: Root
let host: HTMLDivElement
beforeEach(() => { h.rpc.mockReset(); h.listen.mockReset().mockResolvedValue(() => undefined); h.open.mockReset(); host = document.createElement('div'); document.body.appendChild(host); root = createRoot(host) })
afterEach(() => { act(() => root.unmount()); host.remove() })
const settle = async () => { await act(async () => { for (let index = 0; index < 4; index += 1) await Promise.resolve() }) }
const button = (text: string) => [...host.querySelectorAll<HTMLButtonElement>('button')].find((node) => node.textContent?.trim() === text)

describe('coding-agent install', () => {
  it('says each phase in words, and never shows an unknown size as zero', () => {
    expect(jobLine(job('downloading', { bytesDone: 25_600_000 }))).toBe('Downloading 41% · 26 MB of 62 MB')
    expect(jobLine(job('downloading', { bytesTotal: 0 }))).toBe('Downloading')
    expect(jobLine(job('checking'))).toBe('Checking the download')
    expect(jobLine(job('installing'))).toBe('Installing')
    expect(jobLine(job('done'))).toBe('Done · 1.6.0')
    expect(jobLine(job('failed', { error: 'The download did not match the publisher\'s checksum.' }))).toBe('The download did not match the publisher\'s checksum.')
    expect(megabytes(2_500_000_000)).toBe('2.5 GB')
    const adopt = planSentences({ ...MANAGE, mode: 'adopt', owner: 'npm', sourceHost: 'registry.npmjs.org', bytes: null, digestPrefix: null })
    expect(adopt[0]).toBe('Retinue updates your own copy to 1.6.0 with npm, from registry.npmjs.org. No window opens.')
    expect(planSentences({ ...MANAGE, bytes: null })[0]).not.toMatch(/\(0 MB\)/)
  })

  it('shows what it will download, then installs exactly the version and digest it showed', async () => {
    const onChanged = vi.fn()
    h.rpc.mockImplementation(async (method: string) => {
      if (method === 'runtime.install.plan') return { plan: MANAGE }
      if (method === 'runtime.install') return { jobId: 'job-1', provider: 'pi', mode: 'manage', version: '1.6.0' }
      if (method === 'runtime.install.status') return { job: job('done', { bytesDone: 62_400_000 }) }
      throw new Error(method)
    })
    await act(async () => { root.render(<AgentInstallDialog provider="pi" found={false} onClose={vi.fn()} onChanged={onChanged} />) })
    await settle()
    expect(host.textContent).toContain('github.com')
    expect(host.textContent).toContain('sha256 9f2c41ab07de…')
    expect(host.textContent).toContain('Your PATH and any copy you installed yourself are not changed.')
    await act(async () => { button('Install 1.6.0')!.click() })
    await settle()
    expect(h.rpc).toHaveBeenCalledWith('runtime.install', { provider: 'pi', confirm: true, version: '1.6.0', digestPrefix: '9f2c41ab07de' })
    expect(host.querySelector('.agent-install-job')?.textContent).toContain('Done · 1.6.0')
    expect(onChanged).toHaveBeenCalled()
    expect(button('Close')).toBeTruthy()
  })

  it('offers only the install page for a coding agent Retinue does not download', async () => {
    h.rpc.mockResolvedValue({ plan: { ...MANAGE, provider: 'qwen', displayName: 'Qwen Code', mode: 'link', supported: false, version: null, reason: 'Retinue does not download this coding agent yet. Open its install page instead.', page: 'https://github.com/QwenLM/qwen-code' } })
    await act(async () => { root.render(<AgentInstallDialog provider="qwen" found={false} onClose={vi.fn()} onChanged={vi.fn()} />) })
    await settle()
    expect(host.textContent).toContain('Retinue does not download this coding agent yet.')
    expect(host.querySelector('.agent-install .primary-button')).toBeNull()
    await act(async () => { button('Open the install page')!.click() })
    expect(h.open).toHaveBeenCalledWith('https://github.com/QwenLM/qwen-code')
  })

  it('will not update the owner\'s own copy while a bot is using it', async () => {
    h.rpc.mockResolvedValue({ plan: { ...MANAGE, provider: 'codex', displayName: 'Codex', mode: 'adopt', owner: 'npm', runsTurn: true, bytes: null, digestPrefix: null } })
    await act(async () => { root.render(<AgentInstallDialog provider="codex" found onClose={vi.fn()} onChanged={vi.fn()} />) })
    await settle()
    expect(host.textContent).toContain('A bot is using it now.')
    expect(button('Update to 1.6.0')?.disabled).toBe(true)
  })

  it('asks before removing Retinue\'s copy, and removes only on confirm', async () => {
    h.rpc.mockImplementation(async (method: string) => method === 'runtime.install.plan' ? { plan: { ...MANAGE, installed: { version: '1.5.2', digestPrefix: '4be1c0de9a71', damaged: false } } } : { removed: true, provider: 'pi', kept: [] })
    const onChanged = vi.fn()
    await act(async () => { root.render(<AgentInstallDialog provider="pi" found onClose={vi.fn()} onChanged={onChanged} />) })
    await settle()
    await act(async () => { button('Remove Retinue\'s copy')!.click() })
    expect(h.rpc).not.toHaveBeenCalledWith('runtime.install.remove', expect.anything())
    await act(async () => { button('Remove')!.click() })
    await settle()
    expect(h.rpc).toHaveBeenCalledWith('runtime.install.remove', { provider: 'pi', confirm: true })
    expect(onChanged).toHaveBeenCalled()
  })
})
