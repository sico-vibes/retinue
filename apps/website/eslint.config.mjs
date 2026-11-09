import nextVitals from 'eslint-config-next/core-web-vitals'
import nextTypescript from 'eslint-config-next/typescript'

const config = [
  ...nextVitals,
  ...nextTypescript,
  // A static export has no RSC payload for next/link to fetch on any host, so the site uses plain links.
  { rules: { '@next/next/no-html-link-for-pages': 'off' } },
  { ignores: ['.next/**', 'out/**', 'next-env.d.ts', 'scripts/**'] },
]

export default config
