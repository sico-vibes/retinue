import { afterEach, describe, expect, it } from 'vitest'
import { setMediaScope } from './MediaPreview'
import { closeDocument, getOpenDocument, openDocument, registerDocumentPanel } from './documentStore'

// Owner request, 16 November 2026: project files open in a panel beside the chat, not a dialog.
describe('document panel store', () => {
  afterEach(() => { closeDocument(); setMediaScope(null) })

  it('falls back (returns false) where there is no panel or no open conversation', () => {
    setMediaScope({ sessionId: 's1', projectPath: 'C:/p' })
    expect(openDocument('docs/a.md')).toBe(false)
    const release = registerDocumentPanel()
    setMediaScope(null)
    expect(openDocument('docs/a.md')).toBe(false)
    release()
  })

  it('opens a file for the current conversation, names it after the file, and closes when the panel goes', () => {
    const release = registerDocumentPanel()
    setMediaScope({ sessionId: 's1', projectPath: 'C:/p' })
    expect(openDocument('docs/research/05-messaging.md')).toBe(true)
    expect(getOpenDocument()).toEqual({ path: 'docs/research/05-messaging.md', title: '05-messaging.md', sessionId: 's1' })
    expect(openDocument('docs/b.md', 'Report B')).toBe(true)
    expect(getOpenDocument()?.title).toBe('Report B')
    release()
    expect(getOpenDocument()).toBeNull()
  })
})
