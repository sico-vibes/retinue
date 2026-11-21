import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Browser preview with fixture data: `npx vite --config vite.preview.config.ts`,
// then open /gallery.html (every desktop-only surface), / (main window),
// /?companion (companion) or /preview.html (character sheet).
const fixture = (file: string) => fileURLToPath(new URL(`./src/preview/${file}`, import.meta.url))

export default defineConfig({
  server: { watch: { ignored: ['**/src-tauri/**', '**/target/**'] } },
  plugins: [react(), {
    name: 'retinue-preview-demo',
    apply: 'serve',
    transformIndexHtml: () => [{ tag: 'script', attrs: { type: 'module', src: '/src/preview/demoMenu.ts' }, injectTo: 'body' }],
  }],
  clearScreen: false,
  resolve: {
    alias: [
      { find: './pushBrowser', replacement: fixture('fixturePushBrowser.ts') },
      { find: /^\.\.\/tauri$/, replacement: fixture('fixtureBridge.ts') },
      { find: /^\.\.\/desktopIntegrations$/, replacement: fixture('fixtureDesktopIntegrations.ts') },
      { find: /^@tauri-apps\/api\/(event|window)$/, replacement: fixture('fixtureTauriApi.ts') },
    ],
  },
})
