// RPC errors are separate from failed jobs, which arrive in successful replies.
export const workflowRpcErrors = {
  'machine.probe': { invalid_argument: "That address isn't a web address." },
  'machine.pair.begin': { invalid_argument: "The pairing code is required." },
  'project.clone': { invalid_argument: 'Choose an existing folder to clone into.', conflict: 'That folder already exists.' },
  'project.discover': { invalid_argument: 'Choose an existing folder to look in.' },
  'project.content.search': { invalid_argument: 'Type something to look for, up to 200 characters.' },
  'project.create': { invalid_argument: 'agent fields or membership are invalid', not_found: 'agent or runtime not found', internal: 'agent storage operation failed' },
  'project.update': { invalid_argument: 'agent fields or membership are invalid', not_found: 'agent or runtime not found', internal: 'agent storage operation failed' },
  'project.delete': { not_found: 'agent or runtime not found', conflict: 'This bot is locked. Unlock it to change its settings.', internal: 'agent storage operation failed' },
  'project.sharedSettings.trust': { invalid_argument: 'Confirm this file first.', not_found: 'agent or runtime not found', conflict: 'That file has changed. Check it again.', internal: 'agent storage operation failed' },
  'project.sharedSettings.untrust': { invalid_argument: 'projectId is required', not_found: 'agent or runtime not found', internal: 'agent storage operation failed' },
  'project.sharedSettings.write': { invalid_argument: 'The file could not be written.', not_found: 'agent or runtime not found', conflict: 'That file already exists. Replace it?', internal: 'agent storage operation failed' },
  'project.scaffold': { invalid_argument: 'Choose an existing folder to create the project in.', conflict: "That folder already exists and isn't empty." },
} satisfies Record<string, Record<string, string>>

export const pairingFailures = {
  unreachable: "That address can't be reached right now.",
  wrongServer: "That address isn't a Retinue Remote address.",
  requiresPassword: 'That machine now asks for its password. Start again.',
  certificateChanged: "The other machine's certificate changed.",
  refused: 'The other machine refused the request.',
  responseTooLarge: "The other machine's answer was too large.",
  deviceRevoked: 'The other machine no longer accepts this one. Pair again to continue.',
  transportUnreachable: "The other machine can't be reached right now.",
  stoppedAnswering: 'The other machine stopped answering.',
  noSignIn: "The other machine didn't give this computer a sign-in.",
  newerVersion: 'That machine needs a newer version of Retinue.',
  sameMachine: "That's this machine.",
  signInSave: 'The sign-in for that machine could not be saved.',
  linkSave: 'The link could not be saved.',
}

export const dictationReasons = [
  'verification required', "blocked by openai's bot check", 'request rejected',
  'sign-in required', 'sign-in expired', 'rate limited', 'unavailable',
  'recording too large', 'recording too short', 'unreadable response', 'rejected',
]

export function workflowError(method: string, code: string) {
  const errors: Record<string, Record<string, string>> = workflowRpcErrors
  return errors[method]?.[code]
}

export const scaffoldFailures = {
  folder: 'The folder could not be created.',
  first_commit: 'README.md could not be written.',
}

export const clipboardReadFailures = {
  read: 'The clipboard image could not be read.',
  system: 'The system clipboard could not be read.',
  large: 'The clipboard image is too large to attach.',
  decode: 'The clipboard image could not be decoded.',
  encode: 'The clipboard image could not be encoded.',
}
