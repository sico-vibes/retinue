// The CEO's `propose_hires` makes one proposal per candidate with a shared
// groupId (docs/plans/hiring-brief.md). The card offers Hire all for the rest.
import type { PermissionRequest } from '../types'

export function hireSiblings(permissions: readonly PermissionRequest[], current: PermissionRequest): PermissionRequest[] {
  const groupOf = (request: PermissionRequest) => {
    const payload = request.proposal?.payload as Record<string, unknown> | undefined
    return request.kind === 'proposal' && request.proposal?.kind === 'hire' && typeof payload?.groupId === 'string' ? payload.groupId : null
  }
  const group = groupOf(current)
  if (!group) return []
  return permissions.filter((request) => request.id !== current.id && groupOf(request) === group)
}
