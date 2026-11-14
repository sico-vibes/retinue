import type { NextConfig } from 'next'

// Static export: `npm run build` writes plain files to out/, which any host can serve.
const config: NextConfig = {
  output: 'export',
  trailingSlash: true,
  // Off: React Bits shaders release their WebGL context on unmount, and Strict Mode's double effect
  // run in development then reuses a lost context (a blank canvas). Production never double-runs.
  reactStrictMode: false,
  images: { unoptimized: true },
}

export default config
