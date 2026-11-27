// Arrêt à la voix contre le cœur réel : second serveur qui rejoue e2e/fixtures/arret-voix.jsonl.
// La réunion doit se terminer seule après « Sténo, arrête l'écoute. ».
import { expect, test } from '@playwright/test';
import { PORTS } from '../ports';
import { go, main, nav, openMeeting } from './helpers';

test.use({ baseURL: `http://127.0.0.1:${PORTS.voix}` });

test('« Sténo, arrête l’écoute » termine la réunion', async ({ page }) => {
  await go(page, '/live');
  const live = main(page);
  await expect(live.getByText('Appel Teams détecté : Point mise en production')).toBeVisible();
  await live.getByRole('button', { name: 'Démarrer l’écoute' }).click();
  await expect(live.getByRole('heading', { name: 'Point mise en production' })).toBeVisible();

  // La phrase d'arrêt n'est affichée en direct qu'environ 0,3 s avant la fin de la réunion :
  // on la vérifie plus bas dans la transcription enregistrée (pas de course avec l'affichage).

  // Fin sans aucun clic : plus de bouton d'arrêt, l'appel Teams est de nouveau proposé.
  await expect(nav(page).getByRole('button', { name: 'Démarrer l’écoute' })).toBeVisible({ timeout: 20_000 });
  await expect(live.getByText('Appel Teams détecté : Point mise en production')).toBeVisible({ timeout: 60_000 });

  // Rien n'a été transcrit après l'arrêt.
  await openMeeting(page, /Point mise en production/);
  const detail = page.getByRole('region', { name: 'Détail de la réunion' });
  await detail.getByRole('tab', { name: /Transcription/ }).click();
  await expect(detail.getByText('Parfait, on déploie mardi matin comme prévu.')).toBeVisible();
  await expect(detail.getByText('Sténo, arrête l’écoute.').or(detail.getByText("Sténo, arrête l'écoute."))).toBeVisible();
  await expect(detail.getByText(/Phrase dite après l'arrêt/)).toHaveCount(0);
  await expect(detail.getByText('Encore une phrase après l’arrêt.')).toHaveCount(0);
  await expect(detail.locator('.badge.live')).toHaveCount(0);

  await detail.getByRole('tab', { name: /Demandes/ }).click();
  const stop = detail.locator('article.req', { hasText: 'Arrêt de l’écoute' });
  await expect(stop.getByText('Terminée', { exact: true })).toBeVisible();
});
