# Retinue site

The marketing site, rebuilt in November 2026 (it replaces `apps/website`, kept for now). Next.js with static export, Tailwind CSS, React Bits components, GSAP and Lenis smooth scrolling. Dark and light themes.

## Run and build

```
cd apps/site
npm install
npm run dev     # http://localhost:5300
npm run build   # makes WebP copies of the art, lints, then exports plain files to out/
```

Deploy by uploading the contents of `out/`.

## Where things are

| Path | What's there |
|---|---|
| `app/` | The page, the root layout (fonts, theme before first paint) and the design tokens (`globals.css`) |
| `components/sections/` | One file per section, top to bottom in `app/page.tsx` |
| `components/bits/` | React Bits components, copied unchanged from their registry by `scripts/add-bits.mjs` |
| `components/ui.tsx`, `theme.tsx`, `motion.tsx` | Shared parts: section header, buttons, frames, art; the theme toggle; smooth scrolling and `WhenVisible` |
| `content/site.ts` | Every word and link on the home page |
| `content/roster.json`, `public/people/`, `public/badges/` | The employees, built from the app's own presets by `scripts/roster.mjs`; their ID badges by `scripts/badges.mjs` |
| `content/docs/` | The customer docs (Markdown with front matter), shown at /docs/; screenshots in `public/docs/` |
| `app/team/`, `app/changelog/`, `app/docs/` | The other pages. The changelog reads `docs/releases/` from the repository at build time |
| `public/art/` | The WebP art the site loads. Iris's PNG masters live in `art-source/` (not in git); `npm run art` regenerates the WebP from them |
| `public/shots/` | Product screenshots, one per theme |

## Screenshots

The app's own screens come from its browser preview with real employees (`?demo&clean`), captured by Edge at 2x in both looks: start the preview from the repo root (`npx vite --config apps/desktop/vite.preview.config.ts apps/desktop --port 5174`), then `node scripts/capture.mjs` (the home page shots) or `node scripts/capture.mjs --list content/docs/shots.json` (the docs). `node scripts/review.mjs` captures the site itself, whole page, both themes, into C:/rtmp/review.

## Adding a React Bits component

`node scripts/add-bits.mjs ComponentName` copies the TypeScript + Tailwind version into `components/bits/`.

## Rules learned building it

- **One or two WebGL backgrounds on screen at a time.** A page can hold only a few WebGL contexts. Wrap each shader in `WhenVisible`, which mounts it only while it is near the screen.
- **Strict Mode is off on purpose.** React Bits shaders release their context on unmount, and Strict Mode's double run in development then reuses a lost context (a blank canvas).
- **Lenis drives GSAP.** The reveals use ScrollTrigger; `SmoothScroll` runs Lenis from GSAP's ticker and updates ScrollTrigger on every scroll.
- **Art renders only once the page is live** (`Art` in `components/ui.tsx`), so a missing picture shows nothing instead of a broken image.
- **Check art** (the masters in `art-source/`) with `node scripts/check-art.mjs`: size, weight, and whether a PNG really is transparent.
