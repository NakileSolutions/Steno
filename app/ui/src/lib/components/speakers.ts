// Couleur et nom affiché par intervenant.
import type { Speaker } from '../types';
import { t } from '../i18n.svelte';

export function speakerColor(label: string): string {
  if (label === 'Moi') return 'var(--sp-me)';
  const n = parseInt(label.replace(/\D+/g, ''), 10);
  if (Number.isNaN(n)) return 'var(--sp-1)';
  return `var(--sp-${((n - 1) % 5) + 1})`;
}

export function speakerName(label: string, speakers: Speaker[]): string {
  const named = speakers.find((s) => s.label === label)?.display_name;
  if (named) return named;
  // Canaux par défaut : « Moi » (micro) et « Eux » (appel).
  if (label === 'Moi') return t('Moi');
  if (label === 'Eux') return t('Eux');
  // Intervenants reconnus à la voix : « Eux-3 » → « Intervenant 3 ».
  const n = /^Eux-(\d+)$/.exec(label);
  return n ? t('Intervenant {n}', { n: n[1] }) : label;
}

/** Initiale affichée dans la pastille d'un intervenant. */
export function initial(name: string): string {
  const m = name.trim().match(/\p{L}|\p{N}/u);
  return (m?.[0] ?? '?').toUpperCase();
}
