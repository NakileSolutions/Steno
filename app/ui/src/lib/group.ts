// Regroupements et prédicats purs utilisés par les listes (réunions, recherche, actions).

import { dayLabel, isoDate } from './format';
import type { Action, MeetingSummary, SearchHit } from './types';

/** Regroupe des réunions (déjà triées) par jour : « Aujourd'hui », « Hier », « Lundi 24 novembre »… */
export function groupByDay(list: MeetingSummary[]): { day: string; items: MeetingSummary[] }[] {
  const out: { day: string; items: MeetingSummary[] }[] = [];
  for (const m of list) {
    const day = dayLabel(m.started_at);
    const last = out.at(-1);
    if (last && last.day === day) last.items.push(m);
    else out.push({ day, items: [m] });
  }
  return out;
}

export type HitGroup = { id: string; title: string | null; started_at: string; hits: SearchHit[] };

/** Regroupe les résultats de recherche par réunion, dans l'ordre d'arrivée. */
export function groupHits(hits: SearchHit[]): HitGroup[] {
  const map = new Map<string, HitGroup>();
  for (const h of hits) {
    let g = map.get(h.meeting_id);
    if (!g) map.set(h.meeting_id, (g = { id: h.meeting_id, title: h.title, started_at: h.started_at, hits: [] }));
    g.hits.push(h);
  }
  return [...map.values()];
}

/** Action ouverte dont l'échéance (date ISO) est passée. Une échéance libre (« vendredi ») n'est jamais en retard. */
export function isOverdue(a: Pick<Action, 'status' | 'due'>, today: Date = new Date()): boolean {
  return a.status === 'open' && !!a.due && /^\d{4}-\d{2}-\d{2}/.test(a.due) && a.due.slice(0, 10) < isoDate(today);
}

/**
 * Insère une phrase à sa place chronologique (début `t0`) : les deux canaux arrivent avec un
 * décalage, une longue phrase du micro après les réponses courtes de l'appel. Renvoie l'indice.
 */
export function insertChrono<T extends { t0: number | null }>(list: T[], item: T): number {
  if (item.t0 == null) {
    list.push(item);
    return list.length - 1;
  }
  let i = list.length;
  while (i > 0) {
    const prev = list[i - 1].t0;
    if (prev == null || prev <= item.t0) break;
    i--;
  }
  list.splice(i, 0, item);
  return i;
}
