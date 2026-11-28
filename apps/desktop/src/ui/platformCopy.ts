// Words that name the operating system. Windows keeps its exact wording; Linux
// gets the plain equivalent (Linux port, 27 November 2026). Read the OS from
// `usePlatform().os` or `platform().os`, never from the user agent here.
import type { HostOs } from '../transport/platform'

/** Where the daemon keeps secrets: Credential Manager on Windows, the Secret Service keyring on Linux (or an owner-only file when no keyring runs). */
export function secretStoreName(os: HostOs): string {
  return os === 'windows' ? 'Windows Credential Manager' : 'your system keyring'
}

/** One sentence for Linux only: what happens without a keyring. Empty on Windows. */
export function secretStoreFallbackNote(os: HostOs): string {
  return os === 'linux' ? ' With no keyring running, they go in a file only your account can read.' : ''
}

/** The settings label for starting Retinue at sign-in. */
export function startAtLoginLabel(os: HostOs): string {
  return os === 'windows' ? 'Start with Windows' : 'Start when you sign in'
}

/** The system's file manager, for "Reveal in …". */
export function fileManagerName(os: HostOs): string {
  return os === 'windows' ? 'File Explorer' : 'the file manager'
}

/** "Windows" or "the system", as the subject of a sentence ("… did not report a file path"). */
export function systemName(os: HostOs): string {
  return os === 'windows' ? 'Windows' : 'The system'
}

/** "this Windows device" or "this computer". */
export function thisDevice(os: HostOs): string {
  return os === 'windows' ? 'this Windows device' : 'this computer'
}

/** The firewall Native Remote checks. Only Windows Firewall is read today; on Linux the state is unknown. */
export function firewallName(os: HostOs): string {
  return os === 'windows' ? 'Windows Firewall' : 'The firewall'
}
