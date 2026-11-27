import { describe, expect, it } from 'vitest';
import { buildGroups, filterItems, flatten, matchScore, mergeById, moveIndex, normalize, pushRecent, type PaletteItem } from '../palette';

const it_ = (id: string, title: string, extra: Partial<PaletteItem> = {}): PaletteItem => ({ id, title, ...extra });

describe('normalize et matchScore', () => {
  it('ignore accents, casse et apostrophes', () => {
    expect(normalize('  Réglages  d’Écoute ')).toBe('reglages d ecoute');
    expect(matchScore('Démarrer l’écoute', 'demarrer ecoute')).toBeGreaterThan(0);
  });

  it('exige chaque mot et privilégie les débuts de mots', () => {
    expect(matchScore('Revue d’architecture — export Brindille', 'brindille')).toBeGreaterThan(0);
    expect(matchScore('Revue d’architecture', 'brindille')).toBe(0);
    expect(matchScore('Réglages', 'reg')).toBeGreaterThan(matchScore('Aller aux réglages', 'reg'));
    expect(matchScore('Aller aux réglages', 'reg')).toBeGreaterThan(matchScore('Progrès', 'reg'));
  });
});

describe('filterItems', () => {
  const items = [
    it_('a', 'Aller à Réunions'),
    it_('b', 'Réglages › Voix', { keywords: 'mot d’appel wake' }),
    it_('c', 'Démarrer l’écoute'),
  ];
  it('requête vide : tout, dans l’ordre', () => {
    expect(filterItems(items, '').map((x) => x.id)).toEqual(['a', 'b', 'c']);
  });
  it('cherche aussi dans les mots-clés', () => {
    expect(filterItems(items, 'wake').map((x) => x.id)).toEqual(['b']);
  });
  it('trie par pertinence', () => {
    expect(filterItems(items, 're').map((x) => x.id)).toEqual(['b', 'a', 'c']);
  });
});

describe('buildGroups', () => {
  const recent = [it_('m1', 'Daily ORION'), it_('m2', 'Revue d’architecture — export Brindille')];
  const meetings = [it_('m2', 'Revue d’architecture — export Brindille'), it_('m3', 'Atelier Brindille')];
  const actions = [it_('go-search', 'Aller à Recherche'), it_('start', 'Démarrer l’écoute')];

  it('sans requête : récentes puis actions, pas de réunions', () => {
    const g = buildGroups({ query: '', recent, meetings, actions });
    expect(g.map((x) => x.label)).toEqual(['Récemment ouvert', 'Actions']);
    expect(g[0].items).toHaveLength(2);
  });

  it('avec une requête : groupes filtrés, sans doublon, vides omis', () => {
    const g = buildGroups({ query: 'brindille', recent, meetings, actions });
    expect(g.map((x) => x.id)).toEqual(['recent', 'meetings']);
    expect(g[0].items.map((x) => x.id)).toEqual(['m2']);
    expect(g[1].items.map((x) => x.id)).toEqual(['m3']);
    expect(flatten(g).map((x) => x.id)).toEqual(['m2', 'm3']);
  });

  it('limite la taille des groupes', () => {
    const many = Array.from({ length: 30 }, (_, i) => it_(`a${i}`, `Action ${i}`));
    const g = buildGroups({ query: 'action', recent: [], meetings: [], actions: many, limits: { actions: 5 } });
    expect(g[0].items).toHaveLength(5);
  });
});

describe('navigation et récents', () => {
  it('moveIndex boucle', () => {
    expect(moveIndex(0, 1, 3)).toBe(1);
    expect(moveIndex(2, 1, 3)).toBe(0);
    expect(moveIndex(0, -1, 3)).toBe(2);
    expect(moveIndex(-1, 1, 3)).toBe(0);
    expect(moveIndex(-1, -1, 3)).toBe(2);
    expect(moveIndex(0, 1, 0)).toBe(-1);
  });

  it('pushRecent met en tête sans doublon et limite', () => {
    expect(pushRecent(['a', 'b', 'c'], 'b')).toEqual(['b', 'a', 'c']);
    expect(pushRecent(['a', 'b'], 'z', 2)).toEqual(['z', 'a']);
  });

  it('mergeById garde le premier', () => {
    expect(mergeById([{ id: 'a', n: 1 }], [{ id: 'a', n: 2 }, { id: 'b', n: 3 }])).toEqual([{ id: 'a', n: 1 }, { id: 'b', n: 3 }]);
  });
});
