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

test.describe('barre latérale façon Arc', () => {
  const width = async (page: Page) => (await main(page).boundingBox())!.width;

  test('réduire (⌘\\ et bouton), aperçu au survol, état mémorisé', async ({ page }) => {
    await open(page);
    await page.mouse.move(800, 450);
    const w0 = await width(page);
    await expect(page.getByRole('button', { name: 'Replier la barre latérale' })).toHaveAttribute('aria-expanded', 'true');

    // ⌘\ : la barre disparaît complètement (pas de bande d'icônes), le contenu prend toute la largeur.
    await page.locator('body').press('ControlOrMeta+Backslash');
    await expect(nav(page)).toBeHidden();
    await expect.poll(() => width(page)).toBeGreaterThan(w0 + 200);
    const show = page.getByRole('button', { name: 'Afficher la barre latérale' });
    await expect(show).toHaveAttribute('aria-expanded', 'false');

    // Bouton flottant : redéploie ; bouton de la barre : réduit.
    await show.click();
    await expect(nav(page)).toBeVisible();
    await expect.poll(() => width(page)).toBeLessThan(w0 + 1);
    await page.getByRole('button', { name: 'Replier la barre latérale' }).click();
    await expect(nav(page)).toBeHidden();
    const wide = await width(page);

    // Survol du bord gauche : la barre glisse par-dessus le contenu, sans le rétrécir.
    await page.mouse.move(400, 450);
    await page.mouse.move(3, 450);
    await expect(nav(page)).toBeVisible();
    await expect(page.getByRole('button', { name: 'Épingler la barre latérale' })).toBeVisible();
    expect(await width(page)).toBe(wide);
    // Elle se rétracte quand la souris s'en va…
    await page.mouse.move(900, 450);
    await expect(nav(page)).toBeHidden();

    // …et cliquer un élément de l'aperçu navigue puis le referme.
    await page.mouse.move(3, 450);
    await expect(nav(page)).toBeVisible();
    await nav(page).getByRole('link', { name: 'Recherche', exact: true }).click();
    await expect(page).toHaveURL(/#\/search/);
    await expect(nav(page)).toBeHidden();

    // Mémorisé au rechargement.
    await page.reload();
    await expect(page.getByRole('button', { name: 'Afficher la barre latérale' })).toBeVisible();
    await expect(nav(page)).toBeHidden();

    // Épingler depuis l'aperçu : la barre redevient déployée durablement.
    await page.mouse.move(400, 450);
    await page.mouse.move(3, 450);
    await page.getByRole('button', { name: 'Épingler la barre latérale' }).click();
    await page.mouse.move(900, 450);
    await expect(page.getByRole('button', { name: 'Replier la barre latérale' })).toHaveAttribute('aria-expanded', 'true');
    await page.waitForTimeout(400);
    await expect(nav(page)).toBeVisible();
    await page.reload();
    await expect(nav(page)).toBeVisible();
  });

  test('au clavier : le bouton flottant reste atteignable', async ({ page }) => {
    await open(page);
    await page.getByRole('button', { name: 'Replier la barre latérale' }).focus();
    await page.keyboard.press('Enter');
    const show = page.getByRole('button', { name: 'Afficher la barre latérale' });
    await expect(show).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('button', { name: 'Replier la barre latérale' })).toBeFocused();
    await expect(nav(page)).toBeVisible();
  });
});

