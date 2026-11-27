// Scénario complet contre le cœur réel : `scribe serve` sur une base neuve, faux CLI `claude`,
// simulateur audio qui rejoue fixtures/daily-45min.jsonl (×40) une fois l'écoute démarrée.
import { expect, test } from '@playwright/test';
import fs from 'node:fs';
import { go, main, nav, openMeeting, passOnboarding } from './helpers';

test.describe.configure({ mode: 'serial' });

test('réunion complète : écoute manuelle, demandes, compte rendu, corrélation, recherche, export', async ({ page }) => {
  await passOnboarding(page);

  // L'appel Teams est signalé, mais rien ne démarre seul.
  const st = await (await page.request.post('/api/invoke/live_state', { data: {} })).json();
  expect(st.call).toEqual({ active: true, app: 'teams', app_name: 'Microsoft Teams', title: 'Daily ORION' });
  await nav(page).getByRole('link', { name: 'En direct', exact: true }).click();
  const live = main(page);
  await expect(live.getByText('Appel Teams détecté : Daily ORION')).toBeVisible();
  await expect(nav(page).getByText('Appel Teams détecté')).toBeVisible();
  await page.waitForTimeout(2000);
  await expect(live.getByRole('region', { name: 'Transcription en direct' })).toHaveCount(0);

  await live.getByRole('button', { name: 'Démarrer l’écoute' }).click();
  await expect(live.getByRole('heading', { name: 'Daily ORION' })).toBeVisible();
  await expect(live.getByText(/Pour arrêter : bouton, ⌃⌥S, ou dites « Scribe, arrête l’écoute »/)).toBeVisible();

  // La transcription défile.
  const tx = live.getByRole('region', { name: 'Transcription en direct' });
  await expect(tx.getByText('Bonjour à tous, on commence la daily ?')).toBeAttached({ timeout: 20_000 });
  await expect(tx.getByText('le verrou de créneau côté ms-booking').first()).toBeAttached({ timeout: 20_000 });

  // Demande vocale : « Scribe, cherche les erreurs Datadog sur ms-booking… ».
  const feed = live.getByRole('complementary', { name: 'Demandes au Scribe' });
  const voice = feed.locator('article.req', { hasText: 'les erreurs Datadog sur ms-booking' });
  await expect(voice).toBeVisible({ timeout: 30_000 });
  await expect(voice.getByText('Terminée', { exact: true })).toBeVisible({ timeout: 60_000 });
  await expect(voice).toContainText('src/SlotLock.kt:42');

  // Demande tapée.
  const ask = live.getByLabel('Demander au Scribe');
  await ask.fill('Où est géré le verrou de créneau ?');
  await live.getByRole('button', { name: 'Envoyer' }).click();
  const typed = feed.locator('article.req', { hasText: 'Où est géré le verrou de créneau ?' });
  await expect(typed.getByText('Terminée', { exact: true })).toBeVisible({ timeout: 60_000 });
  await expect(typed).toContainText(/SlotLock|verrou/);

  // Demande d'écriture : refusée.
  await ask.fill('Poste ça sur Slack');
  await live.getByRole('button', { name: 'Envoyer' }).click();
  const slack = feed.locator('article.req', { hasText: 'Poste ça sur Slack' });
  await expect(slack.getByText('Refusée', { exact: true })).toBeVisible({ timeout: 60_000 });

  // On laisse passer la décision, l'action de Marie et la note vocale, puis on arrête.
  await expect(tx.getByText('Je relancerai le support Brindille cet après-midi.').first()).toBeAttached({ timeout: 90_000 });
  await live.getByRole('button', { name: 'Arrêter l’écoute' }).click();
  await expect(live.getByRole('button', { name: 'Arrêter l’écoute' })).toHaveCount(0);

  // Compte rendu.
  await openMeeting(page, /Daily ORION/);
  const detail = page.getByRole('region', { name: 'Détail de la réunion' });
  await expect(detail.getByTitle('Appel Teams')).toBeVisible();
  const report = detail.locator('article.report');
  await expect(report).toContainText('verrou de créneau', { timeout: 90_000 });
  await expect(report).toContainText('Marie');
  const actions = detail.locator('ul.actions');
  await expect(actions.locator('li', { hasText: 'APP-1234' })).toContainText('Marie');

  // Corrélation → version 2.
  await detail.getByRole('button', { name: 'Corréler' }).click();
  await expect(detail.getByRole('heading', { name: 'Liens avec le code et les outils' }).first()).toBeVisible({ timeout: 90_000 });
  await expect(detail.getByLabel('Version du compte rendu')).toContainText('Version 2 · corrélée');
  await expect(detail.getByText('Corrélée').first()).toBeVisible();

  // Transcription et demandes de la réunion.
  await detail.getByRole('tab', { name: /Demandes/ }).click();
  await expect(detail.locator('article.req', { hasText: 'Poste ça sur Slack' }).getByText('Refusée', { exact: true })).toBeVisible();

  // Export markdown.
  await detail.getByRole('tab', { name: /Compte rendu/ }).click();
  const [download] = await Promise.all([
    page.waitForEvent('download'),
    detail.getByRole('button', { name: 'Exporter en markdown' }).click(),
  ]);
  expect(download.suggestedFilename()).toMatch(/daily-orion\.md$/);
  const md = fs.readFileSync((await download.path())!, 'utf8');
  expect(md).toContain('verrou de créneau');
  await expect(page.getByText('Export markdown copié et téléchargé.')).toBeVisible();

  // Recherche plein texte.
  await nav(page).getByRole('link', { name: 'Recherche', exact: true }).click();
  await page.getByLabel('Rechercher dans toutes les réunions').fill('Brindille');
  await expect(main(page).getByRole('link', { name: /Daily ORION/ }).first()).toBeVisible();
  await expect(main(page).locator('mark', { hasText: 'Brindille' }).first()).toBeVisible();

  // Décisions & actions : cocher une action.
  await nav(page).getByRole('link', { name: 'Décisions & actions', exact: true }).click();
  const box = page.getByLabel(/^Fait : .*APP-1234/);
  await expect(box).not.toBeChecked();
  await box.check();
  await expect(box).toBeChecked();
  await page.getByLabel('Statut').selectOption('done');
  await expect(page.getByLabel(/^Fait : .*APP-1234/)).toBeChecked();
  await page.getByRole('tab', { name: 'Décisions' }).click();
  await expect(page.getByText(/verrou de créneau/).first()).toBeVisible();

  // L'état persiste côté serveur (rechargement complet).
  await page.reload();
  await page.getByLabel('Statut').selectOption('done');
  await expect(page.getByLabel(/^Fait : .*APP-1234/)).toBeChecked();
});

