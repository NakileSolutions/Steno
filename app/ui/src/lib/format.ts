// Formatage des dates, durées et libellés (langue de l'interface).

import { locale, t, tk } from './i18n.svelte';

/** Formateurs `Intl` mis en cache par locale et par options. */
const cache = new Map<string, Intl.DateTimeFormat>();
const dtf = (o: Intl.DateTimeFormatOptions) => {
  const k = locale() + JSON.stringify(o);
  let f = cache.get(k);
  if (!f) cache.set(k, (f = new Intl.DateTimeFormat(locale(), o)));
  return f;
};
const O_TIME: Intl.DateTimeFormatOptions = { hour: '2-digit', minute: '2-digit' };
const O_TIME_S: Intl.DateTimeFormatOptions = { hour: '2-digit', minute: '2-digit', second: '2-digit' };
const O_DAY: Intl.DateTimeFormatOptions = { weekday: 'long', day: 'numeric', month: 'long' };
const O_SHORT: Intl.DateTimeFormatOptions = { day: 'numeric', month: 'short' };
const O_FULL: Intl.DateTimeFormatOptions = { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' };

/** Formate une date ISO ; une valeur qui n'est pas une date (ex. « vendredi ») est rendue telle quelle. */
function safe(o: Intl.DateTimeFormatOptions, iso: string | null | undefined): string {
  if (!iso) return '';
  const d = new Date(iso);
  return Number.isFinite(d.getTime()) ? dtf(o).format(d) : String(iso);
}

export const time = (iso: string | null | undefined) => safe(O_TIME, iso);
export const timeS = (iso: string | null | undefined) => safe(O_TIME_S, iso);
export const shortDate = (iso: string) => safe(O_SHORT, iso);
export const fullDate = (iso: string) => cap(safe(O_FULL, iso));

export function cap(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** « Aujourd'hui », « Hier », « Lundi 24 novembre »… */
export function dayLabel(iso: string): string {
  const d = new Date(iso);
  if (!Number.isFinite(d.getTime())) return iso;
  const diff = Math.round((startOfDay(new Date()) - startOfDay(d)) / 86400000);
  if (diff === 0) return t("Aujourd'hui");
  if (diff === 1) return t('Hier');
  return cap(dtf(O_DAY).format(d));
}

export function relDay(iso: string): string {
  const d = new Date(iso);
  if (!Number.isFinite(d.getTime())) return iso;
  const diff = Math.round((startOfDay(new Date()) - startOfDay(d)) / 86400000);
  if (diff === 0) return time(iso);
  if (diff === 1) return t('Hier');
  if (diff < 7) return cap(dtf({ weekday: 'short' }).format(d).replace('.', ''));
  return shortDate(iso);
}

export function duration(fromIso: string, toIso: string | null, now = Date.now()): string {
  const s = Math.max(0, Math.round(((toIso ? new Date(toIso).getTime() : now) - new Date(fromIso).getTime()) / 1000));
  return clockDuration(s, false);
}

/** Durée courte d'une tâche : « 8 s », « 1 min 36 s ». `null` si une borne manque. */
export function shortDuration(fromIso: string | null, toIso: string | null): string | null {
  if (!fromIso || !toIso) return null;
  const s = Math.round((new Date(toIso).getTime() - new Date(fromIso).getTime()) / 1000);
  if (!Number.isFinite(s) || s < 0) return null;
  if (s < 60) return t('{s} s', { s });
  const m = Math.floor(s / 60);
  return s % 60 ? t('{m} min {s} s', { m, s: s % 60 }) : t('{m} min', { m });
}

export function clockDuration(totalSeconds: number, withSeconds = true): string {
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = Math.floor(totalSeconds % 60);
  if (!withSeconds) return h ? t('{h} h {mm}', { h, mm: String(m).padStart(2, '0') }) : t('{m} min', { m });
  return (h ? `${h}:${String(m).padStart(2, '0')}` : String(m)) + ':' + String(s).padStart(2, '0');
}

/** Décalage mm:ss depuis le début de la réunion. */
export function offset(t0: number | null): string {
  if (t0 == null) return '';
  return clockDuration(Math.max(0, t0));
}

export const isoDate = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

/** Libellés (français, marqués `tk`) : à traduire à l'affichage avec `t(...)`. */
export const STATUS_LABEL: Record<string, string> = {
  queued: tk('En attente'),
  running: tk('En cours'),
  done: tk('Terminée'),
  error: tk('Erreur'),
  cancelled: tk('Annulée'),
  refused: tk('Refusée'),
};
export const STATUS_CLASS: Record<string, string> = {
  queued: '',
  running: 'warn',
  done: 'ok',
  error: 'err',
  cancelled: '',
  refused: 'err',
};
export const INTENT_LABEL: Record<string, string> = {
  search: tk('Recherche'),
  check: tk('Vérification'),
  note: tk('Note'),
  recap: tk('Récap'),
  cancel: tk('Annulation'),
  stop: tk('Arrêt de l’écoute'),
  history: tk('Historique'),
};
export const ORIGIN_LABEL: Record<string, string> = {
  voice: tk('à la voix'),
  text: tk('tapée'),
  hotkey: tk('raccourci'),
  cli: tk('CLI'),
};
export const MEETING_STATUS: Record<string, string> = {
  planned: tk('Prévue'),
  live: tk('En direct'),
  ended: tk('Terminée'),
  processing: tk('Traitement'),
  done: tk('Terminée'),
  error: tk('Erreur'),
};

export function debounce<A extends unknown[]>(fn: (...a: A) => void, ms: number): (...a: A) => void {
  let t: ReturnType<typeof setTimeout> | undefined;
  return (...a: A) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...a), ms);
  };
}

/** Échappe le texte puis surligne les termes de la requête. */
export function highlight(text: string, query: string): string {
  const esc = (s: string) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
  let out = esc(text);
  const terms = query.trim().split(/\s+/).filter((t) => t.length > 1).map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  if (!terms.length) return out;
  return out.replace(new RegExp(`(${terms.map(esc).join('|')})`, 'gi'), '<mark>$1</mark>');
}

const foldTerm = (s: string) => s.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();

/**
 * Extrait de recherche lisible : retire les marqueurs « » que le moteur plein texte place autour
 * des termes trouvés (on surligne nous-mêmes), la syntaxe markdown des comptes rendus (titres,
 * puces, gras, code) et normalise les points de suspension. Les guillemets d'origine autour
 * d'autre chose qu'un terme cherché sont conservés.
 */
export function cleanSnippet(snippet: string, query: string): string {
  const terms = query.trim().split(/\s+/).filter((t) => t.length > 1).map(foldTerm);
  return snippet
    .replace(/«\s?([^«»\s]+)\s?»/g, (all, w: string) => (terms.some((t) => foldTerm(w).startsWith(t)) ? w : all))
    .replace(/(^|\s)#{1,6}\s+/g, '$1')
    .replace(/^[-*]\s+/, '')
    .replace(/\s[-*]\s+(?=\S)/g, ' · ')
    .replace(/(\*\*|__|`)/g, '')
    .replace(/\.\.\./g, '…')
    .replace(/\s{2,}/g, ' ')
    .trim();
}

/** Message du backend sans pictogramme en tête (l'interface affiche déjà le sien). */
export function stripLeadingIcon(msg: string): string {
  return msg.replace(/^[\s⚠❌✅✓✕✖️]+/u, '');
}