test.describe('palette de commandes', () => {
  test('⌘K : chercher une réunion, l’ouvrir, Échap ferme', async ({ page }) => {
    await open(page);
    await page.locator('body').press('ControlOrMeta+k');
    const dlg = page.getByRole('dialog', { name: 'Palette de commandes' });
    await expect(dlg).toBeVisible();
    const box = dlg.getByRole('combobox', { name: 'Rechercher une réunion ou une commande' });
    await expect(box).toBeFocused();
    await expect(dlg.getByRole('group', { name: 'Actions' }).getByRole('option', { name: /Démarrer l’écoute/ })).toBeVisible();

    await box.fill('Brindille');
    const opt = dlg.getByRole('option', { name: /Revue d'architecture — export Brindille/ });
    await expect(opt).toBeVisible();
    await expect(opt).toHaveAttribute('aria-selected', 'true');
    await box.press('Enter');
    await expect(dlg).toBeHidden();
    await expect(page).toHaveURL(/#\/meetings\/revue-archi-brindille/);
    const detail = page.getByRole('region', { name: 'Détail de la réunion' });
    await expect(detail.getByRole('heading', { name: "Revue d'architecture — export Brindille" })).toBeVisible();

    // Récemment ouvert ; ↑↓ déplacent la sélection ; Échap ferme.
    // La page de la réunion peut encore être en transition : on rouvre la palette jusqu'à ce
    // qu'elle s'affiche (instable sinon sous Linux, en CI).
    await expect(async () => {
      if (!(await dlg.isVisible())) await page.locator('body').press('ControlOrMeta+k');
      await expect(dlg).toBeVisible({ timeout: 1000 });
    }).toPass({ timeout: 10_000 });
    await expect(dlg.getByRole('group', { name: 'Récemment ouvert' }).getByRole('option', { name: /Revue d'architecture/ })).toHaveAttribute('aria-selected', 'true');
    await box.press('ArrowDown');
    await expect(dlg.getByRole('option', { name: /Démarrer l’écoute/ })).toHaveAttribute('aria-selected', 'true');
    await box.press('ArrowUp');
    await box.press('ArrowUp');
    await expect(dlg.getByRole('option').last()).toHaveAttribute('aria-selected', 'true');
    await box.press('Escape');
    await expect(dlg).toBeHidden();

    // Depuis la barre latérale : aller à une section des réglages.
    await nav(page).getByRole('button', { name: /Rechercher…/ }).click();
    await expect(dlg).toBeVisible();
    await box.fill('voix');
    await expect(dlg.getByRole('option', { name: /Réglages › Voix/ })).toHaveAttribute('aria-selected', 'true');
    await box.press('Enter');
    await expect(page).toHaveURL(/#\/settings\?section=voice/);
    await expect(page.locator('#wake')).toBeVisible();
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
    // Compte rendu : « En bref » mis en avant, décisions et actions en cartes (cases à cocher).
    await expect(detail.getByRole('region', { name: 'En bref' })).toContainText('APP-1251 bloqué');
    await expect(detail.getByRole('region', { name: /Décisions/ })).toContainText('pagination par 500');
    const acts = detail.getByRole('region', { name: /Actions/ });
    const box = acts.getByRole('checkbox', { name: 'Ajouter la reprise sur erreur dans ms-booking' });
    await expect(box).not.toBeChecked();
    await box.check();
    await expect(box).toBeChecked();
    await box.uncheck();
    await expect(box).not.toBeChecked();

    await tabs.getByRole('tab', { name: /Transcription/ }).click();
    await expect(page).toHaveURL(/tab=transcript/);
    await expect(detail.getByText('Sténo, cherche les erreurs Datadog sur ms-booking depuis hier soir.')).toBeVisible();
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
    // Sans nom donné, « Eux-2 » s'affiche « Intervenant 2 ».
    await detail.locator('.toolbar').getByRole('button', { name: 'Intervenant 2', exact: true }).click();
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
    await expect(live.getByText('Pour arrêter : bouton, ⌃⌥S, ou dites « Sténo, arrête l’écoute »')).toBeVisible();
    await expect(nav(page).getByRole('button', { name: 'Arrêter l’écoute' })).toBeVisible();
    await expect(live.getByText('On attaque le ticket APP-1260')).toBeVisible({ timeout: 8000 });

    // « Sténo, arrête l'écoute » : la réunion se termine.
    await page.evaluate(() => (window as unknown as { __stenoMock: { say(t: string): boolean } }).__stenoMock.say('Sténo, arrête l’écoute.'));
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
    await live.getByLabel('Demander à Sténo').fill('Poste ça sur Slack');
    await live.getByRole('button', { name: 'Envoyer' }).click();
    const card = live.locator('article.req', { hasText: 'Poste ça sur Slack' });
    await expect(card.getByText('Refusée', { exact: true })).toBeVisible();
    await live.getByRole('button', { name: 'Arrêter l’écoute' }).click();
    await expect(live.getByText(/Traitement/).first()).toBeVisible();
    await expect(nav(page).getByRole('button', { name: 'Démarrer l’écoute' })).toBeVisible({ timeout: 8000 });
  });

  test('réunion en présentiel, pause, micro coupé et rappel aux participants', async ({ page }) => {
    await open(page, '/live');
    const live = main(page);
    await live.getByRole('button', { name: /présentiel/ }).click();
    await expect(live.getByText('En présentiel')).toBeVisible();
    // Pas de son d'app en présentiel : un seul indicateur de niveau (le micro).
    await expect(live.locator('.meters .meter')).toHaveCount(1);
    await expect(live.getByText('Prévenez les participants que la réunion est transcrite.')).toBeVisible();
    await live.getByRole('button', { name: 'Masquer le rappel' }).click();
    await expect(live.getByText('Prévenez les participants')).toHaveCount(0);
    await live.getByRole('button', { name: 'Pause' }).click();
    await expect(live.getByText(/Écoute en pause/)).toBeVisible();
    await live.getByRole('button', { name: 'Reprendre' }).click();
    await expect(live.getByText(/Écoute en pause/)).toHaveCount(0);
    await live.getByRole('button', { name: 'Couper mon micro' }).click();
    await expect(live.getByRole('button', { name: 'Micro coupé' })).toBeVisible();
    await live.getByRole('button', { name: 'Arrêter l’écoute' }).click();
  });
});

test.describe('réglages', () => {
  test('voix : avertissements, variantes et verbe d’arrêt', async ({ page }) => {
    await open(page, '/settings?section=voice');
    const wake = page.locator('#wake');
    await expect(wake).toHaveValue('Sténo');
    // « Sténo » apparaît une fois dans l'historique sans être une commande.
    await expect(page.getByText(/Ce mot apparaît 1 fois/)).toBeVisible();
    await wake.fill('Bonjour');
    await expect(page.getByText('Mot très courant en français')).toBeVisible();
    await wake.fill('Scribouillard');
    await expect(page.getByText('Aucun risque repéré.')).toBeVisible();
    await wake.fill('Sténo');
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
    await page.getByLabel('Verbes : Arrêter l’écoute : ajouter').fill('stop steno');
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
    const zoom = await page.evaluate(() => (window as unknown as { __stenoMock: { call(a: string, t: string): boolean } }).__stenoMock.call('zoom', 'Point'));
    expect(zoom).toBe(false);

    await apps.getByRole('button', { name: 'Supprimer Google Chat' }).click();
    await expect(apps.getByText('Google Chat', { exact: true })).toHaveCount(0);
  });

  test('agent IA : modèle et réflexion par usage, enregistrés', async ({ page }) => {
    await open(page, '/settings?section=agent');
    // Mention de l'auteur, discrète, en bas des Réglages.
    await expect(page.getByText(/^Sténo \d+\.\d+\.\d+ · Développé par Nakile Solutions$/)).toBeVisible();
    await expect(page.getByText('Modèle par niveau')).toHaveCount(0);

    const defaults = page.getByRole('radiogroup', { name: 'Agent par défaut' });
    for (const n of ['Claude Code', 'Codex', 'Antigravity', 'Grok Build']) await expect(defaults.getByText(n, { exact: true })).toBeVisible();
    await expect(defaults.locator('[data-agent="claude-code"] .alogo svg')).toBeVisible();

    const card = page.getByRole('group', { name: 'Modèles et réflexion' });
    const rows = card.locator('[data-tier]');
    await expect(rows).toHaveCount(3);
    await expect(rows.nth(0)).toHaveAttribute('aria-label', 'Recherches et corrélation');
    await expect(rows.nth(1)).toHaveAttribute('aria-label', 'Compte rendu');
    await expect(rows.nth(2)).toHaveAttribute('aria-label', 'Notes et récapitulatifs');
    await expect(card.getByText(/Les recherches consomment le plus/)).toBeVisible();

    const pickAgent = async (row: ReturnType<Page['getByRole']>, name: string) => {
      await row.getByRole('button', { name: /^Agent : / }).click();
      await page.getByRole('listbox', { name: 'Agent' }).getByRole('option', { name, exact: true }).click();
      await expect(row.getByRole('button', { name: `Agent : ${name}` })).toBeVisible();
    };

    // Compte rendu : Claude Code / opus / Élevée (Claude ne liste pas ses modèles : saisie libre et alias).
    const summary = card.getByRole('group', { name: 'Compte rendu' });
    await expect(summary.getByRole('button', { name: 'Agent : Agent par défaut (Claude Code)' })).toBeVisible();
    await pickAgent(summary, 'Claude Code');
    await summary.getByRole('button', { name: 'opus', exact: true }).click();
    await expect(summary.getByLabel('Modèle', { exact: true })).toHaveValue('opus');
    await expect(summary.locator('span.sub', { hasText: 'alias : dernière version' })).toBeVisible();
    await summary.getByLabel('Réflexion').selectOption({ label: 'Élevée' });
    await summary.getByRole('button', { name: 'Tester' }).click();
    await expect(summary.getByRole('status')).toContainText('modèle opus, réflexion élevée');
    await expect(summary.getByRole('status')).toContainText('(1.2 s)');

    // Recherches : Grok / grok-4.7 / Moyenne (Grok liste ses modèles : liste déroulante).
    const search = card.getByRole('group', { name: 'Recherches et corrélation' });
    await pickAgent(search, 'Grok Build');
    const model = search.getByLabel('Modèle', { exact: true });
    await expect(model.locator('option', { hasText: 'grok-4.7' })).toHaveCount(1);
    await model.selectOption('grok-4.7');
    await search.getByLabel('Réflexion').selectOption({ label: 'Moyenne' });
    await expect(search.getByLabel('Réflexion').locator('option')).toHaveText(['Défaut', 'Basse', 'Moyenne', 'Élevée', 'Très élevée']);

    // Notes : Codex avec un modèle libre.
    const quick = card.getByRole('group', { name: 'Notes et récapitulatifs' });
    await pickAgent(quick, 'Codex');
    await quick.getByLabel('Modèle', { exact: true }).fill('gpt-maison-rapide');

    await page.getByRole('button', { name: 'Enregistrer' }).click();
    await expect(page.getByText('Réglages enregistrés.')).toBeVisible();
    await page.reload();

    await expect(summary.getByRole('button', { name: 'Agent : Claude Code' })).toBeVisible();
    await expect(summary.getByLabel('Modèle', { exact: true })).toHaveValue('opus');
    await expect(summary.getByLabel('Réflexion')).toHaveValue('high');
    await expect(search.getByRole('button', { name: 'Agent : Grok Build' })).toBeVisible();
    await expect(search.getByLabel('Modèle', { exact: true })).toHaveValue('grok-4.7');
    await expect(search.getByLabel('Réflexion')).toHaveValue('medium');
    await expect(quick.getByRole('button', { name: 'Agent : Codex' })).toBeVisible();
    await expect(quick.getByLabel('Modèle', { exact: true })).toHaveValue('gpt-maison-rapide');
    await expect(page.getByText('Modifications non enregistrées')).toHaveCount(0);
  });

  test('micro : choisir les AirPods, enregistrer, recharger ; micro débranché signalé', async ({ page }) => {
    await open(page, '/settings?section=audio');
    const mic = page.getByRole('radiogroup', { name: 'Micro' });
    await expect(mic.getByText('Micro par défaut du système (Micro du MacBook Pro)')).toBeVisible();
    await expect(mic.getByRole('radio')).toHaveCount(4);
    await expect(mic.getByRole('radio').first()).toBeChecked();
    await expect(page.getByText(/même pendant l’écoute/)).toBeVisible();
    await mic.getByText('AirPods Pro', { exact: true }).click();
    await expect(mic.locator('[data-device="AirPodsPro-UID:input"] input')).toBeChecked();
    await page.getByRole('button', { name: 'Enregistrer' }).click();
    await expect(page.getByText('Réglages enregistrés.')).toBeVisible();

    await page.reload();
    await expect(mic.locator('[data-device="AirPodsPro-UID:input"] input')).toBeChecked();
    await expect(page.getByText(/débranché/)).toHaveCount(0);

    // Débranchement simulé : avertissement discret, puis la puce dans En direct.
    await page.evaluate(() => (window as unknown as { __stenoMock: { unplug(id: string): void } }).__stenoMock.unplug('AirPodsPro-UID:input'));
    await mic.getByRole('button', { name: 'Rafraîchir' }).click();
    await expect(page.getByText('AirPods Pro — débranché, micro par défaut utilisé')).toBeVisible();
    await expect(mic.locator('[data-device="AirPodsPro-UID:input"]')).toHaveCount(0);

    await nav(page).getByRole('link', { name: 'En direct', exact: true }).click();
    await main(page).getByRole('button', { name: 'Démarrer l’écoute' }).click();
    await expect(main(page).getByText('micro par défaut', { exact: true })).toBeVisible();
    await main(page).getByRole('button', { name: 'Arrêter l’écoute' }).click();
  });

  test('voix : sortie du retour sonore', async ({ page }) => {
    await open(page, '/settings?section=voice');
    const out = page.getByLabel('Jouer sur');
    await expect(out).toBeVisible();
    await expect(out.locator('option')).toHaveText(['Sortie par défaut (AirPods Pro)', 'AirPods Pro', 'Haut-parleurs du MacBook Pro']);
    const fb = page.getByRole('radiogroup', { name: 'Retour sonore' });
    await fb.getByRole('radio', { name: 'Casque uniquement' }).click();
    await expect(page.getByText('Joué seulement si la sortie par défaut est un casque.')).toBeVisible();
    await out.selectOption({ label: 'Haut-parleurs du MacBook Pro' });
    await expect(page.getByText('Joué seulement si la sortie par défaut est un casque.')).toHaveCount(0);
    await fb.getByRole('radio', { name: 'Aucun' }).click();
    await expect(page.getByLabel('Jouer sur')).toHaveCount(0);
  });

  test('agent IA : changer d’agent remet à vide ce qui n’existe plus', async ({ page }) => {
    await open(page, '/settings?section=agent');
    const card = page.getByRole('group', { name: 'Modèles et réflexion' });
    const pickAgent = async (row: ReturnType<Page['getByRole']>, name: string) => {
      await row.getByRole('button', { name: /^Agent : / }).click();
      await page.getByRole('listbox', { name: 'Agent' }).getByRole('option', { name, exact: true }).click();
      await expect(row.getByRole('button', { name: `Agent : ${name}` })).toBeVisible();
    };
    const search = card.getByRole('group', { name: 'Recherches et corrélation' });

    // Antigravity : modèle de sa liste et réflexion « Maximale ».
    await pickAgent(search, 'Antigravity');
    const model = search.getByLabel('Modèle', { exact: true });
    await expect(model.locator('option', { hasText: 'gemini-3.5-flash' })).toHaveCount(1);
    await model.selectOption('gemini-3.5-flash');
    await search.getByLabel('Réflexion').selectOption({ label: 'Maximale' });

    // Grok : le modèle d'Antigravity et « max » n'existent pas, remis à vide.
    await pickAgent(search, 'Grok Build');
    await expect(search.getByLabel('Modèle', { exact: true })).toHaveValue('');
    await expect(search.getByLabel('Réflexion')).toHaveValue('');

    // « Autre… » : saisie libre, gardée au changement d'agent ; l'effort commun aussi.
    await search.getByLabel('Modèle', { exact: true }).selectOption({ label: 'Autre…' });
    await search.getByLabel('Autre modèle').fill('grok-5-preview');
    await search.getByLabel('Réflexion').selectOption({ label: 'Élevée' });
    await pickAgent(search, 'Claude Code');
    await expect(search.getByLabel('Modèle', { exact: true })).toHaveValue('grok-5-preview');
    await expect(search.getByLabel('Réflexion')).toHaveValue('high');

    // Agent sans niveaux de réflexion : contrôle désactivé ; agent non installé sélectionnable mais marqué.
    await search.getByRole('button', { name: /^Agent : / }).click();
    await expect(page.getByRole('option', { name: /Gemini CLI/ })).toContainText('non installé');
    await page.getByRole('option', { name: /Gemini CLI/ }).click();
    await expect(search.getByLabel('Réflexion')).toBeDisabled();
    await expect(search.getByLabel('Réflexion')).toHaveValue('');
    await expect(search.getByText('Non réglable pour cet agent.')).toBeVisible();
  });
});

test.describe('apps de visio', () => {
  test('un appel Slack simulé est proposé, jamais démarré seul', async ({ page }) => {
    await open(page, '/live');
    const live = main(page);
    await expect(live.getByText('Appel Teams détecté : Refinement ORION — sprint 48')).toBeVisible();
    await page.evaluate(() => (window as unknown as { __stenoMock: { call(a: string, t: string): boolean } }).__stenoMock.call('slack', 'Huddle dans #equipe-flotte'));
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
    // L'arrêt ouvre la page de la réunion : rédaction différée, « Rédiger maintenant ».
    await expect(page.getByRole('heading', { name: /Rédaction du compte rendu dans/ })).toBeVisible();
    await page.getByRole('button', { name: 'Rédiger maintenant' }).click();
    await expect(page.locator('article.report')).toBeVisible({ timeout: 8000 });
    await open(page, '/live');
    await expect(live.getByText('Appel Slack détecté : Huddle dans #equipe-flotte')).toBeVisible({ timeout: 8000 });
  });

  test('navigateur : libellé et avertissement', async ({ page }) => {
    await open(page, '/settings?section=audio');
    await page.getByLabel('Surveiller Navigateur (Google Meet…)').check();
    await page.getByRole('button', { name: 'Enregistrer' }).click();
    await expect(page.getByText('Réglages enregistrés.')).toBeVisible();
    await page.evaluate(() => (window as unknown as { __stenoMock: { call(a: string, t: string): boolean } }).__stenoMock.call('browser', 'Meet — Point hebdo'));
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
  const perms = page.locator('.perms');
  await expect(perms.getByText('Accessibilité', { exact: true })).toBeVisible();
  // Droit refusé : bouton vers la bonne page des Réglages Système ; pas encore demandé : « Autoriser ».
  await expect(perms.locator('li.denied').getByRole('button', { name: 'Ouvrir les Réglages Système' })).toBeVisible();
  await expect(perms.locator('li.unknown').getByRole('button', { name: 'Autoriser' })).toBeVisible();
  await page.getByRole('button', { name: 'Suivant' }).click();
  await expect(page.getByRole('heading', { name: 'Agent IA' })).toBeVisible();
  const agents = page.getByRole('radiogroup', { name: 'Agent IA' });
  for (const n of ['Claude Code', 'Codex', 'Antigravity', 'Grok Build']) await expect(agents.getByText(n, { exact: true })).toBeVisible();
  await expect(agents.locator('.alogo svg')).toHaveCount(4);
  await expect(page.getByRole('link', { name: 'Choisir les modèles plus tard dans Réglages' })).toBeVisible();
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
  await expect(page.getByText('Sténo est prêt.')).toBeVisible();
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
    await expect(page.getByText('Pour arrêter : bouton, ⌃⌥S, ou dites « Sténo, arrête l’écoute »')).toBeVisible();
    await expect(page.getByText('Appel Teams détecté :')).toHaveCount(0);

    await page.getByLabel('Demande rapide').fill('cherche le runbook de ms-pricing');
    await page.getByLabel('Demande rapide').press('Enter');
    await expect(page.locator('.recent').getByText('cherche le runbook de ms-pricing')).toBeVisible();

    await page.getByRole('button', { name: 'Arrêter l’écoute' }).click();
    await expect(page.getByText('Compte rendu en préparation…')).toBeVisible();
    await page.getByRole('button', { name: 'Ouvrir Sténo' }).click();
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


test('contexte, documents et format perso pendant la réunion ; arrêt en un clic', async ({ page }) => {
  await open(page, '/live');
  await nav(page).getByRole('button', { name: 'Démarrer l’écoute' }).click();
  const live = main(page);
  await live.getByRole('button', { name: 'Contexte' }).click();
  const dlg = page.getByRole('dialog', { name: 'Contexte du compte rendu' });
  await expect(dlg).toBeVisible();
  await dlg.getByLabel('Contexte').fill('Point Nova : surveiller la TVA belge.');
  await dlg.locator('input[type=file]').setInputFiles({ name: 'regles-tva.md', mimeType: 'text/markdown', buffer: Buffer.from('TVA Belgique : 19 %') });
  await expect(dlg.getByText('regles-tva.md')).toBeVisible();
  await dlg.locator('input[type=file]').setInputFiles({ name: 'photo.png', mimeType: 'image/png', buffer: Buffer.from('x') });
  await expect(page.getByText(/Format non pris en charge/)).toBeVisible();
  // Nouveau format perso, enregistré et sélectionné.
  await dlg.getByLabel('Format du compte rendu').selectOption('__new');
  await dlg.getByLabel('Nom du format').fill('Note COMEX bis');
  await dlg.getByLabel('Consignes du format').fill('Deux puces, ton exécutif.');
  await dlg.getByRole('button', { name: 'Enregistrer et utiliser' }).click();
  await expect(dlg.getByLabel('Format du compte rendu')).toHaveValue('perso-note-comex-bis');
  await dlg.getByRole('button', { name: 'Terminé' }).click();
  await expect(dlg).toBeHidden();
  // Réouverture : tout est conservé.
  await live.getByRole('button', { name: 'Contexte' }).click();
  await expect(dlg.getByLabel('Contexte')).toHaveValue('Point Nova : surveiller la TVA belge.');
  await dlg.getByRole('button', { name: 'Terminé' }).click();
  // Par défaut, l'arrêt se fait en un clic, sans fenêtre.
  await live.getByRole('button', { name: 'Arrêter l’écoute' }).click();
  await expect(page.getByRole('dialog', { name: 'Arrêter l’écoute' })).toHaveCount(0);
  await expect(live.getByRole('button', { name: 'Arrêter l’écoute' })).toHaveCount(0);
});

test('option : fenêtre de contexte à l’arrêt, Entrée pour arrêter directement', async ({ page }) => {
  await open(page, '/settings?section=reports');
  await page.getByRole('switch', { name: /Proposer contexte, documents et format à l’arrêt/ }).check();
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  await open(page, '/live');
  await nav(page).getByRole('button', { name: 'Démarrer l’écoute' }).click();
  const live = main(page);
  await live.getByRole('button', { name: 'Arrêter l’écoute' }).click();
  const dlg = page.getByRole('dialog', { name: 'Arrêter l’écoute' });
  await expect(dlg.getByRole('button', { name: 'Arrêter et rédiger' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(live.getByRole('button', { name: 'Arrêter l’écoute' })).toHaveCount(0);
});

test('réglages : formats de compte rendu perso', async ({ page }) => {
  await open(page, '/settings?section=reports');
  await expect(page.getByLabel('Format par défaut')).toHaveValue('standard');
  await expect(page.getByLabel('Nom du format').first()).toHaveValue('Note COMEX');
  await page.getByRole('button', { name: 'Nouveau format' }).click();
  await page.getByLabel('Nom du format').last().fill('Résumé exécutif');
  await page.getByLabel(/Consignes du format/).last().fill('Cinq lignes au plus.');
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(page.getByLabel('Format par défaut').locator('option', { hasText: 'Résumé exécutif' })).toHaveCount(1);
  await page.getByLabel('Format par défaut').selectOption({ label: 'Mail de synthèse' });
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(page.getByLabel('Format par défaut')).toHaveValue('mail');
});

test('mises à jour : nouvelle version signalée, installée puis redémarrage', async ({ page }) => {
  await open(page, '/meetings');
  const badge = nav(page).getByRole('link', { name: 'Sténo 0.2.0 est disponible' });
  await expect(badge).toBeVisible();
  await badge.click();
  await expect(page.getByText('Version installée :')).toContainText('0.1.0');
  await expect(page.getByText('Contexte et documents dans le compte rendu')).toBeVisible();
  await page.getByRole('button', { name: 'Installer 0.2.0' }).click();
  const restart = nav(page).getByRole('button', { name: /Redémarrer pour passer à 0.2.0/ });
  await expect(restart).toBeVisible({ timeout: 8000 });
  await restart.click();
  await expect(page.getByText('Version installée :')).toContainText('0.2.0');
  await expect(nav(page).getByText(/0\.2\.0/)).toHaveCount(0);
});

test('interface en anglais : Réglages → Profil → Langue', async ({ page }) => {
  await open(page, '/settings?section=profile');
  await page.getByLabel('Langue').selectOption('en');
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  const enNav = page.getByRole('navigation', { name: 'Main navigation' });
  await expect(enNav.getByRole('link', { name: 'Meetings', exact: true })).toBeVisible();
  await expect(enNav.getByRole('link', { name: 'Settings', exact: true })).toBeVisible();
  await enNav.getByRole('link', { name: 'Meetings', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Meetings', exact: true })).toBeVisible();
  // Retour au français (la langue choisie est conservée au rechargement).
  await enNav.getByRole('link', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Profile' }).or(page.getByRole('link', { name: 'Profile' })).first().click();
  await page.getByLabel('Language').selectOption('fr');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(nav(page).getByRole('link', { name: 'Réunions', exact: true })).toBeVisible();
});

test('phase 3 : réunion du calendrier préparée avec l’IA puis démarrée', async ({ page }) => {
  await open(page, '/meetings');
  const list = page.getByRole('region', { name: 'Liste des réunions' });
  await expect(list.getByRole('heading', { name: 'À venir' })).toBeVisible();
  const ev = list.locator('.mitem.event', { hasText: 'Revue Nova — v2' });
  await expect(ev.getByText('2 invités')).toBeVisible();
  await ev.getByRole('button', { name: 'Préparer' }).click();

  const detail = page.getByRole('region', { name: 'Détail de la réunion' });
  await expect(detail.getByRole('heading', { name: 'Revue Nova — v2' })).toBeVisible();
  await expect(detail.getByText('Réunion prévue')).toBeVisible();
  await expect(detail.getByText('Inès Martin')).toBeVisible();
  await expect(detail.getByText('Paul Durand')).toBeVisible();
  await expect(detail.getByLabel('Contexte')).toHaveValue(/Ordre du jour : périmètre de la v2/);

  // Participant ajouté à la main.
  await detail.getByLabel('Nom du participant').fill('Sophie Bernard');
  await detail.getByRole('button', { name: 'Ajouter', exact: true }).click();
  await expect(detail.getByText('Sophie Bernard')).toBeVisible();

  // Préparation guidée.
  await detail.getByRole('button', { name: 'Préparer avec l’IA' }).click();
  await detail.getByLabel('Quel est l’objectif principal de cette réunion ?').fill('Valider la date de sortie de la v2');
  await detail.getByRole('button', { name: 'Rédiger le contexte' }).click();
  await expect(detail.getByLabel('Contexte')).toHaveValue(/Valider la date de sortie de la v2/);

  // La réunion n'apparaît plus comme événement libre : elle est prévue.
  await expect(list.locator('.mitem.planned', { hasText: 'Revue Nova — v2' })).toBeVisible();
  await expect(list.locator('.mitem.event', { hasText: 'Revue Nova — v2' })).toHaveCount(0);

  // Démarrage : l'écoute reprend la réunion prévue.
  await detail.getByRole('button', { name: 'Démarrer l’écoute' }).click();
  await expect(main(page).getByRole('heading', { name: 'Revue Nova — v2' })).toBeVisible();
  await main(page).getByRole('button', { name: 'Arrêter l’écoute' }).click();
});

test('phase 3 : prévoir une réunion à la main', async ({ page }) => {
  await open(page, '/meetings');
  await page.getByRole('button', { name: 'Prévoir' }).click();
  const dlg = page.getByRole('dialog', { name: 'Prévoir une réunion' });
  await dlg.getByLabel('Titre de la réunion').fill('Atelier tarifs');
  await dlg.getByRole('button', { name: 'Prévoir' }).click();
  await expect(page.getByRole('region', { name: 'Détail de la réunion' }).getByRole('heading', { name: 'Atelier tarifs' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Liste des réunions' }).locator('.mitem.planned', { hasText: 'Atelier tarifs' })).toBeVisible();
});
