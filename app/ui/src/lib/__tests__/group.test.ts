import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { groupByDay, groupHits, isOverdue } from '../group';
import type { MeetingSummary, SearchHit } from '../types';

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-11-26T15:00:00+01:00'));
});
afterEach(() => vi.useRealTimers());

const m = (id: string, started_at: string): MeetingSummary => ({
  id, title: id, title_source: null, started_at, ended_at: null, status: 'done', confidential: false,
  agent_provider: null, correlation: 'on_demand', correlated: false, open_actions: 0, segment_count: 0, snippet: null,
});

describe('groupByDay', () => {
  it('regroupe les réunions consécutives du même jour', () => {
    const g = groupByDay([
      m('a', '2026-11-26T14:00:00+01:00'),
      m('b', '2026-11-26T09:30:00+01:00'),
      m('c', '2026-11-25T11:00:00+01:00'),
      m('d', '2026-11-20T11:00:00+01:00'),
    ]);
    expect(g.map((x) => [x.day, x.items.map((i) => i.id)])).toEqual([
      ["Aujourd'hui", ['a', 'b']],
      ['Hier', ['c']],
      ['Vendredi 20 novembre', ['d']],
    ]);
  });
  it('liste vide', () => {
    expect(groupByDay([])).toEqual([]);
  });
});

describe('groupHits', () => {
  it('regroupe par réunion dans l’ordre d’arrivée', () => {
    const h = (meeting_id: string, snippet: string): SearchHit => ({ meeting_id, title: meeting_id, started_at: '2026-11-26T10:00:00+01:00', kind: 'segment', snippet, ts: null });
    const g = groupHits([h('x', '1'), h('y', '2'), h('x', '3')]);
    expect(g.map((x) => [x.id, x.hits.map((i) => i.snippet)])).toEqual([['x', ['1', '3']], ['y', ['2']]]);
  });
});

describe('isOverdue', () => {
  it('en retard si ouverte et échéance passée', () => {
    expect(isOverdue({ status: 'open', due: '2026-11-25' })).toBe(true);
    expect(isOverdue({ status: 'open', due: '2026-11-26' })).toBe(false);
    expect(isOverdue({ status: 'done', due: '2026-11-01' })).toBe(false);
  });
  it('une échéance libre (« vendredi ») ou absente n’est jamais en retard', () => {
    expect(isOverdue({ status: 'open', due: 'vendredi' })).toBe(false);
    expect(isOverdue({ status: 'open', due: null })).toBe(false);
  });
});
