import { defineConfig } from 'vite'
import { reactWithCompiler } from './vite.react'

export default defineConfig({
  plugins: [reactWithCompiler()],
  clearScreen: false,
  server: {
    watch: { ignored: ['**/src-tauri/**', '**/target/**'] }, strictPort: true, port: 1420 },
  envPrefix: ['VITE_', 'TAURI_'],
  build: { target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13' },
})
