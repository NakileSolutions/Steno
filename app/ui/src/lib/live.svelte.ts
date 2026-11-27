// État « en direct » partagé (panneau de barre de menu, barre latérale, vue En direct).

import { api, onEvent } from './api';
import type { LiveState } from './types';
import { loadPrefs } from './prefs.svelte';
import { normalizeLiveState } from './listening';

export const live = $state<{ s: LiveState; ready: boolean }>({
  s: {
    state: 'idle',
    meeting: null,
    levels: { me: -90, them: -90 },
    aside: false,
    sidecar: 'starting',
    last_error: null,
    pending_requests: 0,
    call: { active: false, app: null, app_name: null, title: null },
  },
  ready: false,
});

export const clock = $state({ now: Date.now() });

let started = false;
export function initLive(): void {
  if (started) return;
  started = true;
  // Un cœur plus ancien envoie `teams_call` au lieu de `call` : on le convertit.
  const set = (s: LiveState) => {
    live.s = normalizeLiveState(s);
    live.ready = true;
  };
  api.liveState().then(set).catch(() => {});
  onEvent('scribe://live', set);
  loadPrefs();
  setInterval(() => (clock.now = Date.now()), 1000);
}

export const STATE_LABEL: Record<LiveState['state'], string> = {
  idle: 'N’écoute pas',
  listening: 'Écoute',
  paused: 'En pause',
  processing: 'Traitement',
};
