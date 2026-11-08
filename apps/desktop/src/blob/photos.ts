// Profile photos (owner's idea, 8 November 2026): instead of a mascot, a logo
// or a character, an employee can wear a round profile picture, as people do in
// Slack or Teams. The pictures are the owner's renders in `assets/profile`,
// cropped to head and shoulders at 256px (`blob/photos/photo-NN.webp`).
//
// Stored as `look.photo`. It wins over the art and the Haze icon in every look,
// so the same face shows in Haze and in Stone. Preview first: the daemon's
// `valid_look` does not know the field yet, so the installed app drops it.

const assets = import.meta.glob('./photos/*.webp', { eager: true, query: '?url', import: 'default' }) as Record<string, string>

export interface ProfilePhoto {
  readonly id: `photo-${string}`
  readonly url: string
}

export const PROFILE_PHOTOS: readonly ProfilePhoto[] = Object.keys(assets)
  .sort()
  .map((path) => ({ id: path.slice('./photos/'.length, -'.webp'.length) as `photo-${string}`, url: assets[path]! }))

const BY_ID = new Map(PROFILE_PHOTOS.map((photo) => [photo.id as string, photo.url]))

export function isProfilePhoto(value: unknown): value is ProfilePhoto['id'] {
  return typeof value === 'string' && BY_ID.has(value)
}

/** The picture a look wears, or null when it wears none. */
export function profilePhotoUrl(look: { photo?: unknown } | null | undefined): string | null {
  return isProfilePhoto(look?.photo) ? BY_ID.get(look.photo) ?? null : null
}
