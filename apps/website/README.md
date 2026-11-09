# Retinue website

Next.js (App Router) with React, TypeScript, Tailwind CSS and Motion. It builds to plain static files that any host can serve.

## Run and build

```
cd apps/website
npm install
npm run dev        # http://localhost:5290, with hot reload
npm run build      # lint, type-check, then export to out/
npm run preview    # serve out/ at http://127.0.0.1:5291, as a static host would
```

Deploy by uploading the contents of `out/`.

## Before going live

Set `downloadUrl` in `content/site.ts`. While it's empty, every download button goes to the download section.

## Layout

| Path | What's there |
|---|---|
| `app/` | Pages, the root layout and the design tokens (`globals.css`) |
| `components/sections/` | One file per page section, plus `deferred.tsx` |
| `components/ui/` | Reusable parts: `Section`, `Container`, `Eyebrow`, `Heading`, `Lead`, `Button`, `Avatar`, `EmployeeCard`, `FeatureCard`, `OrgChart`, `ProductFrame`, `Marquee`, `CTA`, `Nav`, `Footer` |
| `components/story/`, `components/mock/` | The idea-to-shipped scenes and the illustrated product mocks |
| `content/` | All the words and data, typed |
| `public/` | Pre-sized images (made by `npm run images`) |

## Adding things

- **A capability in the working day:** add a row to `content/features.ts`. If it needs a new picture, add a component to `components/mock/visuals.tsx` and register it in `featureVisuals`.
- **An employee:** add them to `content/employees.ts` and their photo to `scripts/photo-map.json`, then run `npm run images`.
- **A hiring example:** add a row to `content/hiring.ts`.
- **A section:** create it in `components/sections/` and add it to `app/page.tsx`. If it uses JavaScript, add it to `deferred.tsx` too, so its code only loads as it nears the screen. Sections using Motion wrap themselves in `MotionProvider`.
- **A page:** add `app/<name>/page.tsx`, and link it in `content/site.ts`. `app/team/page.tsx` is a small example.

## Images

```
npm run images -- --assets "<repository>/assets" --shots "<folder of product shots>"
```

This makes the WebP sizes in `public/`. It reads `website/hero.png`, `website/footer.png`, `profile/*.png` and `icon/retinue-*.png`. `--shots` is optional; files there are named `<name>-dark` and `<name>-light`. `lib/image-loader.ts` picks the right size for `next/image`.

Which render belongs to which employee is set in `scripts/photo-map.json`. The source files carry no names.

## Conventions

- **Animation:** transforms and opacity only. Scroll-driven sections use Motion. Page reveals, the hero and the marquee are plain CSS, so they cost no JavaScript.
- **Reduced motion:** every animation has a static fallback. The pinned sections become ordinary stacked layouts.
- **Theme:** a saved choice wins, then the system setting, and dark otherwise. Images that differ use the `only-dark` and `only-light` classes.
- **Links:** plain `<a>` tags, because a static host has no payloads for `next/link` to prefetch.
