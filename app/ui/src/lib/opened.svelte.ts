// Réunions récemment ouvertes (palette ⌘K, « Récemment ouvert »), mémorisées localement.
import { pushRecent } from './palette';

const KEY = 'scribe.palette.recent';

function load(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? '[]');
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string').slice(0, 8) : [];
  } catch {
    return [];
  }
}

export const opened = $state<{ ids: string[] }>({ ids: load() });

export function markOpened(id: string): void {
  if (opened.ids[0] === id) return;
  opened.ids = pushRecent(opened.ids, id);
  try {
    localStorage.setItem(KEY, JSON.stringify(opened.ids));
  } catch {
    // Stockage indisponible : la liste vit le temps de la session.
  }
}
