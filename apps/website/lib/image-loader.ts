// next/image loader for the static export. Each image exists at a fixed set of
// widths (scripts/generate-images.mjs); this picks the smallest one that covers
// the requested width. Files outside the table pass through untouched.
const WIDTHS: Record<string, number[]> = {
  '/people/': [96, 192, 384],
  '/art/': [640, 960, 1280, 1920],
  '/shots/': [640, 960, 1280, 1920],
}

export default function imageLoader({ src, width }: { src: string; width: number; quality?: number }) {
  const folder = Object.keys(WIDTHS).find((key) => src.startsWith(key))
  if (!folder) return src
  const widths = WIDTHS[folder]
  const chosen = widths.find((candidate) => candidate >= width) ?? widths[widths.length - 1]
  return src.replace(/\.webp$/, `-${chosen}.webp`)
}
