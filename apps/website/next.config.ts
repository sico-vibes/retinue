import type { NextConfig } from 'next'

// Static export: `npm run build` writes plain files to out/, which any host can serve.
// Images are pre-sized by scripts/generate-images.mjs and picked by lib/image-loader.ts.
const config: NextConfig = {
  output: 'export',
  trailingSlash: true,
  reactStrictMode: true,
  images: {
    loader: 'custom',
    loaderFile: './lib/image-loader.ts',
    deviceSizes: [640, 960, 1280, 1920],
    imageSizes: [96, 192, 384],
  },
}

export default config
