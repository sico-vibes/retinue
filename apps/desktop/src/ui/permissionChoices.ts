/** Keep choices visually neutral while provider IDs remain opaque. */
export function isNegativePermissionChoice(choice: string): boolean {
  void choice
  return false
}

export function permissionChoiceLabel(choice: string, kind?: unknown) {
  // Phase C (14 November 2026): an employee asking to control this computer. The
  // allow choice lasts for the conversation, so it says so.
  if (kind === 'owner_computer') {
    if (choice === 'allow') return 'Allow for this conversation'
    if (choice === 'allow_once') return 'Once'
    if (choice === 'deny') return 'No'
  }
  if (choice === 'allow_once') return 'Allow once'
  if (choice === 'allow_session') return 'Allow this session'
  if (choice === 'deny') return 'Deny'
  if (choice === 'reject_once') return 'Reject once'
  if (choice === 'reject_always') return 'Reject always'
  return choice.replaceAll('_', ' ').replace(/\b\w/g, (letter) => letter.toUpperCase())
}
