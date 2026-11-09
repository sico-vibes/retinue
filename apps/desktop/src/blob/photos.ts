// Profile photos (owner, 8–9 November 2026): every employee wears a round
// profile picture, as people do in Slack or Teams. The pictures are the
// owner's renders in `assets/profile`, cropped to head and shoulders at 256px
// (`blob/photos/photo-NN.webp`); the owner can also upload their own. Without
// a photo, an employee shows a blank silhouette.
//
// Stored as `look.photo`: a preset id (`photo-NN`) or an uploaded picture as a
// small `data:image/webp` URL (`valid_photo` in the storage crate checks both).
// Each preset employee wears its own photo (`photo` in `team/presets.json`).

const assets = import.meta.glob('./photos/*.webp', { eager: true, query: '?url', import: 'default' }) as Record<string, string>

export interface ProfilePhoto {
  readonly id: `photo-${string}`
  readonly url: string
}

export const PROFILE_PHOTOS: readonly ProfilePhoto[] = Object.keys(assets)
  .sort()
  .map((path) => ({ id: path.slice('./photos/'.length, -'.webp'.length) as `photo-${string}`, url: assets[path]! }))

const BY_ID = new Map(PROFILE_PHOTOS.map((photo) => [photo.id as string, photo.url]))

/** An uploaded picture is kept small: 256px square, under this many characters. */
export const MAX_UPLOAD_LENGTH = 160_000
const UPLOAD = /^data:image\/(webp|png|jpeg);base64,[A-Za-z0-9+/=]+$/

export type PhotoValue = ProfilePhoto['id'] | `data:image/${string}`

export function isUploadedPhoto(value: unknown): value is `data:image/${string}` {
  return typeof value === 'string' && value.length <= MAX_UPLOAD_LENGTH && UPLOAD.test(value)
}

export function isProfilePhoto(value: unknown): value is PhotoValue {
  return (typeof value === 'string' && BY_ID.has(value)) || isUploadedPhoto(value)
}

/** The value as a photo a look can wear, or undefined when it is not one. */
export function asPhoto(value: unknown): PhotoValue | undefined {
  return isProfilePhoto(value) ? value : undefined
}

/** The picture a look wears, or null when it wears none (the blank silhouette). */
export function profilePhotoUrl(look: { photo?: unknown } | null | undefined): string | null {
  const photo = look?.photo
  if (isUploadedPhoto(photo)) return photo
  return typeof photo === 'string' ? BY_ID.get(photo) ?? null : null
}

/** Read an uploaded image, crop its centre to a square and shrink it to 256px WebP. */
export async function photoFromFile(file: File): Promise<`data:image/${string}`> {
  if (!file.type.startsWith('image/')) throw new Error('Choose a picture: PNG, JPEG, WebP or GIF.')
  if (file.size > 20 * 1024 * 1024) throw new Error('That picture is over 20 MB. Choose a smaller one.')
  const bitmap = await createImageBitmap(file)
  const side = Math.min(bitmap.width, bitmap.height)
  const canvas = document.createElement('canvas')
  canvas.width = 256
  canvas.height = 256
  const context = canvas.getContext('2d')
  if (!context) throw new Error('That picture could not be read.')
  context.imageSmoothingQuality = 'high'
  context.drawImage(bitmap, (bitmap.width - side) / 2, (bitmap.height - side) / 2, side, side, 0, 0, 256, 256)
  bitmap.close()
  for (const quality of [0.86, 0.72, 0.6]) {
    const url = canvas.toDataURL('image/webp', quality)
    if (isUploadedPhoto(url)) return url
  }
  throw new Error('That picture could not be made small enough.')
}
