import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri charge `dist/` en local (base relative) ; en dev, l'URL est fixe.
export default defineConfig({
  base: './',
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5173, strictPort: true, host: '127.0.0.1' },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'safari16',
    sourcemap: false,
    chunkSizeWarningLimit: 600,
  },
});
