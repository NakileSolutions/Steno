import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  PEEK_DELAY, PEEK_HIDE_DELAY, STORAGE_KEY, SidebarController, readCollapsed, writeCollapsed, type SidebarState,
} from '../sidebar';

const timers = { set: (fn: () => void, ms: number) => setTimeout(fn, ms), clear: (id: unknown) => clearTimeout(id as number) };

function make(collapsed = true) {
  const changes: SidebarState[] = [];
  const saved: boolean[] = [];
  const c = new SidebarController({ collapsed, timers, onChange: (s) => changes.push(s), persist: (v) => saved.push(v) });
  return { c, changes, saved };
}

describe('mémorisation', () => {
  it('lit et écrit « 1 » / « 0 »', () => {
    const store = new Map<string, string>();
    const s = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) };
    expect(readCollapsed(s)).toBe(false);
    writeCollapsed(s, true);
    expect(store.get(STORAGE_KEY)).toBe('1');
    expect(readCollapsed(s)).toBe(true);
    writeCollapsed(s, false);
    expect(readCollapsed(s)).toBe(false);
  });

  it('tolère un stockage absent ou qui lève', () => {
    const bad = { getItem: () => { throw new Error('refusé'); }, setItem: () => { throw new Error('refusé'); } };
    expect(readCollapsed(bad)).toBe(false);
    expect(() => writeCollapsed(bad, true)).not.toThrow();
    expect(readCollapsed(null)).toBe(false);
  });
});

describe('bascule', () => {
  it('déployée ⇄ réduite, mémorisé à chaque fois', () => {
    const { c, saved } = make(false);
    expect(c.mode).toBe('expanded');
    expect(c.visible).toBe(true);
    c.toggle();
    expect(c.mode).toBe('collapsed');
    expect(c.visible).toBe(false);
    c.toggle();
    expect(c.mode).toBe('expanded');
    expect(saved).toEqual([true, false]);
  });
});

describe('aperçu au survol', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('apparaît après le délai de survol du bord', () => {
    const { c } = make();
    c.edgeEnter();
    expect(c.peek).toBe('arming');
    vi.advanceTimersByTime(PEEK_DELAY - 1);
    expect(c.visible).toBe(false);
    vi.advanceTimersByTime(1);
    expect(c.peek).toBe('shown');
    expect(c.visible).toBe(true);
  });

  it('un passage rapide sur le bord ne fait rien', () => {
    const { c } = make();
    c.edgeEnter();
    vi.advanceTimersByTime(60);
    c.edgeLeave();
    vi.advanceTimersByTime(500);
    expect(c.peek).toBe('hidden');
  });

  it('reste ouvert tant que la souris est sur le panneau, se rétracte ensuite', () => {
    const { c } = make();
    c.edgeEnter();
    vi.advanceTimersByTime(PEEK_DELAY);
    c.edgeLeave();
    c.panelEnter();
    vi.advanceTimersByTime(2000);
    expect(c.peek).toBe('shown');
    c.panelLeave();
    vi.advanceTimersByTime(PEEK_HIDE_DELAY - 1);
    expect(c.peek).toBe('shown');
    vi.advanceTimersByTime(1);
    expect(c.peek).toBe('hidden');
  });

  it('revenir sur le panneau annule la rétraction', () => {
    const { c } = make();
    c.edgeEnter();
    vi.advanceTimersByTime(PEEK_DELAY);
    c.edgeLeave();
    vi.advanceTimersByTime(100);
    c.panelEnter();
    vi.advanceTimersByTime(1000);
    expect(c.peek).toBe('shown');
  });

  it('le focus clavier retient le panneau', () => {
    const { c } = make();
    c.edgeEnter();
    vi.advanceTimersByTime(PEEK_DELAY);
    c.hold(true);
    c.edgeLeave();
    vi.advanceTimersByTime(2000);
    expect(c.peek).toBe('shown');
    c.hold(false);
    vi.advanceTimersByTime(PEEK_HIDE_DELAY);
    expect(c.peek).toBe('hidden');
  });

  it('naviguer referme l’aperçu ; épingler redéploie', () => {
    const { c, saved } = make();
    c.show();
    expect(c.peek).toBe('shown');
    c.dismiss();
    expect(c.peek).toBe('hidden');
    expect(c.mode).toBe('collapsed');
    c.show();
    c.pin();
    expect(c.mode).toBe('expanded');
    expect(c.peek).toBe('hidden');
    expect(saved).toEqual([false]);
  });

  it('aucun aperçu quand la barre est déployée', () => {
    const { c, changes } = make(false);
    c.edgeEnter();
    vi.advanceTimersByTime(1000);
    c.show();
    expect(c.peek).toBe('hidden');
    expect(changes).toEqual([]);
  });
});
