// Appel Slack contre le cœur réel : serveur qui rejoue e2e/fixtures/appel-slack.jsonl
// (message `call` avec `app: "slack"`). Proposition seulement, démarrage manuel.
import { expect, test } from '@playwright/test';
import { PORTS } from '../ports';
import { go, main, nav, openMeeting } from './helpers';

test.use({ baseURL: `http://127.0.0.1:${PORTS.slack}` });

test('un appel Slack est proposé, puis enregistré avec son app', async ({ page }) => {
  // Contrat : `live_state.call` décrit l'appel en cours.
  const st = await (await page.request.post('/api/invoke/live_state', { data: {} })).json();
  expect(st.call).toEqual({ active: true, app: 'slack', app_name: 'Slack', title: 'Huddle dans #equipe-flotte' });
  expect(st).not.toHaveProperty('teams_call');

  await go(page, '/live');
  const live = main(page);
  await expect(live.getByText('Appel Slack détecté : Huddle dans #equipe-flotte')).toBeVisible();
  await expect(nav(page).getByText('Appel Slack détecté')).toBeVisible();
  await page.waitForTimeout(1500);
  await expect(live.getByRole('region', { name: 'Transcription en direct' })).toHaveCount(0);

  await live.getByRole('button', { name: 'Démarrer l’écoute' }).click();
  await expect(live.getByRole('heading', { name: 'Huddle dans #equipe-flotte' })).toBeVisible();
  await expect(live.getByRole('meter', { name: 'Slack' })).toBeVisible();
  const tx = live.getByRole('region', { name: 'Transcription en direct' });
  await expect(tx.getByText(/j'ouvre un ticket chez le fournisseur des cadenas|j’ouvre un ticket chez le fournisseur des cadenas/)).toBeAttached({ timeout: 30_000 });
  await live.getByRole('button', { name: 'Arrêter l’écoute' }).click();
  await expect(live.getByRole('button', { name: 'Arrêter l’écoute' })).toHaveCount(0);

  await openMeeting(page, /Huddle dans #equipe-flotte/);
  const detail = page.getByRole('region', { name: 'Détail de la réunion' });
  await expect(detail.getByTitle('Appel Slack')).toBeVisible();
  const list = page.getByRole('region', { name: 'Liste des réunions' });
  await expect(list.getByRole('link', { name: /Huddle dans #equipe-flotte/ }).getByText('Slack', { exact: true })).toBeVisible();
});
