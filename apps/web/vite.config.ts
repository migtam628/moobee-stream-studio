import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  define: {
    __MOOBEE_BUILD_SHA__: JSON.stringify(process.env.GITHUB_SHA ?? 'local'),
    __MOOBEE_RELEASE_REF__: JSON.stringify(process.env.GITHUB_REF_NAME ?? 'dev'),
  },
  base: process.env.GITHUB_PAGES === 'true' ? '/moobee-stream-studio/' : '/',
  server: { host: '127.0.0.1', port: 5173, strictPort: true, proxy: { '/api': 'http://127.0.0.1:8787' } },
  preview: { host: '127.0.0.1' }
})
