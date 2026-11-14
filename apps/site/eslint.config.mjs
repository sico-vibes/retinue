import nextVitals from 'eslint-config-next/core-web-vitals'
import nextTypescript from 'eslint-config-next/typescript'

const config = [
  ...nextVitals,
  ...nextTypescript,
  // A static export has no RSC payload for next/link to fetch on any host, so the site uses plain links.
  { rules: { '@next/next/no-html-link-for-pages': 'off', '@next/next/no-img-element': 'off' } },
  // React Bits components are vendored as published; they are not linted as our own code.
  { ignores: ['.next/**', 'out/**', 'next-env.d.ts', 'scripts/**', 'components/bits/**'] },
]

export default config
