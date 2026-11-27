// Captures de docs/screenshots : 1440×900 (×2), en clair et en sombre, contre le cœur réel
// (`scribe serve`, base neuve, faux CLI `claude`, simulateur audio). `npm run captures`.
import { expect, test, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { go, main, nav, openMeeting, passOnboarding } from '../reel/helpers';

const OUT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../../docs/screenshots');

/** Une seule capture, dans un seul thème, sans déplacer la souris (aperçu au survol). */
async function shotOne(page: Page, name: string, scheme: 'light' | 'dark') {
  await expect(page.locator('.toast')).toHaveCount(0, { timeout: 10_000 });
  await page.emulateMedia({ colorScheme: scheme });
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(OUT, `${name}-${scheme === 'light' ? 'clair' : 'sombre'}.png`), animations: 'disabled' });
  await page.emulateMedia({ colorScheme: 'light' });
}

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
  // On remplace les captures produites ici ; les comparaisons « avant-apres-* » sont conservées.
  fs.mkdirSync(OUT, { recursive: true });
  for (const f of fs.readdirSync(OUT)) if (/-(clair|sombre)\.png$/.test(f) && !f.startsWith('avant-apres-')) fs.rmSync(path.join(OUT, f));

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

  // Barre latérale réduite (⌘\), puis aperçu flottant au survol du bord gauche.
  await page.mouse.move(900, 500);
  await page.keyboard.press('ControlOrMeta+Backslash');
  await expect(nav(page)).toBeHidden();
  await page.waitForTimeout(400);
  await shotOne(page, 'sidebar-reduite', 'light');
  await page.mouse.move(3, 450);
  await expect(nav(page)).toBeVisible();
  await page.mouse.move(120, 600);
  await page.waitForTimeout(400);
  await shotOne(page, 'sidebar-apercu', 'light');
  await page.getByRole('button', { name: 'Épingler la barre latérale' }).click();
  await page.mouse.move(900, 500);
  await expect(page.getByRole('button', { name: 'Replier la barre latérale' })).toBeVisible();

  // Palette de commandes (⌘K).
  await page.locator('body').press('ControlOrMeta+k');
  const palette = page.getByRole('dialog', { name: 'Palette de commandes' });
  await expect(palette.getByRole('group', { name: 'Récemment ouvert' })).toBeVisible();
  await page.waitForTimeout(300);
  await shotOne(page, 'palette-commandes', 'dark');
  await page.keyboard.press('Escape');
  await expect(palette).toBeHidden();

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

  // Réglages → Audio & transcription : carte « Micro » (périphériques du simulateur), puis
  // « Apps de visio » (liste par défaut du cœur). Rechargement complet : les variantes
  // générées plus haut ne sont pas enregistrées.
  await go(page, '/settings?section=audio');
  await page.reload();
  const mic = page.getByRole('radiogroup', { name: 'Micro' });
  await expect(mic.getByText('Micro par défaut du système (Micro du MacBook Pro)')).toBeVisible();
  await mic.getByText('AirPods Pro', { exact: true }).click();
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(page.getByText('Modifications non enregistrées')).toHaveCount(0);
  await page.waitForTimeout(300);
  await shot(page, 'reglages-audio');

  await expect(page.getByRole('heading', { name: 'Apps de visio' })).toBeVisible();
  await expect(page.getByText(/capte tout le son du navigateur/)).toBeVisible();
  await page.getByRole('heading', { name: 'Apps de visio' }).evaluate((el) => el.scrollIntoView({ block: 'start' }));
  await page.waitForTimeout(300);
  await shot(page, 'reglages-apps');

  // Réglages → Agent IA : carte « Modèles et réflexion » (agents et modèles du cœur, faux CLI).
  const cur = await (await page.request.post('/api/invoke/get_settings', { data: {} })).json();
  cur.agent.tiers = {
    search: { agent: 'grok', model: 'grok-4.7', effort: 'medium' },
    summary: { agent: 'claude-code', model: 'opus', effort: 'high' },
    quick: { agent: '', model: '', effort: '' },
  };
  expect((await page.request.post('/api/invoke/save_settings', { data: { settings: cur } })).ok()).toBe(true);
  await go(page, '/settings?section=agent');
  await page.reload();
  const tiers = page.getByRole('group', { name: 'Modèles et réflexion' });
  const search = tiers.getByRole('group', { name: 'Recherches et corrélation' });
  await expect(search.getByLabel('Modèle', { exact: true })).toHaveValue('grok-4.7');
  await expect(tiers.getByRole('group', { name: 'Compte rendu' }).getByLabel('Réflexion')).toHaveValue('high');
  await page.getByRole('heading', { name: 'Modèles et réflexion' }).evaluate((el) => el.scrollIntoView({ block: 'start' }));
  await page.locator('.sbody').evaluate((el) => el.scrollBy(0, -120));
  await page.waitForTimeout(400);
  await shot(page, 'reglages-agent');

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
