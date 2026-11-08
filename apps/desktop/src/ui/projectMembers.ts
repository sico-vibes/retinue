// Who is hired into each project (8 November 2026, `project.members`). The
// sidebar and the Team page share one cache; a `project.members.changed`
// daemon event (relayed by App as a window event) or a change to the agent or
// project list refreshes it. An older daemon without the RPC gives `null`, and
// callers fall back to working membership out from conversations.
import { useEffect, useState } from 'react'
import { rpc } from '../tauri'

export type ProjectMember = { agentId: string; hiredAt?: number | string; sortOrder?: number }
export type MembersByProject = ReadonlyMap<string, readonly ProjectMember[]>

export const MEMBERS_CHANGED_EVENT = 'bitbots:project-members-changed'

export function announceMembersChanged() {
  try { window.dispatchEvent(new Event(MEMBERS_CHANGED_EVENT)) } catch { /* not in a window */ }
}

export async function loadMembers(projectIds: readonly string[]): Promise<MembersByProject | null> {
  const entries = await Promise.all(projectIds.map(async (projectId) => {
    const result = await rpc<{ members?: ProjectMember[] }>('project.members', { projectId })
    return [projectId, result.members ?? []] as const
  }))
  return new Map(entries)
}

/** Members per project, or null while unknown or when the daemon has no hiring. */
export function useProjectMembers(projectIds: readonly string[], refreshKey: unknown): MembersByProject | null {
  const [members, setMembers] = useState<MembersByProject | null>(null)
  const [tick, setTick] = useState(0)
  const key = projectIds.join('|')
  useEffect(() => {
    const bump = () => setTick((value) => value + 1)
    window.addEventListener(MEMBERS_CHANGED_EVENT, bump)
    return () => window.removeEventListener(MEMBERS_CHANGED_EVENT, bump)
  }, [])
  useEffect(() => {
    let live = true
    if (!key) { setMembers(new Map()); return }
    loadMembers(key.split('|')).then((value) => { if (live) setMembers(value) }, () => { if (live) setMembers(null) })
    return () => { live = false }
  }, [key, refreshKey, tick])
  return members
}
