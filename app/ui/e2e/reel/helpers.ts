import { expect, type Page } from '@playwright/test';

/** Ouvre une route de l'interface servie par `steno serve`, en mode backend HTTP. */
export async function go(page: Page, route: string): Promise<void> {
  await page.goto(`/?backend=http#${route}`);
}

export const nav = (page: Page) => page.getByRole('navigation', { name: 'Navigation principale' });
export const main = (page: Page) => page.locator('main.content');

/** Passe l'assistant de premier lancement, étape par étape. */
export async function passOnboarding(page: Page): Promise<void> {
  await go(page, '/onboarding');
  await expect(page.getByRole('heading', { name: 'Permissions macOS' })).toBeVisible();
  for (const next of ['Agent IA', 'Dépôts lisibles', 'Connecteurs', 'Mot d’appel', 'Corrélation']) {
    await page.getByRole('button', { name: 'Suivant' }).click();
    await expect(page.getByRole('heading', { name: next })).toBeVisible({ timeout: 20_000 });
  }
  await page.getByRole('button', { name: 'Terminer' }).click();
  await expect(page).toHaveURL(/#\/meetings/);
}

/** Va sur une réunion par son titre depuis la liste. */
export async function openMeeting(page: Page, title: string | RegExp): Promise<void> {
  await nav(page).getByRole('link', { name: 'Réunions', exact: true }).click();
  await page.getByRole('region', { name: 'Liste des réunions' }).getByRole('link', { name: title }).first().click();
  await expect(page.getByRole('region', { name: 'Détail de la réunion' }).getByRole('tablist')).toBeVisible();
}
