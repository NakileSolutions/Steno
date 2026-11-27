// Couleur et nom affiché par intervenant.
import type { Speaker } from '../types';

export function speakerColor(label: string): string {
  if (label === 'Moi') return 'var(--sp-me)';
  const n = parseInt(label.replace(/\D+/g, ''), 10);
  if (Number.isNaN(n)) return 'var(--sp-1)';
  return `var(--sp-${((n - 1) % 5) + 1})`;
}

export function speakerName(label: string, speakers: Speaker[]): string {
  return speakers.find((s) => s.label === label)?.display_name || label;
}

/** Initiale affichée dans la pastille d'un intervenant. */
export function initial(name: string): string {
  const m = name.trim().match(/\p{L}|\p{N}/u);
  return (m?.[0] ?? '?').toUpperCase();
}