test('réglages : apps de visio enregistrées par le cœur', async ({ page }) => {
  await go(page, '/settings?section=audio');
  const apps = page.getByRole('group', { name: 'Apps de visio' });
  for (const name of ['Microsoft Teams', 'Slack', 'Zoom', 'Webex', 'Discord', 'FaceTime', 'Navigateur (Google Meet…)']) {
    await expect(apps.getByText(name, { exact: true })).toBeVisible();
  }
  await expect(page.getByLabel('Surveiller Navigateur (Google Meet…)')).not.toBeChecked();
  await expect(apps.getByText(/capte tout le son du navigateur/)).toBeVisible();
  await page.getByLabel('Surveiller Zoom').uncheck();
  await apps.getByRole('button', { name: 'Ajouter une app' }).click();
  const form = apps.getByRole('form', { name: 'Ajouter une app' });
  await form.getByLabel('Nom').fill('Google Chat');
  await form.getByLabel('Identifiant de bundle').fill('com.google.chat');
  await form.getByRole('button', { name: 'Ajouter', exact: true }).click();
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(page.getByText('Réglages enregistrés.')).toBeVisible();

  await page.reload();
  await expect(page.getByLabel('Surveiller Zoom')).not.toBeChecked();
  await expect(page.getByLabel('Surveiller Google Chat')).toBeChecked();
  const s = await (await page.request.post('/api/invoke/get_settings', { data: {} })).json();
  expect(s.audio.auto_detect_calls).toBe(true);
  expect(s.audio.call_apps.find((a: { id: string }) => a.id === 'google-chat')).toMatchObject({ name: 'Google Chat', bundles: ['com.google.chat'], enabled: true, browser: false });

  // Remise en l'état.
  await page.getByLabel('Surveiller Zoom').check();
  await apps.getByRole('button', { name: 'Supprimer Google Chat' }).click();
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(page.getByText('Réglages enregistrés.')).toBeVisible();
});
