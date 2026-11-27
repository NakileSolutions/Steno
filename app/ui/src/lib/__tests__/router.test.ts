import { beforeEach, describe, expect, it } from 'vitest';
import { href, navigate, parseHash, router, setQuery } from '../router.svelte';

describe('parseHash', () => {
  it('va sur les réunions par défaut', () => {
    expect(parseHash('')).toEqual({ path: '/meetings', parts: ['meetings'], query: {} });
    expect(parseHash('#')).toEqual({ path: '/meetings', parts: ['meetings'], query: {} });
  });

  it('découpe chemin et paramètres', () => {
    expect(parseHash('#/meetings/daily-orion?tab=requests&request=req-1')).toEqual({
      path: '/meetings/daily-orion',
      parts: ['meetings', 'daily-orion'],
      query: { tab: 'requests', request: 'req-1' },
    });
  });

  it('ajoute la barre initiale et décode', () => {
    expect(parseHash('#search?q=ms%20booking').query.q).toBe('ms booking');
    expect(parseHash('#settings').path).toBe('/settings');
    expect(parseHash('#/meetings/r%C3%A9union').parts[1]).toBe('réunion');
  });

  it('tolère un hash mal encodé', () => {
    expect(parseHash('#/search?q=100%').path).toBe('/search');
  });
});

describe('navigation', () => {
  beforeEach(() => navigate('/meetings', { replace: true }));

  it('href préfixe par #/', () => {
    expect(href('/live')).toBe('#/live');
    expect(href('live')).toBe('#/live');
  });

  it('navigate met à jour le hash et la route', () => {
    navigate('/meetings/abc?tab=transcript');
    expect(location.hash).toBe('#/meetings/abc?tab=transcript');
    expect(router.route.parts).toEqual(['meetings', 'abc']);
    expect(router.route.query.tab).toBe('transcript');
  });

  it('navigate avec replace n’ajoute pas d’entrée d’historique', () => {
    const before = history.length;
    navigate('/search', { replace: true });
    expect(history.length).toBe(before);
    expect(router.route.path).toBe('/search');
  });

  it('setQuery ajoute, modifie et retire des paramètres', () => {
    navigate('/meetings/abc?tab=report');
    setQuery({ tab: 'requests', request: 'r1' });
    expect(router.route.query).toEqual({ tab: 'requests', request: 'r1' });
    setQuery({ request: null });
    expect(router.route.query).toEqual({ tab: 'requests' });
    expect(location.hash).toBe('#/meetings/abc?tab=requests');
    setQuery({ tab: null });
    expect(location.hash).toBe('#/meetings/abc');
  });

  it('suit les changements de hash faits ailleurs', async () => {
    location.hash = '#/actions';
    await new Promise((r) => setTimeout(r, 0));
    expect(router.route.path).toBe('/actions');
  });
});
