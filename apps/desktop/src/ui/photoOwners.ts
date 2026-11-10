// Who wears which stock photo (owner, 9 November 2026): a photo belongs to one employee until
// that employee changes it, so the picker shows the others' photos as taken. The daemon enforces it.
import { createContext, useContext } from 'react'
import type { Agent } from '../types'

export type PhotoOwner = { agentId: string; name: string }
export const PhotoOwnersContext = createContext<ReadonlyMap<string, PhotoOwner>>(new Map())
export const usePhotoOwners = () => useContext(PhotoOwnersContext)

/** Stock photo id → the employee wearing it. Archived employees keep theirs; uploads are not counted. */
export function photoOwners(agents: readonly Agent[]): Map<string, PhotoOwner> {
  const owners = new Map<string, PhotoOwner>()
  for (const agent of agents) {
    const photo = (agent.look as { photo?: unknown } | null | undefined)?.photo
    if (typeof photo === 'string' && /^photo-\d+$/.test(photo) && !owners.has(photo)) owners.set(photo, { agentId: agent.id, name: agent.name })
  }
  return owners
}
