// Preset pictures for the chat background (owner, 16 November 2026). They ship in
// `public/background/` as `<category>-<n>.webp` and are served from the app's own origin.

export type WallpaperCategory = 'anime' | 'nature' | 'fantasy' | 'space' | 'scifi' | 'samurai'
export type WallpaperPreset = { id: string; category: WallpaperCategory; url: string }

/** `count` is how many pictures ship in the category (the owner dropped two samurai pictures, 16 November 2026). */
export const WALLPAPER_CATEGORIES: { value: WallpaperCategory; label: string; count: number }[] = [
  { value: 'anime', label: 'Anime', count: 5 },
  { value: 'nature', label: 'Nature', count: 5 },
  { value: 'fantasy', label: 'Fantasy', count: 5 },
  { value: 'space', label: 'Space', count: 5 },
  { value: 'scifi', label: 'Sci-fi', count: 5 },
  { value: 'samurai', label: 'Samurai', count: 3 },
]

export const WALLPAPER_PRESETS: WallpaperPreset[] = WALLPAPER_CATEGORIES.flatMap(({ value, count }) =>
  Array.from({ length: count }, (_, index) => {
    const id = `${value}-${index + 1}`
    return { id, category: value, url: `/background/${id}.webp` }
  }))

export const wallpaperPreset = (id: string | null | undefined) => WALLPAPER_PRESETS.find((preset) => preset.id === id) ?? null

/** The picture behind the Haze window in onboarding's look preview. */
export const ONBOARDING_WALLPAPER = wallpaperPreset('samurai-2')!
