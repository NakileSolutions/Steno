// Captures de docs/screenshots : 1440×900 (×2), en clair et en sombre, contre le cœur réel
// (`scribe serve`, base neuve, faux CLI `claude`, simulateur audio). `npm run captures`.
import { expect, test, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { go, main, nav, openMeeting, passOnboarding } from '../reel/helpers';

const OUT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../../docs/screenshots');

async function shot(page: Page, name: string) {
  await expect(page.locator('.toast')).toHaveCount(0, { timeout: 10_000 });
  for (const [scheme, suffix] of [['light', 'clair'], ['dark', 'sombre']] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForTimeout(250);
    await page.mouse.move(0, 899);
    await page.screenshot({ path: path.join(OUT, `${name}-${suffix}.png`), animations: 'disabled' });
  }
  await page.emulateMedia({ colorScheme: 'light' });
}

test('captures', async ({ page, browser }) => {
  fs.rmSync(OUT, { recursive: true, force: true });
  fs.mkdirSync(OUT, { recursive: true });

  // Assistant de premier lancement.
  await go(page, '/onboarding');
  await expect(page.locator('ul.doctor li').nth(2)).toBeVisible({ timeout: 20_000 });
  await shot(page, 'onboarding');
  await passOnboarding(page);

  // Panneau de la barre de menu : appel Teams détecté, rien n'écoute.
  const ctx = await browser.newContext({ viewport: { width: 360, height: 520 }, deviceScaleFactor: 2, locale: 'fr-FR', timezoneId: 'Europe/Paris' });
  const tray = await ctx.newPage();
  await go(tray, '/tray');
  await expect(tray.getByText('Appel Teams détecté :')).toBeVisible();
  await tray.waitForTimeout(500);
  await shot(tray, 'tray');
  await ctx.close();

  // En direct pendant l'écoute.
  await nav(page).getByRole('link', { name: 'En direct', exact: true }).click();
  const live = main(page);
  await expect(live.getByText('Appel Teams détecté : Daily ORION')).toBeVisible();
  await live.getByRole('button', { name: 'Démarrer l’écoute' }).click();
  const feed = live.getByRole('complementary', { name: 'Demandes au Scribe' });
  await live.getByLabel('Demander au Scribe').fill('Où en est le ticket APP-1234 ?');
  await live.getByRole('button', { name: 'Envoyer' }).click();
  await expect(live.getByLabel('Demander au Scribe')).toHaveValue('');
  await live.getByLabel('Demander au Scribe').fill('Poste ça sur Slack');
  await live.getByRole('button', { name: 'Envoyer' }).click();
  const voice = feed.locator('article.req', { hasText: 'les erreurs Datadog sur ms-booking' });
  await expect(voice.getByText('Terminée', { exact: true })).toBeVisible({ timeout: 90_000 });
  await page.waitForTimeout(1500);
  await shot(page, 'en-direct');

  const tx = live.getByRole('region', { name: 'Transcription en direct' });
  await expect(tx.getByText('Je relancerai le support Brindille cet après-midi.').first()).toBeAttached({ timeout: 90_000 });
  await live.getByRole('button', { name: 'Arrêter l’écoute' }).click();

  // Compte rendu corrélé.
  await openMeeting(page, /Daily ORION/);
  const detail = page.getByRole('region', { name: 'Détail de la réunion' });
  await expect(detail.locator('article.report')).toContainText('verrou de créneau', { timeout: 90_000 });
  await detail.getByRole('button', { name: 'Corréler' }).click();
  await expect(detail.getByLabel('Version du compte rendu')).toContainText('corrélée', { timeout: 90_000 });
  await page.waitForTimeout(500);
  await shot(page, 'detail-compte-rendu');

  await detail.getByRole('tab', { name: /Transcription/ }).click();
  await expect(detail.locator('ol.segs li').first()).toBeVisible();
  await detail.getByText('Scribe, cherche les erreurs Datadog sur ms-booking depuis hier.').evaluate((el) => el.scrollIntoView({ block: 'center' }));
  await shot(page, 'detail-transcription');

  await detail.getByRole('tab', { name: /Demandes/ }).click();
  await expect(detail.locator('article.req').first()).toBeVisible();
  await shot(page, 'detail-demandes');

  await nav(page).getByRole('link', { name: 'Recherche', exact: true }).click();
  await page.getByLabel('Rechercher dans toutes les réunions').fill('Brindille');
  await expect(main(page).locator('mark').first()).toBeVisible();
  await page.getByLabel('Rechercher dans toutes les réunions').blur();
  await shot(page, 'recherche');

  await nav(page).getByRole('link', { name: 'Décisions & actions', exact: true }).click();
  await page.getByLabel('Statut').selectOption('all');
  await expect(page.getByLabel(/^Fait : /).first()).toBeVisible();
  await page.getByLabel(/^Fait : .*Confluence/).check();
  await page.waitForTimeout(400);
  await shot(page, 'actions');

  await go(page, '/settings?section=voice');
  await page.getByRole('button', { name: 'Générer les variantes' }).click();
  await expect(page.locator('.rejected')).toBeVisible();
  await page.locator('#wake').blur();
  await shot(page, 'reglages-voix');

  // Réglages → Audio & transcription : section « Apps de visio » (liste par défaut du cœur).
  await go(page, '/settings?section=audio');
  await expect(page.getByRole('heading', { name: 'Apps de visio' })).toBeVisible();
  await expect(page.getByText(/capte tout le son du navigateur/)).toBeVisible();
  await page.waitForTimeout(300);
  await shot(page, 'reglages-apps');

  // Panneau de la barre de menu avec un appel Slack détecté (faux backend : même interface, sans `?backend=http`).
  const ctx2 = await browser.newContext({ viewport: { width: 360, height: 520 }, deviceScaleFactor: 2, locale: 'fr-FR', timezoneId: 'Europe/Paris' });
  const tray2 = await ctx2.newPage();
  await tray2.goto('/#/tray');
  await expect(tray2.getByText('Appel Teams détecté :')).toBeVisible();
  await tray2.evaluate(() => (window as unknown as { __scribeMock: { call(a: string, t: string): boolean } }).__scribeMock.call('slack', 'Huddle dans #equipe-flotte'));
  await expect(tray2.getByText('Appel Slack détecté : Huddle dans #equipe-flotte')).toBeVisible();
  await tray2.waitForTimeout(500);
  await shot(tray2, 'tray-slack');
  await ctx2.close();
});
