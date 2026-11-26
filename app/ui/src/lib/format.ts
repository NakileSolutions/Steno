// Formatage des dates, durées et libellés (français).

const dtf = (o: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat('fr-FR', o);
const fTime = dtf({ hour: '2-digit', minute: '2-digit' });
const fTimeS = dtf({ hour: '2-digit', minute: '2-digit', second: '2-digit' });
const fDay = dtf({ weekday: 'long', day: 'numeric', month: 'long' });
const fShort = dtf({ day: 'numeric', month: 'short' });
const fFull = dtf({ weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });

/** Formate une date ISO ; une valeur qui n'est pas une date (ex. « vendredi ») est rendue telle quelle. */
function safe(f: Intl.DateTimeFormat, iso: string | null | undefined): string {
  if (!iso) return '';
  const d = new Date(iso);
  return Number.isFinite(d.getTime()) ? f.format(d) : String(iso);
}

export const time = (iso: string | null | undefined) => safe(fTime, iso);
export const timeS = (iso: string | null | undefined) => safe(fTimeS, iso);
export const shortDate = (iso: string) => safe(fShort, iso);
export const fullDate = (iso: string) => cap(safe(fFull, iso));

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
  if (diff === 0) return "Aujourd'hui";
  if (diff === 1) return 'Hier';
  return cap(fDay.format(d));
}

export function relDay(iso: string): string {
  const d = new Date(iso);
  if (!Number.isFinite(d.getTime())) return iso;
  const diff = Math.round((startOfDay(new Date()) - startOfDay(d)) / 86400000);
  if (diff === 0) return time(iso);
  if (diff === 1) return 'Hier';
  if (diff < 7) return cap(dtf({ weekday: 'short' }).format(d).replace('.', ''));
  return shortDate(iso);
}

export function duration(fromIso: string, toIso: string | null, now = Date.now()): string {
  const s = Math.max(0, Math.round(((toIso ? new Date(toIso).getTime() : now) - new Date(fromIso).getTime()) / 1000));
  return clockDuration(s, false);
}

export function clockDuration(totalSeconds: number, withSeconds = true): string {
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = Math.floor(totalSeconds % 60);
  if (!withSeconds) return h ? `${h} h ${String(m).padStart(2, '0')}` : `${m} min`;
  return (h ? `${h}:${String(m).padStart(2, '0')}` : String(m)) + ':' + String(s).padStart(2, '0');
}

/** Décalage mm:ss depuis le début de la réunion. */
export function offset(t0: number | null): string {
  if (t0 == null) return '';
  return clockDuration(Math.max(0, t0));
}

export const isoDate = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

export const STATUS_LABEL: Record<string, string> = {
  queued: 'En attente',
  running: 'En cours',
  done: 'Terminée',
  error: 'Erreur',
  cancelled: 'Annulée',
  refused: 'Refusée',
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
  search: 'Recherche',
  check: 'Vérification',
  note: 'Note',
  recap: 'Récap',
  cancel: 'Annulation',
  history: 'Historique',
};
export const ORIGIN_LABEL: Record<string, string> = {
  voice: 'à la voix',
  text: 'tapée',
  hotkey: 'raccourci',
  cli: 'CLI',
};
export const MEETING_STATUS: Record<string, string> = {
  live: 'En direct',
  ended: 'Terminée',
  processing: 'Traitement',
  done: 'Terminée',
  error: 'Erreur',
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
