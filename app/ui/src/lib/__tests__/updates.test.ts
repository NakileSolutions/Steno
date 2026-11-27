import { describe, expect, it } from 'vitest';
import { updateBadge } from '../updates.svelte';
import type { UpdateStatus } from '../types';

const base: UpdateStatus = {
  current: '0.1.0', latest: '0.2.0', available: true, url: null, notes: null, published_at: null, checked_at: null,
  state: 'idle', message: null, source_dir: null, can_install: true, log_path: null,
};

describe('updateBadge', () => {
  it('signale une nouvelle version, l’installation puis le redémarrage', () => {
    expect(updateBadge(base)).toEqual({ kind: 'available', label: 'Sténo 0.2.0 est disponible' });
    expect(updateBadge({ ...base, state: 'installing' })?.kind).toBe('installing');
    expect(updateBadge({ ...base, state: 'installed' })).toEqual({ kind: 'installed', label: 'Redémarrer pour passer à 0.2.0' });
  });
  it('reste discret quand tout est à jour', () => {
    expect(updateBadge({ ...base, available: false })).toBeNull();
    expect(updateBadge(null)).toBeNull();
  });
});
