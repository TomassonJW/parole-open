import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

// Réglages recommandés par Tauri 2 : port fixe, écran non effacé, chemins relatifs.
const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [react()],
  base: './',
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  server: {
    port: 1420,
    strictPort: true,
    host: host || '127.0.0.1',
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**', '**/src-core/**'] },
  },
  build: {
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: './tests/setup.ts',
    css: true,
  },
})
