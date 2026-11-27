// État « en direct » partagé (panneau de barre de menu, barre latérale, vue En direct).

import { api, onEvent } from './api';
import type { LiveState } from './types';
import { loadPrefs } from './prefs.svelte';

export const live = $state<{ s: LiveState; ready: boolean }>({
  s: {
    state: 'idle',
    meeting: null,
    levels: { me: -90, them: -90 },
    aside: false,
    sidecar: 'starting',
    last_error: null,
    pending_requests: 0,
    teams_call: { active: false, title: null },
  },
  ready: false,
});

export const clock = $state({ now: Date.now() });

let started = false;
export function initLive(): void {
  if (started) return;
  started = true;
  // Un backend plus ancien peut omettre `teams_call` : on le complète.
  const set = (s: LiveState) => {
    live.s = { ...s, teams_call: s.teams_call ?? { active: false, title: null } };
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
