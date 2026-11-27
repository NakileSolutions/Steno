// État des mises à jour, partagé par la barre latérale et Réglages → Mises à jour.

import { api, onEvent } from './api';
import type { UpdateStatus } from './types';
import { t } from './i18n.svelte';

export const updates = $state<{ status: UpdateStatus | null }>({ status: null });

let started = false;
export function initUpdates(): void {
  if (started) return;
  started = true;
  api.getUpdateStatus().then((s) => (updates.status = s)).catch(() => {});
  onEvent('steno://update', (s) => (updates.status = s));
}

/** Ce que la barre latérale doit signaler, ou `null`. */
export function updateBadge(s: UpdateStatus | null): { kind: 'available' | 'installing' | 'installed'; label: string } | null {
  if (!s) return null;
  if (s.state === 'installed') return { kind: 'installed', label: s.latest ? t('Redémarrer pour passer à {v}', { v: s.latest }) : t('Redémarrer pour passer à la nouvelle version') };
  if (s.state === 'installing') return { kind: 'installing', label: s.latest ? t('Installation de {v}…', { v: s.latest }) : t('Installation de la mise à jour…') };
  if (s.available && s.latest) return { kind: 'available', label: t('Sténo {v} est disponible', { v: s.latest }) };
  return null;
}
