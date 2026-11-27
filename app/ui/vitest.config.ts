import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Dates déterministes : les libellés (« Aujourd'hui », heures) dépendent du fuseau.
process.env.TZ = 'Europe/Paris';

export default defineConfig({
  plugins: [svelte()],
  resolve: { conditions: ['browser'] },
  test: {
    environment: 'jsdom',
    // Agent utilisateur de macOS : les textes et valeurs par défaut d'origine (`platform.ts`).
    // Les variantes Linux se testent en passant la plateforme explicitement.
    environmentOptions: {
      jsdom: { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) jsdom' },
    },
    include: ['src/**/*.test.ts'],
    restoreMocks: true,
  },
});
