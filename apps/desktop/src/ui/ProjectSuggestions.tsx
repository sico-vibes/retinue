// Three first tasks for a new conversation, read from the project's own folder (wave 3, 17 November 2026):
// its README heading, a script, the newest commit, its AGENTS.md. `project.suggestions` reads them; nothing runs
// until the owner sends one. A card fills the message box so the owner can edit it first.
import { useEffect, useState } from 'react'
import { BookOpen, FileText, GitCommitHorizontal, Lightbulb, Play } from 'lucide-react'
import { rpc } from '../tauri'

export type ProjectSuggestion = {
  id: string
  kind: 'readme' | 'script' | 'commit' | 'agents' | 'static'
  title: string
  prompt: string
  source: { file: string | null; detail: string } | null
}

const ICONS = { readme: BookOpen, script: Play, commit: GitCommitHorizontal, agents: FileText, static: Lightbulb } as const

/** Only complete suggestions, at most three. */
export function suggestionsFrom(raw: unknown): ProjectSuggestion[] {
  const rows = raw && typeof raw === 'object' ? (raw as { suggestions?: unknown }).suggestions : null
  if (!Array.isArray(rows)) return []
  return rows.filter((row): row is ProjectSuggestion => !!row && typeof row === 'object'
    && typeof (row as ProjectSuggestion).id === 'string' && typeof (row as ProjectSuggestion).title === 'string' && typeof (row as ProjectSuggestion).prompt === 'string').slice(0, 3)
}

export function ProjectSuggestions({ projectId, onPick }: { projectId: string; onPick: (prompt: string) => void }) {
  const [suggestions, setSuggestions] = useState<ProjectSuggestion[]>([])
  useEffect(() => {
    let live = true
    setSuggestions([])
    // An older daemon has no such read; the empty state then simply shows no cards.
    void rpc('project.suggestions', { projectId }).then((reply) => { if (live) setSuggestions(suggestionsFrom(reply)) }).catch(() => undefined)
    return () => { live = false }
  }, [projectId])
  if (!suggestions.length) return null
  return <ul className="project-suggestions" aria-label="Ideas to start with">
    {suggestions.map((suggestion) => {
      const Icon = ICONS[suggestion.kind] ?? Lightbulb
      return <li key={suggestion.id}><button type="button" className="project-suggestion" title={suggestion.prompt} onClick={() => onPick(suggestion.prompt)}>
        <Icon size={15} aria-hidden="true" />
        <span><strong>{suggestion.title}</strong>{suggestion.source && <small>{suggestion.source.file ? `${suggestion.source.file} · ` : ''}{suggestion.source.detail}</small>}</span>
      </button></li>
    })}
  </ul>
}
