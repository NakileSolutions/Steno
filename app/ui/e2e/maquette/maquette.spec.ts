// Parcours de l'interface contre le faux backend (vite preview, mock.ts).
import { expect, test, type Page } from '@playwright/test';

const nav = (page: Page) => page.getByRole('navigation', { name: 'Navigation principale' });
const main = (page: Page) => page.locator('main.content');

async function open(page: Page, route = '/meetings') {
  await page.goto(`/#${route}`);
  await expect(nav(page).getByRole('link', { name: 'Réunions', exact: true })).toBeVisible();
}

test.describe('navigation', () => {
  test('barre latérale', async ({ page }) => {
    await open(page);
    const sections: [string, RegExp, string][] = [
      ['En direct', /#\/live$/, 'Appel Teams détecté :'],
      ['Recherche', /#\/search/, 'Recherche'],
      ['Décisions & actions', /#\/actions$/, 'Décisions & actions'],
      ['Réglages', /#\/settings$/, 'Agent IA'],
      ['Réunions', /#\/meetings$/, 'Réunions'],
    ];
    for (const [label, url, heading] of sections) {
      await nav(page).getByRole('link', { name: label, exact: true }).click();
      await expect(page).toHaveURL(url);
      await expect(main(page).getByText(heading).first()).toBeVisible();
      await expect(nav(page).getByRole('link', { name: label, exact: true })).toHaveAttribute('aria-current', 'page');
    }
  });

  test('raccourcis ⌘1 à ⌘5', async ({ page }) => {
    await open(page);
    await page.locator('body').click({ position: { x: 900, y: 450 } });
    const expected = ['/meetings', '/live', '/search', '/actions', '/settings'];
    for (const [i, path] of [1, 2, 3, 4, 5].map((n, i) => [n, expected[i]] as const)) {
      await page.locator('body').press(`ControlOrMeta+${i}`);
      await expect(page).toHaveURL(new RegExp(`#${path}`));
      // La recherche donne le focus à son champ : on le quitte pour que le raccourci suivant s'applique.
      await page.keyboard.press('Escape');
    }
    await page.keyboard.press('ControlOrMeta+,');
    await expect(page).toHaveURL(/#\/settings/);
  });
});

test.describe('réunions', () => {
  test('liste, détail et onglets', async ({ page }) => {
    await open(page);
    const list = page.getByRole('region', { name: 'Liste des réunions' });
    await expect(list.getByRole('heading', { name: "Aujourd'hui" })).toBeVisible();
    await list.getByRole('link', { name: /Daily ORION/ }).click();
    await expect(page).toHaveURL(/#\/meetings\/daily-orion/);
    const detail = page.getByRole('region', { name: 'Détail de la réunion' });
    await expect(detail.getByRole('heading', { name: 'Daily ORION' })).toBeVisible();

    const tabs = detail.getByRole('tablist', { name: 'Sections de la réunion' });
    await expect(tabs.getByRole('tab', { name: /Compte rendu/ })).toHaveAttribute('aria-selected', 'true');
    await expect(detail.getByText('Liens avec le code et les outils').first()).toBeVisible();

    await tabs.getByRole('tab', { name: /Transcription/ }).click();
    await expect(page).toHaveURL(/tab=transcript/);
    await expect(detail.getByText('Scribe, cherche les erreurs Datadog sur ms-booking depuis hier soir.')).toBeVisible();
    // L'aparté est masqué par défaut.
    await expect(detail.getByText('Il faut que je pense à relancer Thomas')).toHaveCount(0);
    await detail.getByLabel(/Afficher les phrases privées/).check();
    await expect(detail.getByText('Il faut que je pense à relancer Thomas pour mon entretien annuel.')).toBeVisible();

    await tabs.getByRole('tab', { name: /Demandes/ }).click();
    await expect(detail.getByText('37 timeouts').first()).toBeVisible();

    await tabs.getByRole('tab', { name: /Notes/ }).click();
    await expect(detail.getByLabel('Nouvelle note')).toBeVisible();

    // Navigation au clavier entre les onglets.
    await tabs.getByRole('tab', { name: /Notes/ }).press('ArrowLeft');
    await expect(tabs.getByRole('tab', { name: /Demandes/ })).toHaveAttribute('aria-selected', 'true');
  });

  test('renommer un intervenant', async ({ page }) => {
    await open(page, '/meetings/daily-orion?tab=transcript');
    const detail = page.getByRole('region', { name: 'Détail de la réunion' });
    await detail.locator('.toolbar').getByRole('button', { name: 'Eux-2', exact: true }).click();
    const input = detail.getByLabel('Nom affiché pour Eux-2');
    await input.fill('Paul');
    await input.press('Enter');
    await expect(detail.locator('.toolbar').getByRole('button', { name: /^Paul\s*Eux-2$/ })).toBeVisible();
    await expect(detail.locator('.seg .who').getByRole('button', { name: /Paul/ }).first()).toBeVisible();
    // Conservé au rechargement du détail.
    await page.getByRole('region', { name: 'Liste des réunions' }).getByRole('link', { name: /Revue d'architecture/ }).click();
    await page.getByRole('region', { name: 'Liste des réunions' }).getByRole('link', { name: /Daily ORION/ }).click();
    await page.getByRole('tab', { name: /Transcription/ }).click();
    await expect(detail.locator('.toolbar').getByRole('button', { name: /Paul/ })).toBeVisible();
  });

  test('recherche plein texte', async ({ page }) => {
    await open(page, '/search');
    const box = page.getByLabel('Rechercher dans toutes les réunions');
    await expect(box).toBeFocused();
    await box.fill('Brindille');
    await expect(page).toHaveURL(/q=Brindille/);
    const results = main(page);
    await expect(results.getByRole('link', { name: /Revue d'architecture — export Brindille/ })).toBeVisible();
    await expect(results.locator('mark', { hasText: 'Brindille' }).first()).toBeVisible();
    await box.fill('zzzintrouvable');
    await expect(results.getByText('Aucun résultat pour « zzzintrouvable ».')).toBeVisible();

    // La touche / ouvre la recherche depuis n'importe où.
    await nav(page).getByRole('link', { name: 'Décisions & actions' }).click();
    await expect(main(page).getByRole('heading', { name: 'Décisions & actions' })).toBeVisible();
    await page.keyboard.press('/');
    await expect(page).toHaveURL(/#\/search/);
  });

  test('cocher une action comme faite', async ({ page }) => {
    await open(page, '/actions');
    const box = page.getByLabel('Fait : Ajouter la reprise sur erreur dans ms-booking');
    await expect(box).not.toBeChecked();
    await box.check();
    await expect(box).toBeChecked();
    // Filtre « À faire » : l'action disparaît, puis apparaît dans « Faites ».
    await expect(page.getByLabel('Fait : Ajouter la reprise sur erreur dans ms-booking')).toHaveCount(0, { timeout: 5000 });
    await page.getByLabel('Statut').selectOption('done');
    await expect(page.getByLabel('Fait : Ajouter la reprise sur erreur dans ms-booking')).toBeChecked();
    await page.getByRole('tab', { name: 'Décisions' }).click();
    await expect(page.getByText('pagination par 500').first()).toBeVisible();
  });
});

test.describe('écoute', () => {
  test('ne démarre jamais seule ; démarrage manuel et arrêt à la voix', async ({ page }) => {
    await open(page, '/live');
    const live = main(page);
    // Appel Teams simulé : simple proposition.
    await expect(live.getByText('Appel Teams détecté :')).toBeVisible();
    await expect(live.getByText('Refinement ORION — sprint 48')).toBeVisible();
    await expect(nav(page).getByText('Appel Teams détecté')).toBeVisible();
    await page.waitForTimeout(3500);
    await expect(live.getByText('Appel Teams détecté :')).toBeVisible();
    await expect(live.getByRole('region', { name: 'Transcription en direct' })).toHaveCount(0);

    await live.getByRole('button', { name: 'Démarrer l’écoute' }).click();
    await expect(live.getByRole('heading', { name: 'Refinement ORION — sprint 48' })).toBeVisible();
    await expect(live.getByText('Pour arrêter : bouton, ⌃⌥S, ou dites « Scribe, arrête l’écoute »')).toBeVisible();
    await expect(nav(page).getByRole('button', { name: 'Arrêter l’écoute' })).toBeVisible();
    await expect(live.getByText('On attaque le ticket APP-1260')).toBeVisible({ timeout: 8000 });

    // « Scribe, arrête l'écoute » : la réunion se termine.
    await page.evaluate(() => (window as unknown as { __scribeMock: { say(t: string): boolean } }).__scribeMock.say('Scribe, arrête l’écoute.'));
    await expect(live.getByRole('button', { name: 'Arrêter l’écoute' })).toHaveCount(0);
    await expect(live.getByText(/Traitement/).first()).toBeVisible();
    await expect(live.getByText('Appel Teams détecté :')).toBeVisible({ timeout: 8000 });
    await nav(page).getByRole('link', { name: 'Réunions', exact: true }).click();
    await page.getByRole('region', { name: 'Liste des réunions' }).getByRole('link', { name: /Refinement ORION/ }).click();
    await page.getByRole('tab', { name: /Demandes/ }).click();
    await expect(page.getByText('Arrêt de l’écoute')).toBeVisible();
  });

  test('arrêt par le bouton et demande tapée refusée si elle publie', async ({ page }) => {
    await open(page, '/live');
    await nav(page).getByRole('button', { name: 'Démarrer l’écoute' }).click();
    const live = main(page);
    await expect(live.getByRole('button', { name: 'Arrêter l’écoute' })).toBeVisible();
    await live.getByLabel('Demander au Scribe').fill('Poste ça sur Slack');
    await live.getByRole('button', { name: 'Envoyer' }).click();
    const card = live.locator('article.req', { hasText: 'Poste ça sur Slack' });
    await expect(card.getByText('Refusée', { exact: true })).toBeVisible();
    await live.getByRole('button', { name: 'Arrêter l’écoute' }).click();
    await expect(live.getByText(/Traitement/).first()).toBeVisible();
    await expect(nav(page).getByRole('button', { name: 'Démarrer l’écoute' })).toBeVisible({ timeout: 8000 });
  });
});

test.describe('réglages', () => {
  test('voix : avertissements, variantes et verbe d’arrêt', async ({ page }) => {
    await open(page, '/settings?section=voice');
    const wake = page.locator('#wake');
    await expect(wake).toHaveValue('Scribe');
    // « Scribe » apparaît une fois dans l'historique sans être une commande.
    await expect(page.getByText(/Ce mot apparaît 1 fois/)).toBeVisible();
    await wake.fill('Bonjour');
    await expect(page.getByText('Mot très courant en français')).toBeVisible();
    await wake.fill('Scribouillard');
    await expect(page.getByText('Aucun risque repéré.')).toBeVisible();
    await wake.fill('Scribe');
    await expect(page.getByText(/Ce mot apparaît 1 fois/)).toBeVisible();

    await page.getByRole('button', { name: 'Générer les variantes' }).click();
    const rejected = page.locator('.rejected');
    await expect(rejected.getByText('script', { exact: true })).toBeVisible();
    await expect(rejected.getByText('description', { exact: true })).toBeVisible();

    const variants = page.getByRole('group', { name: 'Variantes' });
    await page.getByLabel('Variantes : ajouter').fill('inscrit');
    await page.getByLabel('Variantes : ajouter').press('Enter');
    await expect(page.getByText('« inscrit » refusée')).toBeVisible();
    await expect(variants.getByText('inscrit', { exact: true })).toHaveCount(0);
    await page.getByLabel('Variantes : ajouter').fill('scraïbe');
    await page.getByLabel('Variantes : ajouter').press('Enter');
    await expect(variants.getByText('scraïbe')).toBeVisible();

    const stop = page.getByRole('group', { name: 'Verbes : Arrêter l’écoute' });
    await expect(page.getByText('Arrêter l’écoute', { exact: true })).toBeVisible();
    await expect(stop.getByText("arrête l'écoute")).toBeVisible();
    await expect(stop.getByText('fin de la réunion')).toBeVisible();
    await page.getByLabel('Verbes : Arrêter l’écoute : ajouter').fill('stop scribe');
    await page.getByLabel('Verbes : Arrêter l’écoute : ajouter').press('Enter');
    await page.getByRole('button', { name: 'Enregistrer' }).click();
    await expect(page.getByText('Réglages enregistrés.')).toBeVisible();
  });

  test('audio : plus de démarrage ni d’arrêt automatiques', async ({ page }) => {
    await open(page, '/settings?section=audio');
    await expect(page.getByText('Proposer de démarrer l’écoute quand un appel commence')).toBeVisible();
    await expect(page.getByText(/Démarrer automatiquement|Fin de réunion après/)).toHaveCount(0);
    await expect(page.getByText('L’écoute ne démarre jamais seule ; une fois lancée, le son capté est celui de l’app de l’appel en cours.')).toBeVisible();
  });

  test('apps de visio : désactiver, ajouter, supprimer, avertissement navigateur', async ({ page }) => {
    await open(page, '/settings?section=audio');
    await expect(page.getByRole('heading', { name: 'Apps de visio' })).toBeVisible();
    const apps = page.getByRole('group', { name: 'Apps de visio' });
    for (const name of ['Microsoft Teams', 'Slack', 'Zoom', 'Webex', 'Discord', 'FaceTime', 'Navigateur (Google Meet…)']) {
      await expect(apps.getByText(name, { exact: true })).toBeVisible();
    }
    await expect(apps.getByText('com.tinyspeck.slackmacgap')).toBeVisible();
    await expect(page.getByLabel('Proposer de démarrer l’écoute quand un appel commence')).toBeChecked();
    // Le navigateur est désactivé par défaut, avec un avertissement clair.
    await expect(page.getByLabel('Surveiller Navigateur (Google Meet…)')).not.toBeChecked();
    await expect(apps.getByText(/capte tout le son du navigateur, y compris les autres onglets/)).toBeVisible();

    // Désactiver Zoom.
    await expect(page.getByLabel('Surveiller Zoom')).toBeChecked();
    await page.getByLabel('Surveiller Zoom').uncheck();
    await expect(page.getByText('Modifications non enregistrées')).toBeVisible();

    // Les apps de la liste par défaut ne se suppriment pas.
    await expect(apps.getByRole('button', { name: /^Supprimer/ })).toHaveCount(0);

    // Ajouter une app : identifiant de bundle vérifié, identifiant généré.
    await apps.getByRole('button', { name: 'Ajouter une app' }).click();
    const form = apps.getByRole('form', { name: 'Ajouter une app' });
    await form.getByLabel('Nom').fill('Google Chat');
    await form.getByLabel('Identifiant de bundle').fill('pas un bundle');
    await form.getByRole('button', { name: 'Ajouter', exact: true }).click();
    await expect(form.getByRole('alert')).toContainText('n’est pas un identifiant de bundle');
    await form.getByLabel('Identifiant de bundle').fill('com.google.chat');
    await expect(form.getByText('google-chat')).toBeVisible();
    await form.getByRole('button', { name: 'Ajouter', exact: true }).click();
    await expect(apps.getByText('Google Chat', { exact: true })).toBeVisible();
    await expect(apps.getByText('com.google.chat')).toBeVisible();
    await expect(page.getByLabel('Surveiller Google Chat')).toBeChecked();

    await page.getByRole('button', { name: 'Enregistrer' }).click();
    await expect(page.getByText('Réglages enregistrés.')).toBeVisible();

    // Relu depuis le backend : Zoom désactivé, Google Chat ajouté (et supprimable).
    await page.evaluate(() => (location.hash = '#/meetings'));
    await expect(page.getByRole('region', { name: 'Liste des réunions' })).toBeVisible();
    await page.evaluate(() => (location.hash = '#/settings?section=audio'));
    await expect(page.getByLabel('Surveiller Zoom')).not.toBeChecked();
    await expect(page.getByLabel('Surveiller Google Chat')).toBeChecked();
    // Un appel Zoom n'est plus signalé.
    const zoom = await page.evaluate(() => (window as unknown as { __scribeMock: { call(a: string, t: string): boolean } }).__scribeMock.call('zoom', 'Point'));
    expect(zoom).toBe(false);

    await apps.getByRole('button', { name: 'Supprimer Google Chat' }).click();
    await expect(apps.getByText('Google Chat', { exact: true })).toHaveCount(0);
  });
});

test.describe('apps de visio', () => {
  test('un appel Slack simulé est proposé, jamais démarré seul', async ({ page }) => {
    await open(page, '/live');
    const live = main(page);
    await expect(live.getByText('Appel Teams détecté : Refinement ORION — sprint 48')).toBeVisible();
    await page.evaluate(() => (window as unknown as { __scribeMock: { call(a: string, t: string): boolean } }).__scribeMock.call('slack', 'Huddle dans #equipe-flotte'));
    await expect(live.getByText('Appel Slack détecté : Huddle dans #equipe-flotte')).toBeVisible();
    await expect(live.getByRole('button', { name: 'Démarrer l’écoute' })).toBeVisible();
    await expect(nav(page).getByText('Appel Slack détecté')).toBeVisible();
    await expect(live.getByText('Appel Teams détecté :')).toHaveCount(0);
    await page.waitForTimeout(1500);
    await expect(live.getByRole('region', { name: 'Transcription en direct' })).toHaveCount(0);

    // Démarrage manuel : la réunion garde l'app de l'appel.
    await live.getByRole('button', { name: 'Démarrer l’écoute' }).click();
    await expect(live.getByRole('heading', { name: 'Huddle dans #equipe-flotte' })).toBeVisible();
    await expect(live.getByRole('meter', { name: 'Slack' })).toBeVisible();
    await live.getByRole('button', { name: 'Arrêter l’écoute' }).click();
    await expect(live.getByText('Appel Slack détecté : Huddle dans #equipe-flotte')).toBeVisible({ timeout: 8000 });
  });

  test('navigateur : libellé et avertissement', async ({ page }) => {
    await open(page, '/settings?section=audio');
    await page.getByLabel('Surveiller Navigateur (Google Meet…)').check();
    await page.getByRole('button', { name: 'Enregistrer' }).click();
    await expect(page.getByText('Réglages enregistrés.')).toBeVisible();
    await page.evaluate(() => (window as unknown as { __scribeMock: { call(a: string, t: string): boolean } }).__scribeMock.call('browser', 'Meet — Point hebdo'));
    await nav(page).getByRole('link', { name: 'En direct', exact: true }).click();
    await expect(main(page).getByText('Appel détecté dans le navigateur : Meet — Point hebdo')).toBeVisible();
    await expect(main(page).getByText(/tout le son du navigateur sera capté, y compris les autres onglets/)).toBeVisible();
  });

  test('puce de l’app dans la liste et le détail', async ({ page }) => {
    await open(page);
    const list = page.getByRole('region', { name: 'Liste des réunions' });
    const huddle = list.getByRole('link', { name: /Huddle #equipe-flotte/ });
    await expect(huddle.getByText('Slack', { exact: true })).toBeVisible();
    await expect(list.getByRole('link', { name: /Revue d'architecture/ }).getByText('Zoom', { exact: true })).toBeVisible();
    await expect(list.getByRole('link', { name: /Point incident ms-pricing/ }).locator('.pill.app')).toHaveCount(0);
    await huddle.click();
    const detail = page.getByRole('region', { name: 'Détail de la réunion' });
    await expect(detail.getByRole('heading', { name: 'Huddle #equipe-flotte' })).toBeVisible();
    await expect(detail.getByTitle('Appel Slack')).toBeVisible();
  });
});

test('assistant de premier lancement', async ({ page }) => {
  await page.goto('/#/onboarding');
  await expect(page.getByRole('heading', { name: 'Permissions macOS' })).toBeVisible();
  await expect(page.getByText('Accessibilité')).toBeVisible();
  await page.getByRole('button', { name: 'Suivant' }).click();
  await expect(page.getByRole('heading', { name: 'Agent IA' })).toBeVisible();
  await page.getByRole('button', { name: 'Tester l’agent' }).click();
  await expect(page.getByText(/a répondu « OK »/)).toBeVisible();
  await page.getByRole('button', { name: 'Suivant' }).click();
  await expect(page.getByRole('heading', { name: 'Dépôts lisibles' })).toBeVisible();
  await page.getByRole('button', { name: 'Suivant' }).click();
  await expect(page.getByRole('heading', { name: 'Connecteurs' })).toBeVisible();
  await expect(page.getByText('atlassian')).toBeVisible();
  await expect(page.getByText(/connecteur masqué/)).toBeVisible();
  await page.getByRole('button', { name: 'Suivant' }).click();
  await expect(page.getByRole('heading', { name: 'Mot d’appel' })).toBeVisible();
  await expect(page.getByText(/arrête l’écoute/)).toBeVisible();
  await page.getByRole('button', { name: 'Suivant' }).click();
  await expect(page.getByRole('heading', { name: 'Corrélation' })).toBeVisible();
  await page.getByRole('button', { name: 'Précédent' }).click();
  await expect(page.getByRole('heading', { name: 'Mot d’appel' })).toBeVisible();
  await page.getByRole('button', { name: 'Suivant' }).click();
  await page.getByRole('button', { name: 'Terminer' }).click();
  await expect(page.getByText('Scribe est prêt.')).toBeVisible();
  await expect(page).toHaveURL(/#\/meetings/);
});

test.describe('panneau de barre de menu', () => {
  test.use({ viewport: { width: 360, height: 520 } });

  test('propose de démarrer, puis rappelle comment arrêter', async ({ page }) => {
    await page.goto('/#/tray');
    await expect(page.getByText('N’écoute pas', { exact: true })).toBeVisible();
    await expect(page.getByText('Appel Teams détecté :')).toBeVisible();
    await expect(page.getByText('Refinement ORION — sprint 48')).toBeVisible();
    await expect(page.getByText('Dernières réponses')).toBeVisible();
    await page.getByRole('button', { name: 'Démarrer l’écoute' }).click();
    await expect(page.getByRole('button', { name: 'Arrêter l’écoute' })).toBeVisible();
    await expect(page.getByText('Pour arrêter : bouton, ⌃⌥S, ou dites « Scribe, arrête l’écoute »')).toBeVisible();
    await expect(page.getByText('Appel Teams détecté :')).toHaveCount(0);

    await page.getByLabel('Demande rapide').fill('cherche le runbook de ms-pricing');
    await page.getByLabel('Demande rapide').press('Enter');
    await expect(page.locator('.recent').getByText('cherche le runbook de ms-pricing')).toBeVisible();

    await page.getByRole('button', { name: 'Arrêter l’écoute' }).click();
    await expect(page.getByText('Compte rendu en préparation…')).toBeVisible();
    await page.getByRole('button', { name: 'Ouvrir Scribe' }).click();
    await expect(page).toHaveURL(/#\/meetings/);
  });
});

test.describe('thème sombre', () => {
  test.use({ colorScheme: 'dark' });

  test('suit le réglage du système', async ({ page }) => {
    await open(page, '/meetings/daily-orion');
    const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(bg).toBe('rgb(23, 25, 31)');
    const text = await page.evaluate(() => getComputedStyle(document.body).color);
    expect(text).toBe('rgb(236, 235, 231)');
    const surface = await page.getByRole('region', { name: 'Détail de la réunion' }).evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(surface).toBe('rgb(30, 33, 41)');
  });

  test('clair par défaut', async ({ browser }) => {
    const ctx = await browser.newContext({ colorScheme: 'light' });
    const page = await ctx.newPage();
    await page.goto(test.info().project.use.baseURL + '/#/meetings');
    await expect(page.locator('body')).toHaveCSS('background-color', 'rgb(247, 246, 243)');
    await ctx.close();
  });
});
