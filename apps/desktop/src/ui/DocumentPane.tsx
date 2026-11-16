import { useEffect, useMemo, useRef, useState } from 'react'
import { AlertCircle, Code2, Eye, FileText, FolderOpen, ListTree, LoaderCircle, Maximize2, Minimize2, X } from 'lucide-react'
import { revealInExplorer } from '../tauri'
import { MediaPreview, formatSize, useFilePreview, useMediaScope } from './MediaPreview'
import { SafeMarkdown, parseMarkdownBlocks } from './SafeMarkdown'
import { closeDocument, registerDocumentPanel, type OpenDocument } from './documentStore'
import './documentPane.css'

const markdownFile = /\.(md|markdown)$/i

/** Plain heading text for the outline: inline Markdown marks removed. */
function headingLabel(text: string) {
  return text.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/[`*_~]/g, '').trim()
}

/** Keeps the panel registered while the main window shows it, so file links open here rather than in a dialog. */
export function useDocumentPanelSlot(enabled: boolean) {
  useEffect(() => enabled ? registerDocumentPanel() : undefined, [enabled])
}

/**
 * A project file beside the chat, the way an editor's side panel shows it: Markdown is rendered with an outline,
 * other text is shown as source, and media uses the same daemon-authorised preview as the chat.
 */
export function DocumentPane({ doc, expanded, onToggleExpanded, onOpenLink }: { doc: OpenDocument; expanded: boolean; onToggleExpanded: () => void; onOpenLink: (url: string) => void }) {
  const scope = useMediaScope()
  const state = useFilePreview(doc.sessionId, doc.path)
  const isMarkdown = markdownFile.test(doc.path)
  const [source, setSource] = useState(false)
  const [outlineOpen, setOutlineOpen] = useState(true)
  const body = useRef<HTMLDivElement>(null)
  const closeRef = useRef<HTMLButtonElement>(null)

  // A different conversation has its own folder: the file no longer belongs to what is on screen.
  useEffect(() => { if (scope && scope.sessionId !== doc.sessionId) closeDocument() }, [scope, doc.sessionId])
  useEffect(() => { setSource(false); body.current?.scrollTo({ top: 0 }) }, [doc.path])
  useEffect(() => { closeRef.current?.focus({ preventScroll: true }) }, [doc.path])

  const reply = state.phase === 'done' ? state.reply : null
  const text = reply?.status === 'ok' && reply.kind === 'text' ? reply.text ?? '' : null
  const headings = useMemo(() => text != null && isMarkdown
    ? parseMarkdownBlocks(text).filter((block) => block.kind === 'heading').map((block) => ({ level: block.level ?? 1, label: headingLabel(block.text ?? '') }))
    : [], [text, isMarkdown])
  const rendered = text != null && isMarkdown && !source
  const showOutline = rendered && outlineOpen && headings.length > 1

  const jump = (index: number) => {
    const scroller = body.current
    const target = scroller?.querySelectorAll<HTMLElement>('.safe-markdown > h2, .safe-markdown > h3, .safe-markdown > h4, .safe-markdown > h5')[index]
    // Scroll only the document; scrollIntoView would also shift the window's own layout.
    if (scroller && target) scroller.scrollTo({ top: scroller.scrollTop + target.getBoundingClientRect().top - scroller.getBoundingClientRect().top - 12, behavior: 'smooth' })
  }
  const reveal = () => void revealInExplorer(reply?.relativePath ?? doc.path, scope?.projectPath ?? null).catch(() => undefined)
  const name = doc.path.split(/[\\/]/).pop() || doc.path

  return <aside className="document-pane" aria-label={`Document: ${name}`} onKeyDown={(event) => { if (event.key === 'Escape' && !event.defaultPrevented) { event.preventDefault(); closeDocument() } }}>
    <header className="document-tabs">
      <div className="document-tab" title={doc.path}><FileText size={13} aria-hidden="true" /><span>{name}</span><button ref={closeRef} type="button" className="document-icon" aria-label={`Close ${name}`} onClick={closeDocument}><X size={13} aria-hidden="true" /></button></div>
      <span className="document-tabs-fill" />
      <button type="button" className="document-icon" aria-label={expanded ? 'Show the chat again' : 'Widen the document'} title={expanded ? 'Show the chat again' : 'Widen the document'} onClick={onToggleExpanded}>{expanded ? <Minimize2 size={14} aria-hidden="true" /> : <Maximize2 size={14} aria-hidden="true" />}</button>
      <button type="button" className="document-icon" aria-label="Close the document panel" title="Close" onClick={closeDocument}><X size={15} aria-hidden="true" /></button>
    </header>
    <div className="document-bar">
      <span className="document-path" title={reply?.relativePath ?? doc.path}>{reply?.relativePath ?? doc.path}</span>
      {reply?.status === 'ok' && <small>{formatSize(reply.size)}{reply.truncated ? ' · first 256 KB' : ''}</small>}
      <span className="document-bar-actions">
        {text != null && isMarkdown && <button type="button" className={`document-icon${source ? ' active' : ''}`} aria-pressed={source} aria-label={source ? 'Show formatted' : 'Show source'} title={source ? 'Show formatted' : 'Show source'} onClick={() => setSource((value) => !value)}>{source ? <Eye size={14} aria-hidden="true" /> : <Code2 size={14} aria-hidden="true" />}</button>}
        {rendered && headings.length > 1 && <button type="button" className={`document-icon${outlineOpen ? ' active' : ''}`} aria-pressed={outlineOpen} aria-label="Outline" title="Outline" onClick={() => setOutlineOpen((value) => !value)}><ListTree size={14} aria-hidden="true" /></button>}
        <button type="button" className="document-icon" aria-label="Show in folder" title="Show in folder" onClick={reveal}><FolderOpen size={14} aria-hidden="true" /></button>
      </span>
    </div>
    <div className={`document-main${showOutline ? ' with-outline' : ''}`}>
      {showOutline && <nav className="document-outline" aria-label="Outline">{headings.map((heading, index) => <button key={index} type="button" className={`level-${Math.min(heading.level, 4)}`} title={heading.label} onClick={() => jump(index)}>{heading.label}</button>)}</nav>}
      <div ref={body} className="document-body" tabIndex={0}>
        {state.phase === 'loading' && <p className="document-note"><LoaderCircle size={15} className="spinning" aria-hidden="true" />Opening {name}</p>}
        {state.phase === 'error' && <p className="document-note error"><AlertCircle size={15} aria-hidden="true" />{state.message || 'This file could not be opened.'}</p>}
        {reply && reply.status !== 'ok' && <p className="document-note error"><AlertCircle size={15} aria-hidden="true" />{reply.reason ?? 'This file cannot be previewed.'}</p>}
        {rendered && <article className="document-markdown"><SafeMarkdown text={text} onOpenLink={onOpenLink} /></article>}
        {text != null && !rendered && <pre className="document-source"><code>{text}</code></pre>}
        {reply?.status === 'ok' && reply.kind !== 'text' && <div className="document-media"><MediaPreview path={doc.path} /></div>}
      </div>
    </div>
  </aside>
}
