// The word the UI uses for a character: an employee (owner, 9 November 2026),
// who wears a profile picture and is hired into projects. Kept as a helper so
// copy has one place to change.

import { useThemeStyle } from '../blob/retinue'
import type { ThemeStyle } from './appearance'

export interface EntityNounOptions {
  plural?: boolean
  capital?: boolean
}

export function entityNoun(_style: ThemeStyle, options: EntityNounOptions = {}): string {
  const word = 'employee' + (options.plural ? 's' : '')
  return options.capital ? word[0]!.toUpperCase() + word.slice(1) : word
}

export function useEntityNoun(options: EntityNounOptions = {}): string {
  return entityNoun(useThemeStyle(), options)
}
