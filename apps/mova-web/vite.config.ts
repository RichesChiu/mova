import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

const backendTarget = process.env.MOVA_API_PROXY_TARGET ?? 'http://127.0.0.1:36080'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    port: 35173,
    proxy: {
      '/api': backendTarget,
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
  },
})
